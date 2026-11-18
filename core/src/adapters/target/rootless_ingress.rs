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
use crate::app::ports::{CommandSpec, ProcessRunner, RootlessIngressInstaller};
use crate::domain::error::MmError;
use crate::domain::runtime::PODMAN_INHERITED_ENVIRONMENT;
use crate::domain::target::TargetId;
use crate::domain::target_ingress::{TARGET_INGRESS_COMPOSE_PATH, TargetIngressPlan};

const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
const USER_UNIT_DESTINATION: &str = "%h/.config/systemd/user/mm-ingress.service";

#[derive(Clone)]
pub struct SystemdRootlessIngressInstaller {
    process: Arc<dyn ProcessRunner>,
    home: PathBuf,
}

impl std::fmt::Debug for SystemdRootlessIngressInstaller {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SystemdRootlessIngressInstaller")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl SystemdRootlessIngressInstaller {
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

    async fn run_systemctl(&self, arguments: &[&str]) -> Result<(), MmError> {
        let output = self
            .process
            .run(
                CommandSpec::new("/usr/bin/systemctl")
                    .args(arguments.iter().copied())
                    .inherit_environment(PODMAN_INHERITED_ENVIRONMENT)
                    .timeout(Duration::from_secs(60)),
            )
            .await?;
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
}

#[async_trait]
impl RootlessIngressInstaller for SystemdRootlessIngressInstaller {
    async fn install(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        let prepared = verify_plan(plan)?;
        ensure_user_controlled_directory(&self.home)?;

        let compose_destination = self
            .home
            .join(".local/share/mm/ingress/current/compose.yaml");
        let unit_destination = self.home.join(".config/systemd/user/mm-ingress.service");
        let owner_destination = self.home.join(".local/share/mm/ingress/owner.json");
        prepare_parent(&compose_destination)?;
        prepare_parent(&unit_destination)?;
        validate_owner(
            &owner_destination,
            plan.target_id,
            &compose_destination,
            &unit_destination,
        )?;

        let previous_owner = read_existing(&owner_destination)?;
        let previous_compose = read_existing(&compose_destination)?;
        let previous_unit = read_existing(&unit_destination)?;
        let owner = serde_json::to_vec_pretty(&RootlessIngressOwner {
            target_id: plan.target_id,
            artifact_hash: plan.plan_hash.clone(),
        })
        .map_err(|error| MmError::State {
            message: format!("could not serialize rootless ingress ownership: {error}"),
        })?;
        if let Err(error) = atomic_write(&owner_destination, &owner, 0o600)
            .and_then(|()| atomic_write(&compose_destination, &prepared.compose, 0o600))
            .and_then(|()| atomic_write(&unit_destination, &prepared.unit, 0o644))
        {
            let cleanup = restore_installation(
                &owner_destination,
                previous_owner.as_deref(),
                &compose_destination,
                previous_compose.as_deref(),
                &unit_destination,
                previous_unit.as_deref(),
            )
            .err()
            .map(|failure| vec![failure.to_string()])
            .unwrap_or_default();
            return Err(if cleanup.is_empty() {
                error
            } else {
                MmError::operation_failure(
                    &plan.plan_hash,
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    cleanup,
                    "restore or remove the owned rootless ingress files, then retry",
                )
            });
        }

        let mut service_attempted = false;
        let installation = async {
            self.run_systemctl(&["--user", "daemon-reload"]).await?;
            service_attempted = true;
            self.run_systemctl(&["--user", "enable", "--now", "mm-ingress.service"])
                .await
        }
        .await;
        if let Err(error) = installation {
            let cleanup = self
                .rollback_installation(InstallationSnapshot {
                    owner_path: &owner_destination,
                    owner: previous_owner.as_deref(),
                    compose_path: &compose_destination,
                    compose: previous_compose.as_deref(),
                    unit_path: &unit_destination,
                    unit: previous_unit.as_deref(),
                    service_attempted,
                })
                .await;
            if !cleanup.is_empty() {
                return Err(MmError::operation_failure(
                    &plan.plan_hash,
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    cleanup,
                    "restore the owned ingress files and systemd user service before retrying",
                ));
            }
            return Err(error);
        }
        Ok(())
    }

    async fn uninstall(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        verify_plan(plan)?;
        ensure_user_controlled_directory(&self.home)?;

        let compose_destination = self
            .home
            .join(".local/share/mm/ingress/current/compose.yaml");
        let unit_destination = self.home.join(".config/systemd/user/mm-ingress.service");
        let owner_destination = self.home.join(".local/share/mm/ingress/owner.json");
        let previous_owner = read_existing(&owner_destination)?;
        let previous_compose = read_existing(&compose_destination)?;
        let previous_unit = read_existing(&unit_destination)?;
        let Some(owner_bytes) = previous_owner.as_deref() else {
            if previous_compose.is_some() || previous_unit.is_some() {
                return Err(MmError::Security {
                    message: "refusing to remove unmanaged rootless ingress files".to_owned(),
                });
            }
            return Ok(());
        };
        let owner: RootlessIngressOwner =
            serde_json::from_slice(owner_bytes).map_err(|error| MmError::Security {
                message: format!("rootless ingress ownership is invalid: {error}"),
            })?;
        if owner.target_id != plan.target_id {
            return Err(MmError::Security {
                message: "rootless ingress is owned by another target".to_owned(),
            });
        }

        let removal = async {
            if previous_unit.is_some() {
                self.run_systemctl(&["--user", "disable", "--now", "mm-ingress.service"])
                    .await?;
            }
            restore_installation(
                &owner_destination,
                None,
                &compose_destination,
                None,
                &unit_destination,
                None,
            )?;
            self.run_systemctl(&["--user", "daemon-reload"]).await
        }
        .await;
        if let Err(error) = removal {
            let cleanup = self
                .rollback_installation(InstallationSnapshot {
                    owner_path: &owner_destination,
                    owner: previous_owner.as_deref(),
                    compose_path: &compose_destination,
                    compose: previous_compose.as_deref(),
                    unit_path: &unit_destination,
                    unit: previous_unit.as_deref(),
                    service_attempted: previous_unit.is_some(),
                })
                .await;
            if !cleanup.is_empty() {
                return Err(MmError::operation_failure(
                    &plan.plan_hash,
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    cleanup,
                    "restore the owned ingress files and systemd user service before retrying removal",
                ));
            }
            return Err(error);
        }
        Ok(())
    }
}

impl SystemdRootlessIngressInstaller {
    async fn rollback_installation(&self, snapshot: InstallationSnapshot<'_>) -> Vec<String> {
        let InstallationSnapshot {
            owner_path,
            owner,
            compose_path,
            compose,
            unit_path,
            unit,
            service_attempted,
        } = snapshot;
        let mut failures = Vec::new();
        let previous_complete = owner.is_some() && compose.is_some() && unit.is_some();
        if !previous_complete && service_attempted {
            retain_cleanup_error(
                &mut failures,
                self.run_systemctl(&["--user", "disable", "--now", "mm-ingress.service"])
                    .await,
            );
        }
        retain_cleanup_error(
            &mut failures,
            restore_installation(owner_path, owner, compose_path, compose, unit_path, unit),
        );
        retain_cleanup_error(
            &mut failures,
            self.run_systemctl(&["--user", "daemon-reload"]).await,
        );
        if previous_complete {
            retain_cleanup_error(
                &mut failures,
                self.run_systemctl(&["--user", "enable", "mm-ingress.service"])
                    .await,
            );
            retain_cleanup_error(
                &mut failures,
                self.run_systemctl(&["--user", "restart", "mm-ingress.service"])
                    .await,
            );
        }
        failures
    }
}

struct InstallationSnapshot<'a> {
    owner_path: &'a Path,
    owner: Option<&'a [u8]>,
    compose_path: &'a Path,
    compose: Option<&'a [u8]>,
    unit_path: &'a Path,
    unit: Option<&'a [u8]>,
    service_attempted: bool,
}

fn retain_cleanup_error(failures: &mut Vec<String>, result: Result<(), MmError>) {
    if let Err(error) = result {
        failures.push(error.to_string());
    }
}

struct VerifiedRootlessArtifact {
    compose: Vec<u8>,
    unit: Vec<u8>,
}

fn verify_plan(plan: &TargetIngressPlan) -> Result<VerifiedRootlessArtifact, MmError> {
    let manifest = &plan.artifact.manifest;
    if plan.target_id != manifest.target_id
        || plan.plan_hash != manifest.artifact_hash
        || manifest.canonical_hash() != plan.plan_hash
        || manifest.target_compose_path != TARGET_INGRESS_COMPOSE_PATH
    {
        return Err(MmError::Security {
            message: "refusing a non-canonical rootless ingress plan".to_owned(),
        });
    }
    let compose = verified_source(&plan.artifact.compose_path, &manifest.compose_digest)?;
    let unit_manifest = manifest
        .support_files
        .iter()
        .find(|file| file.name == "mm-ingress.service")
        .filter(|file| file.destination == USER_UNIT_DESTINATION && file.mode == "0644")
        .ok_or_else(|| MmError::Security {
            message: "ingress plan has no canonical rootless systemd unit".to_owned(),
        })?;
    let unit_path = plan
        .artifact
        .support_paths
        .iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("mm-ingress.service"))
        .ok_or_else(|| MmError::Security {
            message: "rootless ingress systemd unit is missing from its artifact".to_owned(),
        })?;
    let unit = verified_source(unit_path, &unit_manifest.digest)?;
    Ok(VerifiedRootlessArtifact { compose, unit })
}

fn verified_source(path: &Path, expected_digest: &str) -> Result<Vec<u8>, MmError> {
    ensure_user_controlled_file(path)?;
    let bytes = read_regular_file(path, MAX_ARTIFACT_BYTES)?.ok_or_else(|| MmError::Security {
        message: format!("ingress artifact '{}' disappeared", path.display()),
    })?;
    if blake3::hash(&bytes).to_hex().as_str() != expected_digest {
        return Err(MmError::Security {
            message: format!(
                "ingress artifact '{}' failed digest verification",
                path.display()
            ),
        });
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RootlessIngressOwner {
    target_id: TargetId,
    artifact_hash: String,
}

fn validate_owner(
    owner_path: &Path,
    target_id: TargetId,
    compose_path: &Path,
    unit_path: &Path,
) -> Result<(), MmError> {
    match read_existing(owner_path)? {
        Some(bytes) => {
            let owner: RootlessIngressOwner =
                serde_json::from_slice(&bytes).map_err(|error| MmError::Security {
                    message: format!("rootless ingress ownership is invalid: {error}"),
                })?;
            if owner.target_id != target_id {
                return Err(MmError::Security {
                    message: "rootless ingress is owned by another target".to_owned(),
                });
            }
        }
        None if compose_path.exists() || unit_path.exists() => {
            return Err(MmError::Security {
                message: "refusing to adopt existing unmanaged rootless ingress files".to_owned(),
            });
        }
        None => {}
    }
    Ok(())
}

fn prepare_parent(path: &Path) -> Result<(), MmError> {
    let parent = path.parent().ok_or_else(|| MmError::Security {
        message: "rootless ingress destination has no parent directory".to_owned(),
    })?;
    ensure_no_symlink_components(parent)?;
    fs::create_dir_all(parent).map_err(|source| MmError::Io {
        context: format!("creating rootless ingress directory {}", parent.display()),
        source,
    })?;
    ensure_user_controlled_directory(parent)
}

fn read_existing(path: &Path) -> Result<Option<Vec<u8>>, MmError> {
    ensure_no_symlink_components(path)?;
    let existing = read_regular_file(path, MAX_ARTIFACT_BYTES)?;
    if existing.is_some() {
        ensure_user_controlled_file(path)?;
    }
    Ok(existing)
}

fn restore_installation(
    owner_path: &Path,
    owner: Option<&[u8]>,
    compose_path: &Path,
    compose: Option<&[u8]>,
    unit_path: &Path,
    unit: Option<&[u8]>,
) -> Result<(), MmError> {
    restore_file(compose_path, compose, 0o600)?;
    restore_file(unit_path, unit, 0o644)?;
    restore_file(owner_path, owner, 0o600)
}

fn restore_file(path: &Path, previous: Option<&[u8]>, mode: u32) -> Result<(), MmError> {
    if let Some(previous) = previous {
        atomic_write(path, previous, mode)
    } else if path.exists() {
        ensure_user_controlled_file(path)?;
        fs::remove_file(path).map_err(|source| MmError::Io {
            context: format!("removing rootless ingress file {}", path.display()),
            source,
        })
    } else {
        Ok(())
    }
}

fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let mut options = AtomicWriteFile::options();
    configure_atomic_options(&mut options, mode);
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening rootless ingress file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing rootless ingress file {}", path.display()),
            source,
        })?;
    ensure_no_symlink_components(path)?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing rootless ingress file {}", path.display()),
        source,
    })?;
    set_mode(path, mode)
}

#[cfg(unix)]
fn configure_atomic_options(options: &mut atomic_write_file::OpenOptions, mode: u32) {
    use atomic_write_file::unix::OpenOptionsExt;
    use std::os::unix::fs::OpenOptionsExt as _;

    options
        .mode(mode)
        .preserve_mode(false)
        .preserve_owner(false);
}

#[cfg(not(unix))]
fn configure_atomic_options(_options: &mut atomic_write_file::OpenOptions, _mode: u32) {}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| MmError::Io {
        context: format!(
            "setting rootless ingress file permissions on {}",
            path.display()
        ),
        source,
    })?;
    ensure_user_controlled_file(path)
}

#[cfg(not(unix))]
fn set_mode(path: &Path, _mode: u32) -> Result<(), MmError> {
    ensure_user_controlled_file(path)
}

#[cfg(test)]
#[path = "rootless_ingress/tests.rs"]
mod tests;
