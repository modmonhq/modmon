//! The user services mm installs on this machine, through systemd on Linux
//! and launchd on macOS, and this user's `mm` processes, through `ps`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;

use crate::adapters::launchd::current_uid;
use crate::app::ports::{CommandOutput, CommandSpec, LocalUserServices, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::runtime::PODMAN_INHERITED_ENVIRONMENT;
use crate::domain::target_ingress::{
    LAUNCHD_DASHBOARD_LABEL, LAUNCHD_FORWARDER_LABEL, LAUNCHD_INGRESS_LABEL,
    MACOS_LAUNCH_AGENTS_DIRECTORY,
};
use crate::domain::update_steps::{
    LocalUserService, McpServerProcess, ServiceManager, elapsed_seconds, is_mcp_server,
    plist_program, unit_program,
};

const SYSTEMCTL: &str = "/usr/bin/systemctl";
const LAUNCHCTL: &str = "/bin/launchctl";
const PS: &str = "ps";
const SYSTEMD_UNITS: [&str; 2] = ["mm-dashboard.service", "mm-ingress.service"];
const LAUNCHD_LABELS: [&str; 3] = [
    LAUNCHD_DASHBOARD_LABEL,
    LAUNCHD_INGRESS_LABEL,
    LAUNCHD_FORWARDER_LABEL,
];
const MAX_DEFINITION_BYTES: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(60);
const CLOCK_SLACK: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct UserServiceManager {
    process: Arc<dyn ProcessRunner>,
    home: PathBuf,
    manager: ServiceManager,
    uid: u32,
}

impl std::fmt::Debug for UserServiceManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserServiceManager")
            .field("home", &self.home)
            .field("manager", &self.manager)
            .finish_non_exhaustive()
    }
}

struct MmProcess {
    pid: u32,
    started: Option<SystemTime>,
    arguments: Vec<String>,
}

impl UserServiceManager {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        let home = directories::BaseDirs::new()
            .map(|directories| directories.home_dir().to_path_buf())
            .ok_or_else(|| MmError::State {
                message: "could not resolve the current user's home directory".to_owned(),
            })?;
        let manager = if cfg!(target_os = "macos") {
            ServiceManager::Launchd
        } else {
            ServiceManager::Systemd
        };
        Ok(Self::with_home(process, home, manager, current_uid()))
    }

    #[must_use]
    pub fn with_home(
        process: Arc<dyn ProcessRunner>,
        home: PathBuf,
        manager: ServiceManager,
        uid: u32,
    ) -> Self {
        Self {
            process,
            home,
            manager,
            uid,
        }
    }

    fn definitions(&self) -> Vec<(String, PathBuf)> {
        match self.manager {
            ServiceManager::Systemd => SYSTEMD_UNITS
                .iter()
                .map(|unit| {
                    (
                        (*unit).to_owned(),
                        self.home.join(".config/systemd/user").join(unit),
                    )
                })
                .collect(),
            ServiceManager::Launchd => LAUNCHD_LABELS
                .iter()
                .map(|label| {
                    (
                        (*label).to_owned(),
                        self.home
                            .join(MACOS_LAUNCH_AGENTS_DIRECTORY)
                            .join(format!("{label}.plist")),
                    )
                })
                .collect(),
        }
    }

    async fn run(&self, program: &str, arguments: &[&str]) -> Result<CommandOutput, MmError> {
        self.process
            .run(
                CommandSpec::new(program)
                    .args(arguments.iter().copied())
                    .inherit_environment(PODMAN_INHERITED_ENVIRONMENT)
                    .timeout(TIMEOUT),
            )
            .await
    }

    fn launchd_service(&self, label: &str) -> String {
        format!("gui/{}/{label}", self.uid)
    }

    async fn state(&self, name: &str) -> Result<(bool, Option<u32>), MmError> {
        match self.manager {
            ServiceManager::Systemd => {
                let running = self
                    .run(SYSTEMCTL, &["--user", "is-active", "--quiet", name])
                    .await?
                    .success();
                let shown = self
                    .run(
                        SYSTEMCTL,
                        &["--user", "show", "-p", "MainPID", "--value", name],
                    )
                    .await?;
                let pid = shown
                    .success()
                    .then(|| shown.stdout.trim().parse::<u32>().ok())
                    .flatten()
                    .filter(|pid| *pid > 0);
                Ok((running, pid))
            }
            ServiceManager::Launchd => {
                let service = self.launchd_service(name);
                let printed = self.run(LAUNCHCTL, &["print", &service]).await?;
                if !printed.success() {
                    return Ok((false, None));
                }
                let field = |key: &str| {
                    printed.stdout.lines().find_map(|line| {
                        line.trim()
                            .strip_prefix(key)?
                            .strip_prefix(" = ")
                            .map(str::trim)
                    })
                };
                let running = field("state") == Some("running");
                let pid = field("pid").and_then(|pid| pid.parse().ok());
                Ok((running, pid))
            }
        }
    }

    async fn processes(
        &self,
        executable: &Path,
    ) -> Result<(Option<SystemTime>, Vec<MmProcess>), MmError> {
        let written = std::fs::metadata(executable)
            .and_then(|metadata| metadata.modified())
            .ok();
        let uid = self.uid.to_string();
        let output = self
            .run(PS, &["-U", &uid, "-o", "pid=,etime=,args="])
            .await?;
        if !output.success() {
            return Err(MmError::ProcessFailed {
                program: PS.to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            });
        }
        let now = SystemTime::now();
        let processes = output
            .stdout
            .lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                let pid = words.next()?.parse().ok()?;
                let elapsed = elapsed_seconds(words.next()?);
                Some(MmProcess {
                    pid,
                    started: elapsed
                        .and_then(|seconds| now.checked_sub(Duration::from_secs(seconds))),
                    arguments: words.map(str::to_owned).collect(),
                })
            })
            .collect();
        Ok((written, processes))
    }
}

fn older(started: Option<SystemTime>, written: Option<SystemTime>) -> Option<bool> {
    match (started, written) {
        (Some(started), Some(written)) => Some(started + CLOCK_SLACK < written),
        _ => None,
    }
}

fn read_definition(path: &Path) -> Result<Option<String>, MmError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("inspecting {}", path.display()),
                source,
            });
        }
    };
    if !metadata.is_file() || metadata.len() > MAX_DEFINITION_BYTES {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|source| MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        })
}

#[async_trait]
impl LocalUserServices for UserServiceManager {
    async fn services(&self, executable: &Path) -> Result<Vec<LocalUserService>, MmError> {
        let mut found = Vec::new();
        for (name, path) in self.definitions() {
            let Some(definition) = read_definition(&path)? else {
                continue;
            };
            let program = match self.manager {
                ServiceManager::Systemd => unit_program(&definition),
                ServiceManager::Launchd => plist_program(&definition),
            }
            .map(|program| std::fs::canonicalize(&program).unwrap_or(program));
            found.push((name, program));
        }
        if found.is_empty() {
            return Ok(Vec::new());
        }
        let (written, processes) = self
            .processes(executable)
            .await
            .unwrap_or((None, Vec::new()));
        let mut services = Vec::new();
        for (name, program) in found {
            let (running, pid) = self.state(&name).await?;
            let started = pid
                .and_then(|pid| processes.iter().find(|process| process.pid == pid))
                .map(|process| process.started);
            services.push(LocalUserService {
                name,
                manager: self.manager,
                running,
                program,
                older: started.and_then(|started| older(started, written)),
            });
        }
        Ok(services)
    }

    async fn restart(&self, service: &LocalUserService) -> Result<(), MmError> {
        let (program, output) = match service.manager {
            ServiceManager::Systemd => (
                SYSTEMCTL,
                self.run(SYSTEMCTL, &["--user", "restart", &service.name])
                    .await?,
            ),
            ServiceManager::Launchd => {
                let target = self.launchd_service(&service.name);
                (
                    LAUNCHCTL,
                    self.run(LAUNCHCTL, &["kickstart", "-k", &target]).await?,
                )
            }
        };
        if output.success() {
            Ok(())
        } else {
            Err(MmError::ProcessFailed {
                program: program.to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            })
        }
    }

    async fn running(&self, service: &LocalUserService) -> Result<bool, MmError> {
        self.state(&service.name).await.map(|(running, _)| running)
    }

    async fn mcp_servers(&self, executable: &Path) -> Result<Vec<McpServerProcess>, MmError> {
        let (written, processes) = self.processes(executable).await?;
        Ok(processes
            .into_iter()
            .filter(|process| {
                let arguments = process
                    .arguments
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                is_mcp_server(&arguments)
            })
            .map(|process| McpServerProcess {
                pid: process.pid,
                command: process.arguments.join(" "),
                older: older(process.started, written).unwrap_or(false),
            })
            .collect())
    }
}
