//! macOS ingress integration: user LaunchAgents for the Compose autostart and the
//! HAProxy forwarder. macOS lets unprivileged processes bind wildcard ports 80/443.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::adapters::launchd::{
    AgentFile, AgentInstallation, Launchctl, LaunchdJobState, ensure_log_directory, install_agent,
    uninstall_agent, with_cleanup,
};
use crate::adapters::private_fs::{ensure_user_controlled_file, read_regular_file};
use crate::app::haproxy_config;
use crate::app::ports::{
    CommandSpec, PrivilegedIngressInstaller, ProcessRunner, PublicIngressControl,
    RootlessIngressInstaller,
};
use crate::domain::error::MmError;
use crate::domain::host_ingress::{PublicIngressState, PublicIngressStatus};
use crate::domain::target_ingress::{
    LAUNCHD_FORWARDER_LABEL, LAUNCHD_INGRESS_LABEL, MACOS_AUTOSTART_SUPPORT_FILE,
    MACOS_COMPOSE_FILE, MACOS_FORWARDER_SUPPORT_FILE, MACOS_HAPROXY_FILE, MACOS_INGRESS_DIRECTORY,
    MACOS_LAUNCH_AGENTS_DIRECTORY, MACOS_LOG_DIRECTORY, MacosIngressHost, TargetIngressPlan,
};

const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
/// launchd reports a bootstrapped job as running within a few hundred milliseconds.
const START_WAIT: Duration = Duration::from_secs(5);
const START_POLL: Duration = Duration::from_millis(250);

fn current_home() -> Result<PathBuf, MmError> {
    directories::BaseDirs::new()
        .map(|directories| directories.home_dir().to_path_buf())
        .ok_or_else(|| MmError::State {
            message: "could not resolve the current user's home directory".to_owned(),
        })
}

fn forwarder_log_hint(home: &Path) -> String {
    format!(
        "inspect {}/{MACOS_LOG_DIRECTORY}/ingress-forwarder.log; another program may already use port 80 or 443 (check `lsof -nP -iTCP:80 -iTCP:443 -sTCP:LISTEN`)",
        home.display()
    )
}

/// Verifies a reviewed macOS plan and returns its host profile. The plan's
/// recorded home must be this user's, so a plan cannot write elsewhere.
fn verified_host<'a>(
    plan: &'a TargetIngressPlan,
    home: &Path,
) -> Result<&'a MacosIngressHost, MmError> {
    let manifest = &plan.artifact.manifest;
    let host = manifest.macos_host.as_ref().filter(|host| {
        plan.target_id == manifest.target_id
            && plan.plan_hash == manifest.artifact_hash
            && manifest.canonical_hash() == plan.plan_hash
            && Path::new(&host.home) == home
            && manifest.target_compose_path == host.ingress_path(MACOS_COMPOSE_FILE)
    });
    host.ok_or_else(|| MmError::Security {
        message: "refusing a non-canonical macOS ingress plan".to_owned(),
    })
}

/// Reads one support file whose manifest entry names the exact fixed destination.
fn verified_support(
    plan: &TargetIngressPlan,
    name: &str,
    destination: &str,
) -> Result<Vec<u8>, MmError> {
    let entry = plan
        .artifact
        .manifest
        .support_files
        .iter()
        .find(|file| file.name == name)
        .filter(|file| file.destination == destination && file.mode == "0644")
        .ok_or_else(|| MmError::Security {
            message: format!("macOS ingress plan has no canonical {name}"),
        })?;
    let path = plan
        .artifact
        .support_paths
        .iter()
        .find(|path| path.file_name().and_then(|file| file.to_str()) == Some(name))
        .ok_or_else(|| MmError::Security {
            message: format!("{name} is missing from the ingress artifact"),
        })?;
    verified_source(path, &entry.digest)
}

fn verified_source(path: &Path, digest: &str) -> Result<Vec<u8>, MmError> {
    ensure_user_controlled_file(path)?;
    let bytes = read_regular_file(path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| MmError::Security {
        message: format!("ingress artifact '{}' disappeared", path.display()),
    })?;
    if blake3::hash(&bytes).to_hex().as_str() != digest {
        return Err(MmError::Security {
            message: format!(
                "ingress artifact '{}' failed digest verification",
                path.display()
            ),
        });
    }
    Ok(bytes)
}

fn owner_path(home: &Path, label: &str) -> PathBuf {
    home.join(MACOS_INGRESS_DIRECTORY)
        .join(format!("{label}.owner.json"))
}

fn plist_path(home: &Path, label: &str) -> PathBuf {
    home.join(MACOS_LAUNCH_AGENTS_DIRECTORY)
        .join(format!("{label}.plist"))
}

/// Replays the verified Compose project at login, retrying until Docker Desktop is up.
#[derive(Clone)]
pub struct LaunchdAutostartInstaller {
    launchctl: Launchctl,
    home: PathBuf,
}

impl std::fmt::Debug for LaunchdAutostartInstaller {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchdAutostartInstaller")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl LaunchdAutostartInstaller {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        Ok(Self {
            launchctl: Launchctl::current_user(process),
            home: current_home()?,
        })
    }

    #[cfg(test)]
    fn with_home(process: Arc<dyn ProcessRunner>, home: PathBuf) -> Self {
        Self {
            launchctl: Launchctl::current_user(process),
            home,
        }
    }

    fn installation(&self, plan: &TargetIngressPlan) -> Result<AgentInstallation, MmError> {
        let host = verified_host(plan, &self.home)?;
        let plist = host.launch_agent_path(LAUNCHD_INGRESS_LABEL);
        Ok(AgentInstallation {
            label: LAUNCHD_INGRESS_LABEL.to_owned(),
            target_id: plan.target_id,
            artifact_hash: plan.plan_hash.clone(),
            owner_path: owner_path(&self.home, LAUNCHD_INGRESS_LABEL),
            plist: AgentFile {
                bytes: verified_support(plan, MACOS_AUTOSTART_SUPPORT_FILE, &plist)?,
                destination: PathBuf::from(plist),
                mode: 0o644,
            },
            files: vec![AgentFile {
                destination: PathBuf::from(host.ingress_path(MACOS_COMPOSE_FILE)),
                bytes: verified_source(
                    &plan.artifact.compose_path,
                    &plan.artifact.manifest.compose_digest,
                )?,
                mode: 0o600,
            }],
        })
    }
}

#[async_trait]
impl RootlessIngressInstaller for LaunchdAutostartInstaller {
    async fn install(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        let installation = self.installation(plan)?;
        ensure_log_directory(&self.home)?;
        install_agent(&self.launchctl, &installation)
            .await
            .map(drop)
    }

    async fn uninstall(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        uninstall_agent(&self.launchctl, &self.installation(plan)?).await
    }
}

/// HAProxy on ports 80/443 as a user agent; `KeepAlive` restarts it until bootout.
#[derive(Clone)]
pub struct LaunchdForwarderInstaller {
    process: Arc<dyn ProcessRunner>,
    launchctl: Launchctl,
    home: PathBuf,
    verify: fn() -> Result<(), MmError>,
}

impl std::fmt::Debug for LaunchdForwarderInstaller {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchdForwarderInstaller")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl LaunchdForwarderInstaller {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        Ok(Self {
            launchctl: Launchctl::current_user(Arc::clone(&process)),
            process,
            home: current_home()?,
            verify: super::privileged_ingress::verify_public_forwarder,
        })
    }

    #[cfg(test)]
    fn with_home(
        process: Arc<dyn ProcessRunner>,
        home: PathBuf,
        verify: fn() -> Result<(), MmError>,
    ) -> Self {
        Self {
            launchctl: Launchctl::current_user(Arc::clone(&process)),
            process,
            home,
            verify,
        }
    }

    fn installation(&self, plan: &TargetIngressPlan) -> Result<AgentInstallation, MmError> {
        let host = verified_host(plan, &self.home)?;
        let config_destination = host.ingress_path(MACOS_HAPROXY_FILE);
        let config = verified_support(plan, MACOS_HAPROXY_FILE, &config_destination)?;
        let manifest = &plan.artifact.manifest;
        if config != haproxy_config(manifest.proxy_http_port, manifest.proxy_https_port).as_bytes()
        {
            return Err(MmError::Security {
                message: "HAProxy configuration does not match mm's fixed template".to_owned(),
            });
        }
        let plist = host.launch_agent_path(LAUNCHD_FORWARDER_LABEL);
        Ok(AgentInstallation {
            label: LAUNCHD_FORWARDER_LABEL.to_owned(),
            target_id: plan.target_id,
            artifact_hash: plan.plan_hash.clone(),
            owner_path: owner_path(&self.home, LAUNCHD_FORWARDER_LABEL),
            plist: AgentFile {
                bytes: verified_support(plan, MACOS_FORWARDER_SUPPORT_FILE, &plist)?,
                destination: PathBuf::from(plist),
                mode: 0o644,
            },
            files: vec![AgentFile {
                destination: PathBuf::from(config_destination),
                bytes: config,
                mode: 0o644,
            }],
        })
    }

    async fn validate_config(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        let host = verified_host(plan, &self.home)?;
        let source = plan
            .artifact
            .support_paths
            .iter()
            .find(|path| {
                path.file_name().and_then(|name| name.to_str()) == Some(MACOS_HAPROXY_FILE)
            })
            .and_then(|path| path.to_str())
            .ok_or_else(|| MmError::Security {
                message: "HAProxy configuration is missing from the ingress artifact".to_owned(),
            })?;
        let output = self
            .process
            .run(
                CommandSpec::new(host.haproxy_executable.clone())
                    .args(["-c", "-f", source])
                    .timeout(Duration::from_secs(30)),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(MmError::ProcessFailed {
                program: host.haproxy_executable.clone(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            })
        }
    }
}

#[async_trait]
impl PrivilegedIngressInstaller for LaunchdForwarderInstaller {
    async fn install(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        let installation = self.installation(plan)?;
        self.validate_config(plan).await?;
        ensure_log_directory(&self.home)?;
        let snapshot = install_agent(&self.launchctl, &installation).await?;
        let verify = self.verify;
        let verified =
            tokio::task::spawn_blocking(verify)
                .await
                .map_err(|error| MmError::State {
                    message: format!("public ingress verification did not complete: {error}"),
                })?;
        match verified {
            Ok(()) => Ok(()),
            Err(error) => {
                let error = MmError::RuntimeResponse {
                    message: format!("{error}. {}", forwarder_log_hint(&self.home)),
                };
                Err(with_cleanup(
                    &plan.plan_hash,
                    error,
                    snapshot.rollback(&self.launchctl).await,
                    "restore the mm ingress forwarder agent, then retry",
                ))
            }
        }
    }

    async fn uninstall(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        uninstall_agent(&self.launchctl, &self.installation(plan)?).await
    }
}

/// Start/stop for the forwarder agent. Stop boots the job out (so `KeepAlive`
/// cannot relaunch it) but keeps the installed plist; start loads it again.
#[derive(Clone)]
pub struct LaunchdPublicIngressControl {
    launchctl: Launchctl,
    home: PathBuf,
}

impl std::fmt::Debug for LaunchdPublicIngressControl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LaunchdPublicIngressControl")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl LaunchdPublicIngressControl {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        Ok(Self {
            launchctl: Launchctl::current_user(process),
            home: current_home()?,
        })
    }

    #[cfg(test)]
    fn with_home(process: Arc<dyn ProcessRunner>, home: PathBuf) -> Self {
        Self {
            launchctl: Launchctl::current_user(process),
            home,
        }
    }

    fn installed(&self) -> bool {
        let owner = owner_path(&self.home, LAUNCHD_FORWARDER_LABEL);
        plist_path(&self.home, LAUNCHD_FORWARDER_LABEL).is_file()
            && crate::adapters::launchd::read_existing(&owner)
                .ok()
                .flatten()
                .is_some()
    }

    /// Two consecutive running observations: a forwarder that cannot bind exits
    /// at once and is relaunched by `KeepAlive`, so one sample can mislead.
    async fn wait_until_running(&self) -> Result<(), MmError> {
        let deadline = tokio::time::Instant::now() + START_WAIT;
        let mut previous_running = false;
        loop {
            match self.launchctl.state(LAUNCHD_FORWARDER_LABEL).await? {
                LaunchdJobState::Running if previous_running => return Ok(()),
                LaunchdJobState::Running => {
                    previous_running = true;
                    tokio::time::sleep(START_POLL).await;
                }
                state if tokio::time::Instant::now() >= deadline => {
                    return Err(MmError::RuntimeResponse {
                        message: format!(
                            "the ingress forwarder did not stay running ({state:?}); {}",
                            forwarder_log_hint(&self.home)
                        ),
                    });
                }
                _ => {
                    previous_running = false;
                    tokio::time::sleep(START_POLL).await;
                }
            }
        }
    }
}

#[async_trait]
impl PublicIngressControl for LaunchdPublicIngressControl {
    async fn inspect(&self) -> Result<PublicIngressStatus, MmError> {
        let installed = self.installed();
        let state = self.launchctl.state(LAUNCHD_FORWARDER_LABEL).await?;
        Ok(PublicIngressStatus {
            installed,
            running: installed && state == LaunchdJobState::Running,
            failed: installed && state.failed(),
        })
    }

    async fn set_state(
        &self,
        state: PublicIngressState,
        _allow_prompt: bool,
    ) -> Result<(), MmError> {
        match state {
            PublicIngressState::Stopped => self.launchctl.bootout(LAUNCHD_FORWARDER_LABEL).await,
            PublicIngressState::Running => {
                if !self.installed() {
                    return Err(MmError::Target {
                        message: "the ingress forwarder is not installed; run `mm ingress start` to set it up".to_owned(),
                    });
                }
                match self.launchctl.state(LAUNCHD_FORWARDER_LABEL).await? {
                    LaunchdJobState::Running => return Ok(()),
                    LaunchdJobState::Unloaded => {
                        if let Err(error) = self
                            .launchctl
                            .bootstrap(&plist_path(&self.home, LAUNCHD_FORWARDER_LABEL))
                            .await
                        {
                            if self.launchctl.disabled(LAUNCHD_FORWARDER_LABEL).await? {
                                return Err(MmError::Target {
                                    message: format!(
                                        "the ingress forwarder is disabled in launchd; run `launchctl enable gui/{}/{LAUNCHD_FORWARDER_LABEL}`, then retry",
                                        crate::adapters::launchd::current_uid()
                                    ),
                                });
                            }
                            return Err(error);
                        }
                    }
                    LaunchdJobState::Exited { .. } => {
                        self.launchctl.kickstart(LAUNCHD_FORWARDER_LABEL).await?;
                    }
                }
                // A forwarder that cannot bind would crash-loop under KeepAlive
                // and could take the ports later; leave it cleanly stopped.
                match self.wait_until_running().await {
                    Ok(()) => Ok(()),
                    Err(error) => Err(with_cleanup(
                        LAUNCHD_FORWARDER_LABEL,
                        error,
                        self.launchctl
                            .bootout(LAUNCHD_FORWARDER_LABEL)
                            .await
                            .err()
                            .map(|failure| vec![failure.to_string()])
                            .unwrap_or_default(),
                        "run `mm ingress stop` before retrying",
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "launchd_ingress/tests.rs"]
mod tests;
