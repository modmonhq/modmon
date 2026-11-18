use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_user_controlled_directory, ensure_user_controlled_file,
    read_regular_file,
};
use crate::app::ports::{
    CommandOutput, CommandSpec, DashboardServiceInstaller, DashboardServiceStatus, ProcessRunner,
};
use crate::app::render_dashboard_service_unit;
use crate::domain::error::MmError;
use crate::domain::runtime::PODMAN_INHERITED_ENVIRONMENT;
use crate::domain::service_stack::ServiceStackPlan;
use crate::domain::target::TargetId;

mod launchd;
pub use launchd::LaunchdDashboardServiceInstaller;

const MAX_UNIT_BYTES: usize = 64 * 1024;
const SERVICE_NAME: &str = "mm-dashboard.service";

#[derive(Clone)]
pub struct SystemdDashboardServiceInstaller {
    process: Arc<dyn ProcessRunner>,
    home: PathBuf,
}

impl std::fmt::Debug for SystemdDashboardServiceInstaller {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SystemdDashboardServiceInstaller")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl SystemdDashboardServiceInstaller {
    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        let home = directories::BaseDirs::new()
            .map(|directories| directories.home_dir().to_path_buf())
            .ok_or_else(|| MmError::State {
                message: "could not resolve the current user's home directory".to_owned(),
            })?;
        Ok(Self { process, home })
    }

    #[cfg(test)]
    fn with_home(process: Arc<dyn ProcessRunner>, home: PathBuf) -> Self {
        Self { process, home }
    }

    fn paths(&self) -> (PathBuf, PathBuf) {
        (
            self.home.join(".config/systemd/user/mm-dashboard.service"),
            self.home.join(".local/share/mm/dashboard/owner.json"),
        )
    }

    async fn systemctl(&self, arguments: &[&str]) -> Result<CommandOutput, MmError> {
        self.process
            .run(
                CommandSpec::new("/usr/bin/systemctl")
                    .args(arguments.iter().copied())
                    .inherit_environment(PODMAN_INHERITED_ENVIRONMENT)
                    .timeout(Duration::from_secs(60)),
            )
            .await
    }

    async fn require_systemctl(&self, arguments: &[&str]) -> Result<(), MmError> {
        let output = self.systemctl(arguments).await?;
        if output.success() {
            Ok(())
        } else {
            Err(MmError::ProcessFailed {
                program: "/usr/bin/systemctl".to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            })
        }
    }

    async fn restore(
        &self,
        unit_path: &Path,
        unit: Option<&[u8]>,
        owner_path: &Path,
        owner: Option<&[u8]>,
        service_attempted: bool,
    ) -> Result<(), MmError> {
        let mut failure = None;
        if unit.is_none() && service_attempted {
            retain_first(
                &mut failure,
                self.require_systemctl(&["--user", "disable", "--now", SERVICE_NAME])
                    .await,
            );
        }
        retain_first(&mut failure, restore_file(unit_path, unit, 0o644));
        retain_first(&mut failure, restore_file(owner_path, owner, 0o600));
        retain_first(
            &mut failure,
            self.require_systemctl(&["--user", "daemon-reload"]).await,
        );
        if unit.is_some() && owner.is_some() {
            retain_first(
                &mut failure,
                self.require_systemctl(&["--user", "enable", SERVICE_NAME])
                    .await,
            );
            retain_first(
                &mut failure,
                self.require_systemctl(&["--user", "restart", SERVICE_NAME])
                    .await,
            );
        }
        failure.map_or(Ok(()), Err)
    }
}

#[async_trait]
impl DashboardServiceInstaller for SystemdDashboardServiceInstaller {
    async fn preflight(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        verified_unit(plan)?;
        ensure_user_controlled_directory(&self.home)?;
        let (unit_path, owner_path) = self.paths();
        validate_owner(&owner_path, plan.target_id, &unit_path)
    }

    async fn install(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        let Some(unit) = verified_unit(plan)? else {
            return self.remove_managed(plan).await;
        };
        ensure_user_controlled_directory(&self.home)?;
        let (unit_path, owner_path) = self.paths();
        prepare_parent(&unit_path)?;
        prepare_parent(&owner_path)?;
        validate_owner(&owner_path, plan.target_id, &unit_path)?;
        let previous_unit = read_existing(&unit_path)?;
        let previous_owner = read_existing(&owner_path)?;
        let owner = serde_json::to_vec_pretty(&DashboardServiceOwner {
            target_id: plan.target_id,
            plan_hash: plan.plan_hash.clone(),
        })
        .map_err(|error| MmError::State {
            message: format!("could not serialize dashboard service ownership: {error}"),
        })?;
        if let Err(error) = atomic_write(&owner_path, &owner, 0o600)
            .and_then(|()| atomic_write(&unit_path, &unit, 0o644))
        {
            restore_file(&unit_path, previous_unit.as_deref(), 0o644)?;
            restore_file(&owner_path, previous_owner.as_deref(), 0o600)?;
            return Err(error);
        }

        let mut service_attempted = false;
        let result = async {
            self.require_systemctl(&["--user", "daemon-reload"]).await?;
            service_attempted = true;
            self.require_systemctl(&["--user", "enable", SERVICE_NAME])
                .await?;
            self.require_systemctl(&["--user", "restart", SERVICE_NAME])
                .await
        }
        .await;
        if let Err(error) = result {
            if let Err(rollback) = self
                .restore(
                    &unit_path,
                    previous_unit.as_deref(),
                    &owner_path,
                    previous_owner.as_deref(),
                    service_attempted,
                )
                .await
            {
                return Err(rollback_failure("installation", &error, &rollback));
            }
            return Err(error);
        }
        Ok(())
    }

    async fn status(&self, plan: &ServiceStackPlan) -> Result<DashboardServiceStatus, MmError> {
        let expected_unit = verified_unit(plan)?;
        let (unit_path, owner_path) = self.paths();
        let unit = read_existing(&unit_path)?;
        let owner = read_existing(&owner_path)?;
        if unit.is_none() && owner.is_none() {
            return Ok(DashboardServiceStatus {
                installed: false,
                running: false,
            });
        }
        let Some(owner) = owner else {
            return Err(MmError::Security {
                message: "dashboard service unit has no ownership record".to_owned(),
            });
        };
        let owner: DashboardServiceOwner =
            serde_json::from_slice(&owner).map_err(|error| MmError::Security {
                message: format!("dashboard service ownership is invalid: {error}"),
            })?;
        if owner.target_id != plan.target_id {
            return Err(MmError::Security {
                message: "dashboard service is owned by another target".to_owned(),
            });
        }
        if let Some(expected_unit) = expected_unit
            && (owner.plan_hash != plan.plan_hash
                || unit.as_deref() != Some(expected_unit.as_slice()))
        {
            return Err(MmError::Security {
                message: "dashboard service installation differs from its reviewed plan".to_owned(),
            });
        }
        let running = self
            .systemctl(&["--user", "is-active", "--quiet", SERVICE_NAME])
            .await?
            .success();
        Ok(DashboardServiceStatus {
            installed: true,
            running,
        })
    }

    async fn uninstall(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        self.remove_managed(plan).await
    }
}

impl SystemdDashboardServiceInstaller {
    async fn remove_managed(&self, plan: &ServiceStackPlan) -> Result<(), MmError> {
        ensure_user_controlled_directory(&self.home)?;
        let (unit_path, owner_path) = self.paths();
        validate_owner(&owner_path, plan.target_id, &unit_path)?;
        let previous_unit = read_existing(&unit_path)?;
        let previous_owner = read_existing(&owner_path)?;
        if previous_unit.is_none() && previous_owner.is_none() {
            return Ok(());
        }
        let result = async {
            self.require_systemctl(&["--user", "disable", "--now", SERVICE_NAME])
                .await?;
            restore_file(&unit_path, None, 0o644)?;
            restore_file(&owner_path, None, 0o600)?;
            self.require_systemctl(&["--user", "daemon-reload"]).await
        }
        .await;
        if let Err(error) = result {
            if let Err(rollback) = self
                .restore(
                    &unit_path,
                    previous_unit.as_deref(),
                    &owner_path,
                    previous_owner.as_deref(),
                    true,
                )
                .await
            {
                return Err(rollback_failure("removal", &error, &rollback));
            }
            return Err(error);
        }
        Ok(())
    }
}

fn verified_unit(plan: &ServiceStackPlan) -> Result<Option<Vec<u8>>, MmError> {
    plan.validate()
        .map_err(|message| MmError::Security { message })?;
    let Some(service) = plan.dashboard_service.as_ref() else {
        return Ok(None);
    };
    verify_executable(&service.executable)?;
    ensure_user_controlled_directory(&service.mm_home)?;
    let path = plan
        .artifact
        .dashboard_unit_path
        .as_ref()
        .ok_or_else(|| MmError::Security {
            message: "dashboard service unit artifact is missing".to_owned(),
        })?;
    ensure_user_controlled_file(path)?;
    let bytes = read_regular_file(path, MAX_UNIT_BYTES)?.ok_or_else(|| MmError::Security {
        message: "dashboard service unit artifact disappeared".to_owned(),
    })?;
    let digest = blake3::hash(&bytes).to_hex().to_string();
    if plan.artifact.dashboard_unit_digest.as_deref() != Some(&digest)
        || bytes
            != render_dashboard_service_unit(
                service,
                plan.runtime,
                plan.ingress.manifest.macos_host.as_ref(),
            )?
    {
        return Err(MmError::Security {
            message: "dashboard service unit failed canonical verification".to_owned(),
        });
    }
    Ok(Some(bytes))
}

fn verify_executable(path: &Path) -> Result<(), MmError> {
    if !path.is_absolute() {
        return Err(MmError::Security {
            message: "dashboard service executable must be absolute".to_owned(),
        });
    }
    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting dashboard executable {}", path.display()),
        source,
    })?;
    if !metadata.is_file() {
        return Err(MmError::Security {
            message: "dashboard service executable is not a regular file".to_owned(),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mode = metadata.permissions().mode();
        let owner = metadata.uid();
        if mode & 0o111 == 0
            || mode & 0o022 != 0
            || (owner != 0 && owner != rustix::process::geteuid().as_raw())
        {
            return Err(MmError::Security {
                message: "dashboard executable must be executable, owned by root or the current user, and not group/other writable".to_owned(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DashboardServiceOwner {
    target_id: TargetId,
    plan_hash: String,
}

fn validate_owner(owner_path: &Path, target_id: TargetId, unit_path: &Path) -> Result<(), MmError> {
    match read_existing(owner_path)? {
        Some(bytes) => {
            let owner: DashboardServiceOwner =
                serde_json::from_slice(&bytes).map_err(|error| MmError::Security {
                    message: format!("dashboard service ownership is invalid: {error}"),
                })?;
            if owner.target_id != target_id {
                return Err(MmError::Security {
                    message: "dashboard service is owned by another target".to_owned(),
                });
            }
        }
        None if unit_path.exists() => {
            return Err(MmError::Security {
                message: "refusing to adopt an unmanaged dashboard service unit".to_owned(),
            });
        }
        None => {}
    }
    Ok(())
}

fn prepare_parent(path: &Path) -> Result<(), MmError> {
    let parent = path.parent().ok_or_else(|| MmError::Security {
        message: "dashboard service destination has no parent".to_owned(),
    })?;
    ensure_no_symlink_components(parent)?;
    fs::create_dir_all(parent).map_err(|source| MmError::Io {
        context: format!("creating dashboard service directory {}", parent.display()),
        source,
    })?;
    ensure_user_controlled_directory(parent)
}

fn read_existing(path: &Path) -> Result<Option<Vec<u8>>, MmError> {
    ensure_no_symlink_components(path)?;
    let bytes = read_regular_file(path, MAX_UNIT_BYTES)?;
    if bytes.is_some() {
        ensure_user_controlled_file(path)?;
    }
    Ok(bytes)
}

fn restore_file(path: &Path, previous: Option<&[u8]>, mode: u32) -> Result<(), MmError> {
    if let Some(previous) = previous {
        atomic_write(path, previous, mode)
    } else if path.exists() {
        ensure_user_controlled_file(path)?;
        fs::remove_file(path).map_err(|source| MmError::Io {
            context: format!("removing dashboard service file {}", path.display()),
            source,
        })
    } else {
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut options = AtomicWriteFile::options();
    configure_options(&mut options, mode);
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening dashboard service file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing dashboard service file {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing dashboard service file {}", path.display()),
        source,
    })
}

#[cfg(unix)]
fn configure_options(options: &mut atomic_write_file::OpenOptions, mode: u32) {
    use atomic_write_file::unix::OpenOptionsExt;
    use std::os::unix::fs::OpenOptionsExt as _;
    options
        .mode(mode)
        .preserve_mode(false)
        .preserve_owner(false);
}

#[cfg(not(unix))]
fn configure_options(_options: &mut atomic_write_file::OpenOptions, _mode: u32) {}

fn retain_first(failure: &mut Option<MmError>, result: Result<(), MmError>) {
    if let Err(error) = result
        && failure.is_none()
    {
        *failure = Some(error);
    }
}

fn rollback_failure(operation: &str, original: &MmError, rollback: &MmError) -> MmError {
    MmError::Security {
        message: format!(
            "dashboard service {operation} failed with {} and rollback failed with {}",
            original.code(),
            rollback.code()
        ),
    }
}

#[cfg(test)]
#[path = "dashboard_service/tests.rs"]
mod tests;
