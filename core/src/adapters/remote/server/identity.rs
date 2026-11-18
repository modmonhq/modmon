use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{UnixListener, UnixStream};

use crate::adapters::remote::access::{AccessGuard, DeployedState, HostAccessStore};
use crate::adapters::remote::git::ProjectRepositories;
use crate::adapters::remote::codec::{FrameReader, encode_frame, write_frame, write_raw_frame};
use crate::adapters::remote::config::DaemonConfig;
use crate::adapters::remote::journal::RemoteJournal;
use crate::app::ports::{ExecStream, LogStream, RemoteRequestContext, RemoteUseCases};
use crate::domain::access::{
    AccessAction, AccessRequest, AccessResourceKind, Authorization, Capability, PrincipalId,
};
use crate::domain::build::BuildReceipt;
use crate::domain::error::MmError;
use crate::domain::observability::OperationCancellation;
use crate::domain::remote::{
    check_receipt_scope, ClientFrame, MAX_REQUEST_TIMEOUT_MS, MAX_STREAM_BYTES, MAX_STREAM_FRAMES, MutationState,
    PLAN_TTL_MS, PROTOCOL_VERSION, RemoteAccessPlan, RemoteDeploymentPlan, RemoteError,
    RemoteMethod, RemotePlanBinding, RemotePlanKind, RemoteResult, RemoteStreamItem,
    ServerFrame, ServerHello, StoredRemotePlan,
};
use crate::domain::users::AccessChange;

#[derive(Debug, Clone)]
pub struct DaemonIdentity {
    pub fingerprint: String,
}

impl DaemonIdentity {
    pub fn load_or_create(state_root: &Path) -> Result<Self, MmError> {
        let path = state_root.join("daemon.identity");
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut bytes = [0_u8; 32];
                getrandom::fill(&mut bytes).map_err(|error| MmError::State {
                    message: format!("could not generate daemon identity: {error}"),
                })?;
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                match options.open(&path) {
                    Ok(mut file) => {
                        file.write_all(&bytes)
                            .and_then(|()| file.sync_all())
                            .map_err(|source| MmError::Io {
                                context: format!("writing daemon identity {}", path.display()),
                                source,
                            })?;
                        bytes.to_vec()
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        fs::read(&path).map_err(|source| MmError::Io {
                            context: format!("reading daemon identity {}", path.display()),
                            source,
                        })?
                    }
                    Err(source) => {
                        return Err(MmError::Io {
                            context: format!("creating daemon identity {}", path.display()),
                            source,
                        });
                    }
                }
            }
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading daemon identity {}", path.display()),
                    source,
                });
            }
        };
        let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
            context: format!("inspecting daemon identity {}", path.display()),
            source,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.uid() != rustix::process::getuid().as_raw()
                || metadata.permissions().mode() & 0o022 != 0
            {
                return Err(MmError::Security {
                    message: "daemon identity file has an unsafe type, owner, or mode".to_owned(),
                });
            }
        }
        if !metadata.is_file() || metadata.file_type().is_symlink() || bytes.len() != 32 {
            return Err(MmError::Security {
                message:
                    "daemon identity must be a regular file containing exactly 32 random bytes"
                        .to_owned(),
            });
        }
        Ok(Self {
            fingerprint: blake3::hash(&bytes).to_hex().to_string(),
        })
    }
}

#[cfg(test)]
#[path = "identity/tests.rs"]
mod identity_tests;
