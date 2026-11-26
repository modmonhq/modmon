//! `mm update` past the binaries: which local user services restart, what
//! comes back, which `mm mcp serve` processes are left on the old release,
//! and which hosts are older. Services and hosts are fakes; no real user
//! service is touched.

mod temp_dirs;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use mm::adapters::local_services::UserServiceManager;
use mm::app::UpdateFollowUpService;
use mm::app::ports::{CommandOutput, CommandSpec, HostVersions, LocalUserServices, ProcessRunner};
use mm::domain::error::MmError;
use mm::domain::update_steps::{
    EnvironmentHosts, HostsView, LocalUserService, McpServerProcess, NodeVersion, ServiceManager,
    ServiceOutcome, elapsed_seconds, host_is_older, is_mcp_server, plist_program, unit_program,
    upgrade_commands,
};
use zeroize::Zeroizing;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MM: &str = "/usr/local/bin/mm";

#[derive(Default)]
struct FakeServices {
    services: Vec<LocalUserService>,
    failing: Vec<String>,
    down: Vec<String>,
    mcp: Vec<McpServerProcess>,
    restarted: Mutex<Vec<String>>,
}

#[async_trait]
impl LocalUserServices for FakeServices {
    async fn services(&self, _executable: &Path) -> Result<Vec<LocalUserService>, MmError> {
        Ok(self.services.clone())
    }

    async fn restart(&self, service: &LocalUserService) -> Result<(), MmError> {
        if self.failing.contains(&service.name) {
            return Err(MmError::ProcessFailed {
                program: "/usr/bin/systemctl".to_owned(),
                code: Some(1),
                stderr: "Failed to connect to bus".to_owned(),
            });
        }
        self.restarted
            .lock()
            .map_err(|_| MmError::State {
                message: "poisoned".to_owned(),
            })?
            .push(service.name.clone());
        Ok(())
    }

    async fn running(&self, service: &LocalUserService) -> Result<bool, MmError> {
        Ok(!self.down.contains(&service.name))
    }

    async fn mcp_servers(&self, _executable: &Path) -> Result<Vec<McpServerProcess>, MmError> {
        Ok(self.mcp.clone())
    }
}

struct FakeHosts(HostsView);

#[async_trait]
impl HostVersions for FakeHosts {
    async fn hosts(&self, _version: &str) -> HostsView {
        self.0.clone()
    }
}

fn service(name: &str, program: &str, running: bool, older: Option<bool>) -> LocalUserService {
    LocalUserService {
        name: name.to_owned(),
        manager: ServiceManager::Systemd,
        running,
        program: Some(PathBuf::from(program)),
        older,
    }
}

fn steps(services: Arc<FakeServices>, hosts: HostsView) -> UpdateFollowUpService {
    UpdateFollowUpService::new(services, Arc::new(FakeHosts(hosts))).with_checks(Duration::ZERO, 4)
}

fn production() -> EnvironmentHosts {
    let (upgrade, note) = upgrade_commands("production", true);
    EnvironmentHosts {
        environment: "production".to_owned(),
        target: "fleet".to_owned(),
        cluster: true,
        nodes: vec![NodeVersion {
            name: "app-1".to_owned(),
            version: Some("0.1.1".to_owned()),
            older: true,
            problem: None,
        }],
        problem: None,
        upgrade,
        note: Some(note),
    }
}

#[tokio::test]
async fn an_update_restarts_every_running_service_that_starts_this_mm() -> TestResult {
    let services = Arc::new(FakeServices {
        services: vec![
            service("mm-dashboard.service", MM, true, Some(false)),
            service("mm-ingress.service", "/usr/bin/podman", true, None),
            service("local.mm.other", "/opt/mm/bin/mm", true, Some(true)),
            service("mm-stopped.service", MM, false, None),
        ],
        mcp: vec![McpServerProcess {
            pid: 4242,
            command: "mm mcp serve".to_owned(),
            older: true,
        }],
        ..FakeServices::default()
    });
    let hosts = HostsView::Project {
        environments: vec![production()],
    };
    let steps = steps(Arc::clone(&services), hosts.clone());
    let plan = steps.plan(Path::new(MM), "0.1.2", true).await?;
    assert_eq!(plan.restart, ["mm-dashboard.service"]);
    let summary = steps.apply(&plan, Path::new(MM), "0.1.2").await;
    assert_eq!(
        *services.restarted.lock().map_err(|_| "poisoned")?,
        ["mm-dashboard.service"]
    );
    let outcomes = summary
        .services
        .iter()
        .map(|report| {
            (
                report.name.as_str(),
                report.outcome.clone(),
                report.version.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes,
        [
            (
                "mm-dashboard.service",
                ServiceOutcome::Restarted,
                Some("0.1.2".to_owned())
            ),
            ("mm-ingress.service", ServiceOutcome::NotMm, None),
            ("local.mm.other", ServiceOutcome::OtherMm, None),
            ("mm-stopped.service", ServiceOutcome::Stopped, None),
        ]
    );
    assert_eq!(summary.services[2].next, ["/opt/mm/bin/mm update"]);
    assert_eq!(summary.mcp_servers[0].pid, 4242);
    assert_eq!(summary.hosts, hosts);
    assert_eq!(summary.version, "0.1.2");
    Ok(())
}

#[tokio::test]
async fn a_service_that_does_not_come_back_names_the_next_command() -> TestResult {
    let services = Arc::new(FakeServices {
        services: vec![
            service("mm-dashboard.service", MM, true, Some(false)),
            service("mm-other.service", MM, true, Some(false)),
        ],
        failing: vec!["mm-other.service".to_owned()],
        down: vec!["mm-dashboard.service".to_owned()],
        ..FakeServices::default()
    });
    let steps = steps(
        services,
        HostsView::NoProject {
            note: "none".to_owned(),
        },
    );
    let plan = steps.plan(Path::new(MM), "0.1.2", true).await?;
    let summary = steps.apply(&plan, Path::new(MM), "0.1.2").await;
    assert_eq!(summary.services[0].outcome, ServiceOutcome::Unhealthy);
    assert_eq!(
        summary.services[0].next,
        [
            "systemctl --user status mm-dashboard.service",
            "journalctl --user -u mm-dashboard.service -n 50",
        ]
    );
    assert!(matches!(
        &summary.services[1].outcome,
        ServiceOutcome::RestartFailed { problem } if problem.contains("Failed to connect to bus")
    ));
    assert_eq!(
        summary.services[1].next,
        ["systemctl --user restart mm-other.service"]
    );
    Ok(())
}

#[tokio::test]
async fn without_a_new_mm_only_services_older_than_it_restart() -> TestResult {
    let services = Arc::new(FakeServices {
        services: vec![
            service("mm-dashboard.service", MM, true, Some(true)),
            service("mm-fresh.service", MM, true, Some(false)),
            service("mm-unknown.service", MM, true, None),
        ],
        ..FakeServices::default()
    });
    let steps = steps(
        Arc::clone(&services),
        HostsView::NoProject {
            note: "none".to_owned(),
        },
    );
    let plan = steps.plan(Path::new(MM), "0.1.2", false).await?;
    assert_eq!(plan.restart, ["mm-dashboard.service"]);
    let summary = steps.apply(&plan, Path::new(MM), "0.1.2").await;
    assert_eq!(summary.services[1].outcome, ServiceOutcome::Current);
    assert_eq!(summary.services[1].version.as_deref(), Some("0.1.2"));
    Ok(())
}

#[test]
fn units_plists_process_listings_and_versions_are_read_as_mm_writes_them() {
    assert_eq!(
        unit_program(
            "[Service]\nType=simple\nExecStart=/usr/local/bin/mm --mm-home /var/lib/mm-home dev dashboard serve\n"
        ),
        Some(PathBuf::from(MM))
    );
    assert_eq!(
        unit_program("[Service]\nExecStart=-/usr/bin/podman compose up -d\n"),
        Some(PathBuf::from("/usr/bin/podman"))
    );
    assert_eq!(
        plist_program(
            "<dict>\n  <key>ProgramArguments</key>\n  <array>\n    <string>/opt/homebrew/bin/mm</string>\n    <string>dev</string>\n"
        ),
        Some(PathBuf::from("/opt/homebrew/bin/mm"))
    );
    assert_eq!(elapsed_seconds("05:07"), Some(307));
    assert_eq!(elapsed_seconds("02:00:01"), Some(7201));
    assert_eq!(elapsed_seconds("1-00:00:00"), Some(86_400));
    assert_eq!(elapsed_seconds("soon"), None);
    assert!(is_mcp_server(&["mm", "mcp", "serve"]));
    assert!(is_mcp_server(&[MM, "--mm-home", "/tmp/x", "mcp", "serve"]));
    assert!(!is_mcp_server(&["node", "mcp", "serve"]));
    assert!(!is_mcp_server(&["mm", "mcp", "config"]));
    assert!(host_is_older("0.1.1", "0.1.2"));
    assert!(!host_is_older("0.1.2", "0.1.2"));
    assert!(!host_is_older("0.1.3", "0.1.2"));
    assert!(!host_is_older("local-build", "0.1.2"));
    let (cluster, note) = upgrade_commands("production", true);
    assert_eq!(
        cluster,
        ["mm --environment production target setup plan --upgrade"]
    );
    assert!(note.contains("apply command that plan prints"), "{note}");
    let (_, single) = upgrade_commands("staging", false);
    assert!(single.contains("--break-glass"), "{single}");
}

/// A host's `systemctl`, `launchctl` and `ps`, with canned answers, that
/// records every command.
struct Host {
    answers: BTreeMap<String, (i32, String)>,
    commands: Mutex<Vec<String>>,
}

impl Host {
    fn new(answers: &[(&str, i32, &str)]) -> Self {
        Self {
            answers: answers
                .iter()
                .map(|(command, code, stdout)| {
                    ((*command).to_owned(), (*code, (*stdout).to_owned()))
                })
                .collect(),
            commands: Mutex::default(),
        }
    }

    fn commands(&self) -> Vec<String> {
        self.commands
            .lock()
            .map(|commands| commands.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl ProcessRunner for Host {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let record = specification.redacted_record();
        let command = format!("{} {}", record.program, record.arguments.join(" "));
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "poisoned".to_owned(),
            })?
            .push(command.clone());
        let (code, stdout) = self
            .answers
            .get(&command)
            .cloned()
            .unwrap_or((1, String::new()));
        Ok(CommandOutput {
            code: Some(code),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

/// An `mm` written an hour ago.
fn written_an_hour_ago(home: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mm = home.join("bin/mm");
    write(&mm, "#!/bin/sh\n")?;
    std::fs::File::options()
        .write(true)
        .open(&mm)?
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))?;
    Ok(std::fs::canonicalize(&mm)?)
}

#[tokio::test]
async fn systemd_user_units_are_read_restarted_and_compared_with_the_mm_on_disk() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let mm = written_an_hour_ago(home.path())?;
    write(
        &home
            .path()
            .join(".config/systemd/user/mm-dashboard.service"),
        &format!(
            "[Service]\nExecStart={} dev dashboard serve\n",
            mm.display()
        ),
    )?;
    write(
        &home.path().join(".config/systemd/user/mm-ingress.service"),
        "[Service]\nExecStart=/usr/bin/podman compose up -d\n",
    )?;
    let listing = format!(
        "  700 1-02:00:00 {mm} dev dashboard serve\n  800    00:03 {mm} mcp serve\n  900 3-00:00:00 mm --mm-home /tmp/h mcp serve\n  950    10:00 bash\n",
        mm = mm.display()
    );
    let host = Arc::new(Host::new(&[
        ("ps -U 1000 -o pid=,etime=,args=", 0, listing.as_str()),
        (
            "/usr/bin/systemctl --user is-active --quiet mm-dashboard.service",
            0,
            "",
        ),
        (
            "/usr/bin/systemctl --user show -p MainPID --value mm-dashboard.service",
            0,
            "700\n",
        ),
        (
            "/usr/bin/systemctl --user is-active --quiet mm-ingress.service",
            0,
            "",
        ),
        (
            "/usr/bin/systemctl --user show -p MainPID --value mm-ingress.service",
            0,
            "0\n",
        ),
        (
            "/usr/bin/systemctl --user restart mm-dashboard.service",
            0,
            "",
        ),
    ]));
    let manager = UserServiceManager::with_home(
        Arc::clone(&host) as Arc<dyn ProcessRunner>,
        home.path().to_path_buf(),
        ServiceManager::Systemd,
        1000,
    );
    let services = manager.services(&mm).await?;
    assert_eq!(services.len(), 2);
    assert_eq!(services[0].name, "mm-dashboard.service");
    assert_eq!(services[0].program.as_deref(), Some(mm.as_path()));
    assert!(services[0].running && services[0].runs_mm());
    assert_eq!(services[0].older, Some(true));
    assert_eq!(services[1].program, Some(PathBuf::from("/usr/bin/podman")));
    assert!(!services[1].runs_mm());
    assert_eq!(services[1].older, None);

    let servers = manager.mcp_servers(&mm).await?;
    let found = servers
        .iter()
        .map(|server| (server.pid, server.older))
        .collect::<Vec<_>>();
    assert_eq!(found, [(800, false), (900, true)]);

    manager.restart(&services[0]).await?;
    assert!(
        host.commands()
            .contains(&"/usr/bin/systemctl --user restart mm-dashboard.service".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn launchd_agents_are_read_and_kickstarted_in_the_users_domain() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let mm = written_an_hour_ago(home.path())?;
    write(
        &home
            .path()
            .join("Library/LaunchAgents/local.mm.dashboard.plist"),
        &format!(
            "<plist><dict><key>ProgramArguments</key>\n<array>\n<string>{}</string>\n<string>dev</string>\n</array></dict></plist>\n",
            mm.display()
        ),
    )?;
    let host = Arc::new(Host::new(&[
        (
            "ps -U 501 -o pid=,etime=,args=",
            0,
            "  42    00:01 mm dev dashboard serve\n",
        ),
        (
            "/bin/launchctl print gui/501/local.mm.dashboard",
            0,
            "gui/501/local.mm.dashboard = {\n\tstate = running\n\tpid = 42\n}\n",
        ),
        (
            "/bin/launchctl kickstart -k gui/501/local.mm.dashboard",
            0,
            "",
        ),
    ]));
    let manager = UserServiceManager::with_home(
        Arc::clone(&host) as Arc<dyn ProcessRunner>,
        home.path().to_path_buf(),
        ServiceManager::Launchd,
        501,
    );
    let services = manager.services(&mm).await?;
    assert_eq!(services.len(), 1);
    assert!(services[0].running && services[0].runs_mm());
    assert_eq!(services[0].older, Some(false));
    assert_eq!(
        services[0].restart_command(),
        "launchctl kickstart -k gui/$(id -u)/local.mm.dashboard"
    );
    manager.restart(&services[0]).await?;
    assert!(
        host.commands()
            .contains(&"/bin/launchctl kickstart -k gui/501/local.mm.dashboard".to_owned())
    );
    Ok(())
}

#[tokio::test]
async fn no_installed_unit_means_no_service_manager_is_asked() -> TestResult {
    let home = temp_dirs::tempdir()?;
    let host = Arc::new(Host::new(&[]));
    let manager = UserServiceManager::with_home(
        Arc::clone(&host) as Arc<dyn ProcessRunner>,
        home.path().to_path_buf(),
        ServiceManager::Systemd,
        1000,
    );
    assert!(manager.services(Path::new(MM)).await?.is_empty());
    assert!(host.commands().is_empty());
    Ok(())
}
