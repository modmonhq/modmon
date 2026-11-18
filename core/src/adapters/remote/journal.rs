use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::domain::error::MmError;
use crate::domain::remote::{ClusterPlanSeal, RemoteScope, StoredRemotePlan};

/// A request's nonce is kept this long, and then forgotten. A request
/// carries a deadline at most a day ahead and is refused after it, so a
/// copy of one older than this can no longer run anyway.
const REQUEST_RETENTION: Duration = Duration::from_secs(2 * 24 * 60 * 60);
/// How often the journal looks for nonces to forget.
const PRUNE_EVERY: Duration = Duration::from_secs(60 * 60);

pub(super) struct RemoteJournal {
    root: PathBuf,
    audit: Mutex<()>,
    log: super::audit::AuditLog,
    /// When the request nonces were last pruned.
    pruned: Mutex<Option<Instant>>,
}

impl RemoteJournal {
    pub(super) fn open(root: PathBuf) -> Result<Self, MmError> {
        create_private_directory(&root)?;
        for child in ["plans", "claims", "requests", "seals"] {
            create_private_directory(&root.join(child))?;
        }
        Ok(Self {
            log: super::audit::AuditLog::new(&root),
            root,
            audit: Mutex::new(()),
            pruned: Mutex::new(None),
        })
    }

    /// Forgets request nonces older than [`REQUEST_RETENTION`], at most
    /// once every [`PRUNE_EVERY`]. A nonce that cannot be removed stays;
    /// the next look tries again.
    fn prune_requests(&self) {
        {
            let Ok(mut pruned) = self.pruned.lock() else {
                return;
            };
            if pruned.is_some_and(|last| last.elapsed() < PRUNE_EVERY) {
                return;
            }
            *pruned = Some(Instant::now());
        }
        let Ok(entries) = fs::read_dir(self.root.join("requests")) else {
            return;
        };
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > REQUEST_RETENTION));
            if old && let Err(error) = fs::remove_file(entry.path()) {
                tracing::debug!(%error, "forgetting an old request nonce failed");
            }
        }
    }

    pub(super) fn save_plan(&self, plan: &StoredRemotePlan) -> Result<(), MmError> {
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        let path = self.plan_path(&plan.binding().plan_hash)?;
        write_new_private(&path, &serde_json::to_vec(plan).map_err(state_json)?)
    }

    #[cfg(test)]
    pub(super) fn claim_plan(
        &self,
        hash: &str,
        scope: &RemoteScope,
        now_unix_ms: i64,
    ) -> Result<StoredRemotePlan, MmError> {
        let plan = self.read_plan(hash, scope, now_unix_ms)?;
        self.claim_validated_plan(hash, plan)
    }

    pub(super) fn read_plan(
        &self,
        hash: &str,
        scope: &RemoteScope,
        now_unix_ms: i64,
    ) -> Result<StoredRemotePlan, MmError> {
        let plan = self.stored_plan(hash, scope)?;
        if now_unix_ms > plan.binding().expires_at_unix_ms {
            return Err(MmError::Security {
                message: "reviewed remote plan has expired; produce and review a new plan"
                    .to_owned(),
            });
        }
        Ok(plan)
    }

    /// A plan this host already applied, for a step that follows its apply.
    /// Its review window no longer matters: the apply happened inside it.
    pub(super) fn applied_plan(
        &self,
        hash: &str,
        scope: &RemoteScope,
    ) -> Result<Option<StoredRemotePlan>, MmError> {
        if !self.is_claimed(hash)? {
            return Ok(None);
        }
        self.stored_plan(hash, scope).map(Some)
    }

    fn stored_plan(&self, hash: &str, scope: &RemoteScope) -> Result<StoredRemotePlan, MmError> {
        let path = self.plan_path(hash)?;
        let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
            context: format!("inspecting remote plan {}", path.display()),
            source,
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(MmError::Security {
                message: "stored remote plan was replaced by an unsafe object".to_owned(),
            });
        }
        let bytes = fs::read(&path).map_err(|source| MmError::Io {
            context: format!("reading remote plan {}", path.display()),
            source,
        })?;
        if bytes.len() > crate::domain::remote::MAX_FRAME_BYTES {
            return Err(state("stored remote plan exceeds the protocol bound"));
        }
        let plan: StoredRemotePlan = serde_json::from_slice(&bytes).map_err(state_json)?;
        plan.validate()
            .map_err(|message| MmError::Security { message })?;
        let binding = plan.binding();
        if binding.plan_hash != hash
            || binding.project_id != scope.project_id
            || binding.environment != scope.environment
            || binding.target != scope.target
            || binding.target_fingerprint != scope.target_fingerprint
            || binding.runtime != scope.runtime
            || binding.node != scope.node
        {
            return Err(MmError::Security {
                message: "remote apply scope does not exactly match the reviewed plan".to_owned(),
            });
        }
        Ok(plan)
    }

    pub(super) fn claim_validated_plan(
        &self,
        hash: &str,
        plan: StoredRemotePlan,
    ) -> Result<StoredRemotePlan, MmError> {
        let claim = self.root.join("claims").join(hash);
        write_new_private(&claim, plan.binding().plan_nonce.to_string().as_bytes()).map_err(|error| {
            if matches!(&error, MmError::Io { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists)
            {
                MmError::Security {
                    message: "reviewed remote plan was already claimed and cannot be replayed"
                        .to_owned(),
                }
            } else {
                error
            }
        })?;
        Ok(plan)
    }

    pub(super) fn begin_request(
        &self,
        operation_nonce: uuid::Uuid,
        request_id: uuid::Uuid,
    ) -> Result<(), MmError> {
        self.prune_requests();
        let path = self.root.join("requests").join(operation_nonce.to_string());
        let record = serde_json::to_vec(&serde_json::json!({
            "request_id": request_id,
            "phase": "authorized",
        }))
        .map_err(state_json)?;
        write_new_private(&path, &record).map_err(|error| {
            if matches!(&error, MmError::Io { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists)
            {
                MmError::Security {
                    message: "operation nonce was already used; request replay rejected".to_owned(),
                }
            } else {
                error
            }
        })
    }

    pub(super) fn set_request_phase(
        &self,
        operation_nonce: uuid::Uuid,
        phase: &str,
    ) -> Result<(), MmError> {
        if !matches!(
            phase,
            "running" | "succeeded" | "failed" | "unknown" | "interrupted"
        ) {
            return Err(state("remote request phase is invalid"));
        }
        let path = self.root.join("requests").join(operation_nonce.to_string());
        let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
            context: format!("inspecting remote request journal {}", path.display()),
            source,
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(MmError::Security {
                message: "remote request journal entry was replaced".to_owned(),
            });
        }
        let previous: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).map_err(|source| MmError::Io {
                context: format!("reading remote request journal {}", path.display()),
                source,
            })?)
            .map_err(state_json)?;
        let request_id = previous
            .get("request_id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| state("remote request journal has no request ID"))?;
        let record = serde_json::to_vec(&serde_json::json!({
            "request_id": request_id,
            "phase": phase,
        }))
        .map_err(state_json)?;
        let mut options = OpenOptions::new();
        options.write(true).truncate(true);
        let mut file = options.open(&path).map_err(|source| MmError::Io {
            context: format!("updating remote request journal {}", path.display()),
            source,
        })?;
        file.write_all(&record)
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("updating remote request journal {}", path.display()),
                source,
            })
    }

    /// What a daemon that starts does with the requests the last one left
    /// open: a request that was let in or running then ended with that
    /// daemon, so it is marked `interrupted`. Only entries written within
    /// the longest request deadline are looked at; older ones cannot be
    /// running. Returns how many were marked.
    pub(super) fn interrupt_open_requests(&self) -> Result<usize, MmError> {
        let directory = self.root.join("requests");
        let recent =
            std::time::Duration::from_millis(crate::domain::remote::MAX_REQUEST_TIMEOUT_MS);
        let entries = fs::read_dir(&directory).map_err(|source| MmError::Io {
            context: format!("listing remote request journal {}", directory.display()),
            source,
        })?;
        let mut marked = 0;
        for entry in entries.flatten() {
            let fresh = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age <= recent);
            let nonce = entry
                .file_name()
                .to_str()
                .and_then(|name| uuid::Uuid::parse_str(name).ok());
            let (true, Some(nonce)) = (fresh, nonce) else {
                continue;
            };
            let open = fs::read(entry.path())
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|record| {
                    record
                        .get("phase")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .is_some_and(|phase| matches!(phase.as_str(), "authorized" | "running"));
            if open {
                self.set_request_phase(nonce, "interrupted")?;
                marked += 1;
            }
        }
        Ok(marked)
    }

    /// Whether the journal can be written to: nothing holds its lock.
    pub(super) fn is_free(&self) -> Result<(), MmError> {
        self.audit
            .lock()
            .map(drop)
            .map_err(|_| state("audit lock is poisoned"))
    }

    pub(super) fn audit(&self, value: &serde_json::Value) -> Result<(), MmError> {
        let _guard = self
            .audit
            .lock()
            .map_err(|_| state("audit lock is poisoned"))?;
        self.log.append(value)
    }

    /// Keeps the seal of a cluster plan this node is part of. Sealing the
    /// same plan again is harmless.
    pub(super) fn save_seal(&self, seal: &ClusterPlanSeal) -> Result<(), MmError> {
        seal.validate()
            .map_err(|message| MmError::Security { message })?;
        let path = self.hashed_path("seals", &seal.cluster_plan_hash)?;
        let bytes = serde_json::to_vec(seal).map_err(state_json)?;
        match write_new_private(&path, &bytes) {
            Err(MmError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::AlreadyExists =>
            {
                if self.read_seal(&seal.cluster_plan_hash)?.as_ref() == Some(seal) {
                    Ok(())
                } else {
                    Err(MmError::Security {
                        message: "another cluster plan is already sealed under this hash"
                            .to_owned(),
                    })
                }
            }
            result => result,
        }
    }

    /// The seal kept under a cluster plan hash, if this node has one.
    pub(super) fn read_seal(&self, hash: &str) -> Result<Option<ClusterPlanSeal>, MmError> {
        let path = self.hashed_path("seals", hash)?;
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading cluster plan seal {}", path.display()),
                    source,
                });
            }
        };
        if bytes.len() > crate::domain::remote::MAX_FRAME_BYTES {
            return Err(state("stored cluster plan seal exceeds the protocol bound"));
        }
        let seal: ClusterPlanSeal = serde_json::from_slice(&bytes).map_err(state_json)?;
        seal.validate()
            .map_err(|message| MmError::Security { message })?;
        if seal.cluster_plan_hash != hash {
            return Err(MmError::Security {
                message: "stored cluster plan seal does not match its hash".to_owned(),
            });
        }
        Ok(Some(seal))
    }

    /// Whether a plan was already claimed by an apply.
    pub(super) fn is_claimed(&self, hash: &str) -> Result<bool, MmError> {
        Ok(self.hashed_path("claims", hash)?.exists())
    }

    fn plan_path(&self, hash: &str) -> Result<PathBuf, MmError> {
        self.hashed_path("plans", hash)
    }

    fn hashed_path(&self, directory: &str, hash: &str) -> Result<PathBuf, MmError> {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(MmError::Security {
                message: "remote plan hash is invalid".to_owned(),
            });
        }
        Ok(self.root.join(directory).join(hash))
    }
}

fn create_private_directory(path: &Path) -> Result<(), MmError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(path).map_err(|source| MmError::Io {
            context: format!("creating remote daemon state {}", path.display()),
            source,
        })?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating remote daemon state {}", path.display()),
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting remote daemon state {}", path.display()),
        source,
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(MmError::Security {
            message: format!(
                "remote daemon state '{}' is not a safe directory",
                path.display()
            ),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != rustix::process::getuid().as_raw()
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err(MmError::Security {
                message: format!(
                    "remote daemon state '{}' has an unsafe owner or mode",
                    path.display()
                ),
            });
        }
    }
    Ok(())
}

fn write_new_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("creating replay-safe remote state {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing replay-safe remote state {}", path.display()),
            source,
        })
}

fn state(message: &str) -> MmError {
    MmError::State {
        message: message.to_owned(),
    }
}

fn state_json(error: serde_json::Error) -> MmError {
    state(&format!("remote journal JSON is invalid: {error}"))
}

#[cfg(test)]
#[path = "journal/tests.rs"]
mod tests;
