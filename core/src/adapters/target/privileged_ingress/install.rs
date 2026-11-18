use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use fs4::{FileExt, TryLockError};
use serde::{Deserialize, Serialize};

use crate::adapters::private_fs::{ensure_no_symlink_components, read_regular_file};
use crate::app::ports::{CommandSpec, PrivilegedIngressInstaller, ProcessRunner};
use crate::app::{haproxy_config, haproxy_service_unit};
use crate::domain::error::MmError;
use crate::domain::target::TargetId;
use crate::domain::target_ingress::{
    INGRESS_PUBLIC_HTTP_PORT, TargetIngressArtifactManifest, TargetIngressPlan,
};

const MAX_ARTIFACT_BYTES: usize = 1024 * 1024;
const INGRESS_SUPPORT_FILES: [&str; 4] = [
    "dynamic.yaml",
    "haproxy.cfg",
    "mm-ingress-forwarder.service",
    "mm-ingress.service",
];
const HAPROXY_CONFIG: &str = "/etc/mm/ingress/haproxy.cfg";
const SYSTEM_UNIT: &str = "/etc/systemd/system/mm-ingress-forwarder.service";
const OWNER_FILE: &str = "/etc/mm/ingress/owner.json";
const HOST_LOCK: &str = "/run/lock/mm-ingress.lock";
const PRIVILEGED_HELPER: &str = "/usr/local/libexec/mm/mm-ingress-helper";

#[derive(Clone)]
pub struct SudoPrivilegedIngressInstaller {
    process: Arc<dyn ProcessRunner>,
    executable: PathBuf,
}

impl std::fmt::Debug for SudoPrivilegedIngressInstaller {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SudoPrivilegedIngressInstaller")
            .field("executable", &self.executable)
            .finish_non_exhaustive()
    }
}

impl SudoPrivilegedIngressInstaller {
    fn privileged_executable(&self) -> Result<PathBuf, MmError> {
        if verify_privileged_executable(&self.executable).is_ok() {
            return Ok(self.executable.clone());
        }
        let helper = PathBuf::from(PRIVILEGED_HELPER);
        verify_privileged_executable(&helper).map_err(|_| MmError::Security {
            message: format!("public ingress needs the protected root-owned helper at {PRIVILEGED_HELPER}; run `mm ingress start`, which installs it from beside mm with sudo (or `./install.sh --setup-only` in a checkout). The user-owned CLI can remain in ~/.local/bin"),
        })?;
        Ok(helper)
    }

    pub fn new(process: Arc<dyn ProcessRunner>) -> Result<Self, MmError> {
        let executable = std::env::current_exe().map_err(|source| MmError::Io {
            context: "resolving the mm executable for privileged ingress installation".to_owned(),
            source,
        })?;
        ensure_no_symlink_components(&executable)?;
        Ok(Self {
            process,
            executable,
        })
    }
}

#[async_trait]
impl PrivilegedIngressInstaller for SudoPrivilegedIngressInstaller {
    async fn install(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        if plan.plan_hash != plan.artifact.manifest.artifact_hash
            || plan.artifact.manifest.canonical_hash() != plan.plan_hash
            || plan.target_id != plan.artifact.manifest.target_id
        {
            return Err(MmError::Security {
                message: "refusing a non-canonical privileged ingress plan".to_owned(),
            });
        }
        if installed_forwarder_matches(plan)? { return Ok(()); }
        let executable = self.privileged_executable()?;
        let output = self
            .process
            .run(
                CommandSpec::new("/usr/bin/sudo")
                    .args([
                        "--".to_owned(),
                        path_text(&executable, "mm executable")?,
                        "__install-ingress".to_owned(),
                        "--artifact-root".to_owned(),
                        path_text(&plan.artifact.root, "ingress artifact root")?,
                        "--target-id".to_owned(),
                        plan.target_id.0.to_string(),
                        "--artifact-hash".to_owned(),
                        plan.plan_hash.clone(),
                    ])
                    .inherit_environment(&["PATH", "HOME", "USER", "LOGNAME", "TERM"])
                    .inherit_stdin()
                    .timeout(Duration::from_secs(120)),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(MmError::ProcessFailed {
                program: "sudo".to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            })
        }
    }

    async fn uninstall(&self, plan: &TargetIngressPlan) -> Result<(), MmError> {
        let executable = self.privileged_executable()?;
        if plan.plan_hash != plan.artifact.manifest.artifact_hash
            || plan.artifact.manifest.canonical_hash() != plan.plan_hash
            || plan.target_id != plan.artifact.manifest.target_id
        {
            return Err(MmError::Security {
                message: "refusing a non-canonical privileged ingress removal plan".to_owned(),
            });
        }
        let output = self
            .process
            .run(
                CommandSpec::new("/usr/bin/sudo")
                    .args([
                        "--".to_owned(),
                        path_text(&executable, "mm executable")?,
                        "__uninstall-ingress".to_owned(),
                        "--target-id".to_owned(),
                        plan.target_id.0.to_string(),
                        "--artifact-hash".to_owned(),
                        plan.plan_hash.clone(),
                    ])
                    .inherit_environment(&["PATH", "HOME", "USER", "LOGNAME", "TERM"])
                    .inherit_stdin()
                    .timeout(Duration::from_secs(120)),
            )
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(MmError::ProcessFailed {
                program: "sudo".to_owned(),
                code: output.code,
                stderr: output.stderr.trim().to_owned(),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct HostIngressOwner {
    target_id: TargetId,
    source_uid: u32,
    artifact_hash: String,
}

pub fn install_privileged_ingress_artifact(
    _artifact_root: &Path,
    _target_id: TargetId,
    _artifact_hash: &str,
) -> Result<(), MmError> {
    #[cfg(not(unix))]
    {
        Err(MmError::Security {
            message: "privileged ingress installation requires Unix permission semantics"
                .to_owned(),
        })
    }
    #[cfg(unix)]
    {
        let (artifact_root, target_id, artifact_hash) =
            (_artifact_root, _target_id, _artifact_hash);
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(MmError::Security {
                message: "privileged ingress installation must run through sudo".to_owned(),
            });
        }
        let source_uid = std::env::var("SUDO_UID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|uid| *uid != 0)
            .ok_or_else(|| MmError::Security {
                message: "privileged ingress installation requires a non-root SUDO_UID".to_owned(),
            })?;
        validate_hash(artifact_hash)?;
        let artifact = verify_source_artifact(artifact_root, target_id, artifact_hash, source_uid)?;
        verify_fixed_templates(target_id, &artifact)?;
        let _lock = host_lock()?;
        ensure_fixed_directories()?;
        validate_owner(target_id, source_uid)?;

        let validation = ValidationFiles::new(&artifact.haproxy, &artifact.unit)?;
        let haproxy_source = path_text(&validation.haproxy, "staged HAProxy config")?;
        run_checked("/usr/sbin/haproxy", &["-c", "-f", &haproxy_source])?;
        let unit_source = path_text(&validation.unit, "staged systemd unit")?;
        run_checked("/usr/bin/systemd-analyze", &["verify", &unit_source])?;

        let previous_owner = read_existing_fixed(Path::new(OWNER_FILE))?;
        let previous_config = read_existing_fixed(Path::new(HAPROXY_CONFIG))?;
        let previous_unit = read_existing_fixed(Path::new(SYSTEM_UNIT))?;
        let owner = serde_json::to_vec_pretty(&HostIngressOwner {
            target_id,
            source_uid,
            artifact_hash: artifact_hash.to_owned(),
        })
        .map_err(|error| MmError::State {
            message: format!("could not serialize ingress host ownership: {error}"),
        })?;
        let mut service_attempted = false;
        let installation = (|| {
            // Public, non-secret attribution lets the user CLI recognize an
            // unchanged forwarder without elevating on every site update.
            atomic_replace(Path::new(OWNER_FILE), &owner, 0o644)?;
            atomic_replace(Path::new(HAPROXY_CONFIG), &artifact.haproxy, 0o644)?;
            atomic_replace(Path::new(SYSTEM_UNIT), &artifact.unit, 0o644)?;
            configure_selinux(target_id)?;
            run_checked("/usr/bin/systemctl", &["daemon-reload"])?;
            service_attempted = true;
            run_checked(
                "/usr/bin/systemctl",
                &["enable", "mm-ingress-forwarder.service"],
            )?;
            run_checked(
                "/usr/bin/systemctl",
                &["restart", "mm-ingress-forwarder.service"],
            )?;
            verify_public_forwarder()
        })();
        if let Err(error) = installation {
            let cleanup = rollback_host_install(
                previous_owner.as_deref(),
                previous_config.as_deref(),
                previous_unit.as_deref(),
                service_attempted,
            );
            if !cleanup.is_empty() {
                return Err(MmError::operation_failure(
                    artifact_hash,
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    cleanup,
                    "restore the owned HAProxy and systemd files before retrying installation",
                ));
            }
            return Err(error);
        }
        Ok(())
    }
}

pub fn uninstall_privileged_ingress(
    _target_id: TargetId,
    _artifact_hash: &str,
) -> Result<(), MmError> {
    #[cfg(not(unix))]
    {
        Err(MmError::Security {
            message: "privileged ingress removal requires Unix permission semantics".to_owned(),
        })
    }
    #[cfg(unix)]
    {
        let (target_id, artifact_hash) = (_target_id, _artifact_hash);
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(MmError::Security {
                message: "privileged ingress removal must run through sudo".to_owned(),
            });
        }
        let source_uid = std::env::var("SUDO_UID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|uid| *uid != 0)
            .ok_or_else(|| MmError::Security {
                message: "privileged ingress removal requires a non-root SUDO_UID".to_owned(),
            })?;
        validate_hash(artifact_hash)?;
        let _lock = host_lock()?;
        let previous_owner = read_existing_fixed(Path::new(OWNER_FILE))?;
        let previous_config = read_existing_fixed(Path::new(HAPROXY_CONFIG))?;
        let previous_unit = read_existing_fixed(Path::new(SYSTEM_UNIT))?;
        let Some(owner_bytes) = previous_owner.as_deref() else {
            if previous_config.is_some() || previous_unit.is_some() {
                return Err(MmError::Security {
                    message: "refusing to remove unmanaged privileged ingress files".to_owned(),
                });
            }
            return Ok(());
        };
        let owner: HostIngressOwner =
            serde_json::from_slice(owner_bytes).map_err(|error| MmError::Security {
                message: format!("existing ingress host ownership is invalid: {error}"),
            })?;
        if owner.target_id != target_id || owner.source_uid != source_uid {
            return Err(MmError::Security {
                message: "this host's public ingress ports are owned by another target or user"
                    .to_owned(),
            });
        }

        let removal = (|| {
            if previous_unit.is_some() {
                run_checked(
                    "/usr/bin/systemctl",
                    &["disable", "--now", "mm-ingress-forwarder.service"],
                )?;
            }
            restore_fixed(Path::new(HAPROXY_CONFIG), None, 0o644)?;
            restore_fixed(Path::new(SYSTEM_UNIT), None, 0o644)?;
            restore_fixed(Path::new(OWNER_FILE), None, 0o600)?;
            run_checked("/usr/bin/systemctl", &["daemon-reload"])
        })();
        if let Err(error) = removal {
            let cleanup = rollback_host_install(
                previous_owner.as_deref(),
                previous_config.as_deref(),
                previous_unit.as_deref(),
                previous_unit.is_some(),
            );
            if !cleanup.is_empty() {
                return Err(MmError::operation_failure(
                    artifact_hash,
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    cleanup,
                    "restore the owned HAProxy and systemd files before retrying removal",
                ));
            }
            return Err(error);
        }
        Ok(())
    }
}
