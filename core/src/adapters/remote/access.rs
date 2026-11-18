//! Host access state: the root-owned host policy, the users file each
//! environment last reconciled, and emergency blocks. Together they form the
//! effective policy `mmd` enforces and the `authorized_keys` sshd reads.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::file_lock;
use crate::domain::access::AccessPolicy;
use crate::domain::error::MmError;
use crate::domain::host_policy::{
    EmergencyBlock, EnvironmentAccessRecord, HostPolicy, ProjectEnrollment, effective_policy,
};
use crate::domain::workspace::ProjectId;

const MAX_POLICY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HostAccessStore {
    host_policy_path: PathBuf,
    state_root: PathBuf,
    authorized_keys_path: PathBuf,
    relay_command: String,
}

/// Every input of the effective policy, read together.
#[derive(Debug, Clone)]
pub struct AccessSnapshot {
    pub host: HostPolicy,
    pub records: Vec<EnvironmentAccessRecord>,
    pub blocks: Vec<EmergencyBlock>,
    pub policy: AccessPolicy,
}

/// How long a change to access state waits for another one. Each keeps
/// the lock for one short section, so a longer wait means a process is
/// stuck with it.
const LOCK_PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

/// The lock file of access state, held. Dropping it releases the lock.
struct HeldLock(fs::File);

impl Drop for HeldLock {
    fn drop(&mut self) {
        // The note names the holder; with the lock released there is none.
        if let Err(error) = self.0.set_len(0) {
            tracing::debug!(%error, "could not clear the access state lock's holder note");
        }
        if let Err(error) = self.0.unlock() {
            tracing::warn!(%error, "failed to release the access state lock");
        }
    }
}

/// The snapshot read under the access lock. It exists only inside
/// `HostAccessStore::locked`, whose section cannot await: the lock is never
/// kept while something waits on the engine, the network, or a child
/// process.
pub struct AccessGuard {
    snapshot: AccessSnapshot,
    _lock: HeldLock,
}

impl std::ops::Deref for AccessGuard {
    type Target = AccessPolicy;

    fn deref(&self) -> &Self::Target {
        &self.snapshot.policy
    }
}

impl AccessGuard {
    #[must_use]
    pub fn snapshot(&self) -> &AccessSnapshot {
        &self.snapshot
    }
}

impl AccessSnapshot {
    /// A digest of what decides access to one environment of a project on
    /// this host: the host policy, the environment's access record, and its
    /// blocks. A sync, a deployment's access change, a block, or a change
    /// of the host policy changes it; a change to another environment's
    /// access does not.
    #[must_use]
    pub fn environment_access(&self, project: ProjectId, environment: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mm-environment-access-v1\0");
        let mut part = |bytes: Vec<u8>| {
            hasher.update(&(bytes.len() as u64).to_le_bytes());
            hasher.update(&bytes);
        };
        part(serde_json::to_vec(&self.host).unwrap_or_default());
        for record in self
            .records
            .iter()
            .filter(|record| record.project_id == project && record.environment == environment)
        {
            part(serde_json::to_vec(record).unwrap_or_default());
        }
        for block in self
            .blocks
            .iter()
            .filter(|block| block.project_id == project && block.environment == environment)
        {
            part(serde_json::to_vec(block).unwrap_or_default());
        }
        hasher.finalize().to_hex().to_string()
    }
}

impl HostAccessStore {
    #[must_use]
    pub fn new(config: &crate::adapters::remote::DaemonConfig) -> Self {
        Self {
            host_policy_path: config.host_policy_path.clone(),
            state_root: config.state_root.clone(),
            authorized_keys_path: config.authorized_keys_path.clone(),
            relay_command: config.relay_command.clone(),
        }
    }

    #[must_use]
    pub fn host_policy_path(&self) -> &Path {
        &self.host_policy_path
    }

    pub fn load_host(&self) -> Result<(HostPolicy, Vec<u8>), MmError> {
        load_host_policy(&self.host_policy_path)
    }

    pub fn load(&self) -> Result<AccessSnapshot, MmError> {
        let (host, host_bytes) = self.load_host().map_err(|error| self.unusable(error))?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mm-access-revision-v1\0");
        hasher.update(&host_bytes);
        let mut records = Vec::new();
        let mut blocks = Vec::new();
        for project in host.projects.keys() {
            let directory = self.project_directory(*project).join("access");
            for (path, bytes) in read_json_files(&directory)? {
                let record: EnvironmentAccessRecord =
                    serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
                        path: path.clone(),
                        message: error.to_string(),
                    })?;
                if record.project_id != *project
                    || path.file_stem().and_then(|stem| stem.to_str())
                        != Some(record.environment.as_str())
                {
                    return Err(security(
                        "access record does not match its project or environment",
                    ));
                }
                hasher.update(&(bytes.len() as u64).to_le_bytes());
                hasher.update(&bytes);
                records.push(record);
            }
            let path = self.blocks_path(*project);
            if let Some(bytes) = read_optional(&path)? {
                let project_blocks: Vec<EmergencyBlock> =
                    serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
                        path: path.clone(),
                        message: error.to_string(),
                    })?;
                hasher.update(&(bytes.len() as u64).to_le_bytes());
                hasher.update(&bytes);
                blocks.extend(
                    project_blocks
                        .into_iter()
                        .filter(|block| block.project_id == *project),
                );
            }
        }
        let digest = hasher.finalize();
        let mut revision = [0_u8; 8];
        revision.copy_from_slice(&digest.as_bytes()[..8]);
        let policy = effective_policy(&host, &records, &blocks, u64::from_le_bytes(revision));
        policy
            .validate()
            .map_err(|message| MmError::Security { message })?;
        Ok(AccessSnapshot {
            host,
            records,
            blocks,
            policy,
        })
    }

    /// Takes the lock every change of access state takes, in this daemon
    /// and in `mmd access …` run as root. The wait never stops the thread
    /// and ends after `LOCK_PATIENCE` with an error that names the holder.
    /// `what` is this holder's own note.
    async fn hold(&self, what: &str) -> Result<HeldLock, MmError> {
        create_private_directory(&self.state_root)?;
        let path = self.state_root.join("access.lock");
        let lock = private_lock_file(&path)?;
        file_lock::exclusive_async(&lock, &path, LOCK_PATIENCE).await?;
        file_lock::note_holder(&lock, what);
        Ok(HeldLock(lock))
    }

    /// Runs `section` with access state locked and freshly read. The
    /// section is not async, so the lock is released before anything
    /// else is awaited. `what` says who holds the lock, for a process that
    /// waits for it in vain.
    pub async fn locked<T>(
        &self,
        what: &str,
        section: impl FnOnce(&AccessGuard) -> Result<T, MmError>,
    ) -> Result<T, MmError> {
        let lock = self.hold(what).await?;
        let guard = AccessGuard {
            snapshot: self.load()?,
            _lock: lock,
        };
        section(&guard)
    }

    /// Rewrites sshd's `authorized_keys` from the policy as it is now.
    pub async fn render_current_keys(&self) -> Result<(), MmError> {
        self.locked("mmd access render-keys", |guard| {
            self.render_authorized_keys(guard)
        })
        .await
    }

    /// Blocks a key in one environment at once, or lifts the block, as
    /// host root (`mmd access block` and `unblock`).
    pub async fn set_block(
        &self,
        project: ProjectId,
        environment: &str,
        fingerprint: &str,
        blocked: bool,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        self.locked("mmd access block", |guard| {
            let snapshot = guard.snapshot();
            if !snapshot
                .host
                .projects
                .get(&project)
                .is_some_and(|enrolled| enrolled.environments.contains(environment))
            {
                return Err(MmError::Environment {
                    message: format!(
                        "this host does not run {environment} for project {}",
                        project.0
                    ),
                });
            }
            let mut blocks = snapshot
                .blocks
                .iter()
                .filter(|entry| entry.project_id == project)
                .filter(|entry| {
                    !(entry.environment == environment && entry.fingerprint == fingerprint)
                })
                .cloned()
                .collect::<Vec<_>>();
            if blocked {
                blocks.push(EmergencyBlock {
                    project_id: project,
                    environment: environment.to_owned(),
                    fingerprint: fingerprint.to_owned(),
                    blocked_by: "host-root".to_owned(),
                    blocked_at_unix_ms: now_unix_ms,
                });
            }
            self.write_blocks(guard, project, &blocks)?;
            self.render_authorized_keys(&self.load()?.policy)
        })
        .await
    }

    /// Enrolls a project under the access lock, creating the host policy on
    /// a new host. Returns `false` when the policy already said the same.
    /// The file is rewritten from its parsed form, so comments are not kept.
    pub async fn enroll(&self, enrollment: &ProjectEnrollment) -> Result<bool, MmError> {
        let _lock = self.hold("mmd access enroll").await?;
        let current = match fs::symlink_metadata(&self.host_policy_path) {
            Ok(_) => Some(self.load_host()?.0),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("inspecting host policy {}", self.host_policy_path.display()),
                    source,
                });
            }
        };
        let mut next = current.clone().unwrap_or_else(HostPolicy::empty);
        next.enroll(enrollment)
            .map_err(|message| MmError::ConfigParse {
                path: self.host_policy_path.clone(),
                message,
            })?;
        if current.as_ref() == Some(&next) {
            return Ok(false);
        }
        let yaml = serde_yaml_ng::to_string(&next).map_err(|error| MmError::State {
            message: format!("serializing host policy: {error}"),
        })?;
        create_private_directory(
            self.host_policy_path
                .parent()
                .ok_or_else(|| security("host policy has no parent directory"))?,
        )?;
        write_atomic(
            &self.host_policy_path,
            format!(
                "# Host policy for mmd. `mm target setup` enrolls projects here; edit by hand for ceilings and exceptions.\n{yaml}"
            ),
            0o600,
        )?;
        self.load_host()?;
        Ok(true)
    }

    /// A host policy that does not load, as every request is refused with
    /// it: what is wrong, and that only the requests wait for the fix.
    fn unusable(&self, error: MmError) -> MmError {
        let path = self.host_policy_path.display();
        let fix = format!(
            "mmd on this host refuses every request until {path} loads again; the containers and their names are not affected. Fix the file as root, check it with `mmd access validate`, and run the command again"
        );
        match error {
            MmError::ConfigParse { path, message } => MmError::ConfigParse {
                path,
                message: format!("{message}. {fix}"),
            },
            MmError::Security { message } => MmError::Security {
                message: format!("host policy {path}: {message}. {fix}"),
            },
            other => other,
        }
    }

    #[must_use]
    pub fn project_directory(&self, project: ProjectId) -> PathBuf {
        self.state_root.join("projects").join(project.0.to_string())
    }

    fn record_path(&self, project: ProjectId, environment: &str) -> PathBuf {
        self.project_directory(project)
            .join("access")
            .join(format!("{environment}.json"))
    }

    fn blocks_path(&self, project: ProjectId) -> PathBuf {
        self.project_directory(project).join("blocks.json")
    }

    /// Replaces one environment's reconciled users. The caller holds the
    /// guard, so the change is serialized with every other access change.
    pub fn write_record(
        &self,
        _guard: &AccessGuard,
        record: &EnvironmentAccessRecord,
    ) -> Result<(), MmError> {
        let path = self.record_path(record.project_id, &record.environment);
        create_private_directory(path.parent().unwrap_or(&self.state_root))?;
        write_atomic(&path, &to_json(record)?, 0o600)
    }

    pub fn write_blocks(
        &self,
        _guard: &AccessGuard,
        project: ProjectId,
        blocks: &[EmergencyBlock],
    ) -> Result<(), MmError> {
        let path = self.blocks_path(project);
        create_private_directory(path.parent().unwrap_or(&self.state_root))?;
        write_atomic(&path, &to_json(&blocks)?, 0o600)
    }

    /// Writes `authorized_keys` from the current effective policy. Each line
    /// is restricted and forced to the relay, so sshd alone already refuses
    /// shells, forwarding, and PTYs.
    pub fn render_authorized_keys(&self, policy: &AccessPolicy) -> Result<(), MmError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| {
                i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
            });
        let mut contents = String::from(
            "# Managed by mmd from the host policy and reconciled users files. Do not edit.\n",
        );
        for (key, expiry) in policy.authorized_keys(now) {
            let mut options = format!("restrict,command=\"{}\"", self.relay_command);
            if let Some(expiry) = expiry.and_then(expiry_time) {
                options.push_str(&format!(",expiry-time=\"{expiry}\""));
            }
            contents.push_str(&format!("{options} {key}\n"));
        }
        if let Some(parent) = self.authorized_keys_path.parent() {
            fs::create_dir_all(parent).map_err(|source| MmError::Io {
                context: format!("creating {}", parent.display()),
                source,
            })?;
        }
        if read_optional(&self.authorized_keys_path)?.as_deref() == Some(contents.as_bytes()) {
            return Ok(());
        }
        write_atomic(&self.authorized_keys_path, &contents, 0o644)
    }
}

/// The commit running in one environment and where its checkout lives.
/// Unknown keys are ignored: earlier releases wrote one more.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DeployedState {
    pub project_id: ProjectId,
    pub environment: String,
    pub commit: String,
    pub root: PathBuf,
    pub deployed_at_unix_ms: i64,
    pub deployed_by: String,
    /// Whether the deployment placed services on this host. `false` on a
    /// cluster node that runs nothing. Records written before clusters
    /// existed have no key and ran services.
    #[serde(default = "runs_services")]
    pub runs_services: bool,
}

const fn runs_services() -> bool {
    true
}

impl HostAccessStore {
    fn deployed_path(&self, project: ProjectId, environment: &str) -> PathBuf {
        self.project_directory(project)
            .join("deployed")
            .join(format!("{environment}.json"))
    }

    pub fn deployed(
        &self,
        project: ProjectId,
        environment: &str,
    ) -> Result<Option<DeployedState>, MmError> {
        let path = self.deployed_path(project, environment);
        let Some(bytes) = read_optional(&path)? else {
            return Ok(None);
        };
        let state: DeployedState =
            serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
                path: path.clone(),
                message: error.to_string(),
            })?;
        if state.project_id != project
            || state.environment != environment
            || !state.root.is_absolute()
        {
            return Err(security(
                "deployment record does not match its project or environment",
            ));
        }
        Ok(Some(state))
    }

    pub fn write_deployed(
        &self,
        _guard: &AccessGuard,
        state: &DeployedState,
    ) -> Result<(), MmError> {
        let path = self.deployed_path(state.project_id, &state.environment);
        create_private_directory(path.parent().unwrap_or(&self.state_root))?;
        write_atomic(&path, &to_json(state)?, 0o600)
    }
}

/// Reads and checks `/etc/mm/host.yaml`, which must be daemon-owned 0600.
pub fn load_host_policy(path: &Path) -> Result<(HostPolicy, Vec<u8>), MmError> {
    validate_private_file(path)?;
    let bytes = fs::read(path).map_err(|source| MmError::Io {
        context: format!("reading host policy {}", path.display()),
        source,
    })?;
    if bytes.len() > MAX_POLICY_BYTES {
        return Err(security("host policy exceeds 4 MiB"));
    }
    let duplicate = |error: serde_yaml_ng::Error| MmError::ConfigParse {
        path: path.to_path_buf(),
        message: crate::domain::host_policy::duplicate_key_message(
            &String::from_utf8_lossy(&bytes),
            &error.to_string(),
        ),
    };
    let value = serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&bytes).map_err(duplicate)?;
    // YAML merge keys can otherwise obscure the authoritative value being reviewed.
    if contains_tag_or_merge(&value) {
        return Err(security(
            "host policy cannot contain YAML tags or merge keys",
        ));
    }
    if value.get("schema_version").is_some()
        || value
            .get("schema-version")
            .and_then(serde_yaml_ng::Value::as_u64)
            == Some(1)
    {
        return Err(security(
            "this is a schema-1 access.yaml; mmd now reads /etc/mm/host.yaml (schema-version: 2) and takes users from each project's users file. See the migration guide in the remote access docs.",
        ));
    }
    let host: HostPolicy = serde_yaml_ng::from_slice(&bytes).map_err(duplicate)?;
    host.validate().map_err(|message| MmError::ConfigParse {
        path: path.to_path_buf(),
        message,
    })?;
    Ok((host, bytes))
}

/// `expiry-time` takes `YYYYMMDD`; keys expire at the start of that UTC day.
fn expiry_time(unix_ms: i64) -> Option<String> {
    let days = unix_ms.div_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    (year >= 1970).then(|| format!("{year:04}{month:02}{day:02}"))
}

/// Howard Hinnant's days-to-civil conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if month <= 2 { y + 1 } else { y }, month, day)
}

fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<String, MmError> {
    let mut text = serde_json::to_string_pretty(value).map_err(|error| MmError::State {
        message: format!("serializing access state: {error}"),
    })?;
    text.push('\n');
    Ok(text)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, MmError> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() > MAX_POLICY_BYTES => {
            Err(security("access state file exceeds 4 MiB"))
        }
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        }),
    }
}

fn read_json_files(directory: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, MmError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("reading {}", directory.display()),
                source,
            });
        }
    };
    let mut files = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| MmError::Io {
                context: format!("reading {}", directory.display()),
                source,
            })?
            .path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && let Some(bytes) = read_optional(&path)?
        {
            files.push((path, bytes));
        }
    }
    files.sort();
    Ok(files)
}

fn create_private_directory(path: &Path) -> Result<(), MmError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).map_err(|source| MmError::Io {
        context: format!("creating {}", path.display()),
        source,
    })
}

fn validate_private_file(path: &Path) -> Result<(), MmError> {
    let parent = path
        .parent()
        .ok_or_else(|| security("host access state has no parent directory"))?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(|source| MmError::Io {
        context: format!(
            "inspecting host access state directory {}",
            parent.display()
        ),
        source,
    })?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting host access state {}", path.display()),
        source,
    })?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.file_type().is_symlink()
    {
        return Err(security(
            "host access state and parent must be regular non-symlink objects",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let uid = rustix::process::getuid().as_raw();
        if parent_metadata.uid() != uid
            || metadata.uid() != uid
            || parent_metadata.permissions().mode() & 0o022 != 0
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(security(
                "host access state must be daemon-owned mode 0600 under a daemon-owned non-writable directory",
            ));
        }
    }
    Ok(())
}

fn private_lock_file(path: &Path) -> Result<fs::File, MmError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|source| MmError::Io {
        context: format!("opening host access state lock {}", path.display()),
        source,
    })
}

fn write_atomic(path: &Path, contents: impl AsRef<[u8]>, mode: u32) -> Result<(), MmError> {
    let parent = path
        .parent()
        .ok_or_else(|| security("access state has no parent directory"))?;
    let temporary = parent.join(format!(".mm-write.{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _unused_mode = mode;
    let result = (|| {
        let mut file = options.open(&temporary).map_err(|source| MmError::Io {
            context: format!(
                "creating temporary host access state {}",
                temporary.display()
            ),
            source,
        })?;
        // The creation mode is reduced by the umask, and mmd.service runs
        // with 0077; sshd reads authorized_keys as the relay account.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))
                .map_err(|source| MmError::Io {
                    context: format!(
                        "setting the mode of temporary host access state {}",
                        temporary.display()
                    ),
                    source,
                })?;
        }
        file.write_all(contents.as_ref())
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!(
                    "writing temporary host access state {}",
                    temporary.display()
                ),
                source,
            })?;
        fs::rename(&temporary, path).map_err(|source| MmError::Io {
            context: format!("publishing host access state {}", path.display()),
            source,
        })?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("syncing host access state directory {}", parent.display()),
                source,
            })
    })();
    match result {
        Ok(()) => Ok(()),
        Err(primary) => match fs::remove_file(&temporary) {
            Ok(()) => Err(primary),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(primary),
            Err(cleanup) => Err(MmError::operation_failure(
                path.display().to_string(),
                crate::domain::error::MutationStatus::Unknown,
                primary,
                vec![cleanup.to_string()],
                "remove the temporary file and verify the published access state before retrying",
            )),
        },
    }
}

fn contains_tag_or_merge(value: &serde_yaml_ng::Value) -> bool {
    match value {
        serde_yaml_ng::Value::Tagged(_) => true,
        serde_yaml_ng::Value::Mapping(mapping) => mapping.iter().any(|(key, value)| {
            key.as_str() == Some("<<") || contains_tag_or_merge(key) || contains_tag_or_merge(value)
        }),
        serde_yaml_ng::Value::Sequence(values) => values.iter().any(contains_tag_or_merge),
        _ => false,
    }
}

fn security(message: &str) -> MmError {
    MmError::Security {
        message: message.to_owned(),
    }
}

mod deploy_key;
mod rollout;
pub use rollout::StoredRollout;

#[cfg(test)]
#[path = "access/tests.rs"]
mod tests;
