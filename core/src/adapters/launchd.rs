//! Fixed-label launchd user agents. Every command targets the current user's
//! `gui/<uid>` domain, so no operation here needs elevation.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_user_controlled_directory, ensure_user_controlled_file,
    read_regular_file,
};
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use crate::domain::error::MmError;
use crate::domain::target::TargetId;

const LAUNCHCTL: &str = "/bin/launchctl";
const MAX_AGENT_FILE_BYTES: usize = 1024 * 1024;
/// launchctl exit statuses observed for an unloaded job: `print` and `bootout`.
const NOT_FOUND_STATUS: i32 = 113;
const NO_SUCH_PROCESS_STATUS: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchdJobState {
    /// The plist may be installed, but launchd has no job: stopped, not broken.
    Unloaded,
    Running,
    /// Loaded but not running. `code` is `None` before the first exit.
    Exited {
        code: Option<i32>,
    },
}

impl LaunchdJobState {
    #[must_use]
    pub const fn loaded(self) -> bool {
        !matches!(self, Self::Unloaded)
    }

    #[must_use]
    pub const fn failed(self) -> bool {
        matches!(self, Self::Exited { code: Some(code) } if code != 0)
    }
}

pub(crate) fn parse_print(stdout: &str) -> LaunchdJobState {
    let field = |name: &str| {
        stdout.lines().find_map(|line| {
            line.strip_prefix('\t')?
                .strip_prefix(name)?
                .strip_prefix(" = ")
                .map(str::trim)
        })
    };
    if field("state") == Some("running") {
        return LaunchdJobState::Running;
    }
    LaunchdJobState::Exited {
        code: field("last exit code").and_then(|code| code.parse().ok()),
    }
}

#[derive(Clone)]
pub struct Launchctl {
    process: Arc<dyn ProcessRunner>,
    domain: String,
}

impl std::fmt::Debug for Launchctl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Launchctl")
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

impl Launchctl {
    #[must_use]
    pub fn current_user(process: Arc<dyn ProcessRunner>) -> Self {
        Self {
            process,
            domain: format!("gui/{}", current_uid()),
        }
    }

    async fn run(&self, arguments: Vec<String>) -> Result<CommandOutput, MmError> {
        self.process
            .run(
                CommandSpec::new(LAUNCHCTL)
                    .args(arguments)
                    .timeout(Duration::from_secs(30)),
            )
            .await
    }

    fn service(&self, label: &str) -> String {
        format!("{}/{label}", self.domain)
    }

    pub async fn state(&self, label: &str) -> Result<LaunchdJobState, MmError> {
        let output = self
            .run(vec!["print".to_owned(), self.service(label)])
            .await?;
        if output.success() {
            return Ok(parse_print(&output.stdout));
        }
        if output.code == Some(NOT_FOUND_STATUS) || output.stderr.contains("Could not find service")
        {
            return Ok(LaunchdJobState::Unloaded);
        }
        Err(failure("print", &output))
    }

    pub async fn bootstrap(&self, plist: &Path) -> Result<(), MmError> {
        let plist = plist.to_str().ok_or_else(|| MmError::Security {
            message: "launchd plist path must be UTF-8".to_owned(),
        })?;
        let output = self
            .run(vec![
                "bootstrap".to_owned(),
                self.domain.clone(),
                plist.to_owned(),
            ])
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(failure("bootstrap", &output))
        }
    }

    /// Unloads the job so `KeepAlive` cannot relaunch it. Already unloaded is success.
    pub async fn bootout(&self, label: &str) -> Result<(), MmError> {
        let output = self
            .run(vec!["bootout".to_owned(), self.service(label)])
            .await?;
        if output.success()
            || matches!(output.code, Some(NO_SUCH_PROCESS_STATUS | NOT_FOUND_STATUS))
        {
            Ok(())
        } else {
            Err(failure("bootout", &output))
        }
    }

    /// Whether the user disabled `label` with `launchctl disable`, which makes
    /// bootstrap fail until it is enabled again.
    pub async fn disabled(&self, label: &str) -> Result<bool, MmError> {
        let output = self
            .run(vec!["print-disabled".to_owned(), self.domain.clone()])
            .await?;
        if !output.success() {
            return Err(failure("print-disabled", &output));
        }
        let entry = format!("\"{label}\" => disabled");
        Ok(output.stdout.lines().any(|line| line.trim() == entry))
    }

    /// Restarts a loaded job in place, keeping its installed definition.
    pub async fn kickstart(&self, label: &str) -> Result<(), MmError> {
        let output = self
            .run(vec![
                "kickstart".to_owned(),
                "-k".to_owned(),
                self.service(label),
            ])
            .await?;
        if output.success() {
            Ok(())
        } else {
            Err(failure("kickstart", &output))
        }
    }
}

fn failure(action: &str, output: &CommandOutput) -> MmError {
    MmError::ProcessFailed {
        program: format!("launchctl {action}"),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

#[cfg(unix)]
pub(crate) fn current_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

#[cfg(not(unix))]
pub(crate) fn current_uid() -> u32 {
    0
}

pub(crate) struct AgentFile {
    pub(crate) destination: PathBuf,
    pub(crate) bytes: Vec<u8>,
    pub(crate) mode: u32,
}

/// One mm-owned agent: its plist plus any files the job reads. The owner record
/// lets repair and removal refuse files mm did not write.
pub(crate) struct AgentInstallation {
    pub(crate) label: String,
    pub(crate) target_id: TargetId,
    pub(crate) artifact_hash: String,
    pub(crate) owner_path: PathBuf,
    pub(crate) plist: AgentFile,
    pub(crate) files: Vec<AgentFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AgentOwner {
    label: String,
    target_id: TargetId,
    artifact_hash: String,
}

impl AgentInstallation {
    fn all_files(&self) -> impl Iterator<Item = &AgentFile> {
        std::iter::once(&self.plist).chain(&self.files)
    }

    fn owner_bytes(&self) -> Result<Vec<u8>, MmError> {
        serde_json::to_vec_pretty(&AgentOwner {
            label: self.label.clone(),
            target_id: self.target_id,
            artifact_hash: self.artifact_hash.clone(),
        })
        .map_err(|error| MmError::State {
            message: format!("could not serialize launchd agent ownership: {error}"),
        })
    }

    /// Refuses another target's agent and unmanaged files at mm's fixed paths.
    fn validate_owner(&self, previous_owner: Option<&[u8]>) -> Result<(), MmError> {
        match previous_owner {
            Some(bytes) => {
                let owner: AgentOwner =
                    serde_json::from_slice(bytes).map_err(|error| MmError::Security {
                        message: format!("launchd agent ownership is invalid: {error}"),
                    })?;
                if owner.label != self.label || owner.target_id != self.target_id {
                    return Err(MmError::Security {
                        message: format!("launchd agent {} is owned by another target", self.label),
                    });
                }
                Ok(())
            }
            None if self.all_files().any(|file| file.destination.exists()) => {
                Err(MmError::Security {
                    message: format!(
                        "refusing to adopt unmanaged files for launchd agent {}",
                        self.label
                    ),
                })
            }
            None => Ok(()),
        }
    }
}

/// What an installation replaced, so a failed verification can restore it.
pub(crate) struct AgentSnapshot {
    label: String,
    plist_path: PathBuf,
    previous: Vec<(PathBuf, Option<Vec<u8>>, u32)>,
    was_loaded: bool,
    changed: bool,
}

impl AgentSnapshot {
    #[cfg(test)]
    pub(crate) const fn changed(&self) -> bool {
        self.changed
    }

    pub(crate) async fn rollback(self, launchctl: &Launchctl) -> Vec<String> {
        let mut failures = Vec::new();
        if !self.changed {
            return failures;
        }
        retain(&mut failures, launchctl.bootout(&self.label).await);
        for (path, bytes, mode) in &self.previous {
            retain(&mut failures, restore_file(path, bytes.as_deref(), *mode));
        }
        let previous_plist = self
            .previous
            .iter()
            .any(|(path, bytes, _)| *path == self.plist_path && bytes.is_some());
        if self.was_loaded && previous_plist {
            retain(&mut failures, launchctl.bootstrap(&self.plist_path).await);
        }
        failures
    }
}

/// Publishes the agent's files and (re)loads it. Unchanged files with a loaded,
/// healthy job are left alone so repeated reconciliation does not restart it.
pub(crate) async fn install_agent(
    launchctl: &Launchctl,
    installation: &AgentInstallation,
) -> Result<AgentSnapshot, MmError> {
    let mut previous = Vec::new();
    let owner = installation.owner_bytes()?;
    let owner_file = AgentFile {
        destination: installation.owner_path.clone(),
        bytes: owner,
        mode: 0o600,
    };
    for file in installation.all_files().chain(std::iter::once(&owner_file)) {
        prepare_parent(&file.destination)?;
        previous.push((
            file.destination.clone(),
            read_existing(&file.destination)?,
            file.mode,
        ));
    }
    let previous_owner = previous.last().and_then(|(_, bytes, _)| bytes.as_deref());
    installation.validate_owner(previous_owner)?;
    let state = launchctl.state(&installation.label).await?;
    let files_unchanged = installation
        .all_files()
        .zip(&previous)
        .all(|(file, (_, existing, _))| existing.as_deref() == Some(file.bytes.as_slice()));
    let mut snapshot = AgentSnapshot {
        label: installation.label.clone(),
        plist_path: installation.plist.destination.clone(),
        previous,
        was_loaded: state.loaded(),
        changed: false,
    };
    if files_unchanged && state.loaded() {
        if state.failed() {
            launchctl.kickstart(&installation.label).await?;
        }
        if snapshot
            .previous
            .last()
            .and_then(|(_, bytes, _)| bytes.as_deref())
            != Some(owner_file.bytes.as_slice())
        {
            atomic_write(&owner_file.destination, &owner_file.bytes, owner_file.mode)?;
        }
        return Ok(snapshot);
    }
    snapshot.changed = true;
    let applied = async {
        for file in installation.all_files().chain(std::iter::once(&owner_file)) {
            atomic_write(&file.destination, &file.bytes, file.mode)?;
        }
        if state.loaded() {
            launchctl.bootout(&installation.label).await?;
        }
        launchctl.bootstrap(&installation.plist.destination).await
    }
    .await;
    match applied {
        Ok(()) => Ok(snapshot),
        Err(error) => Err(with_cleanup(
            &installation.artifact_hash,
            error,
            snapshot.rollback(launchctl).await,
            "restore the mm launchd agent files, then retry",
        )),
    }
}

/// Checks ownership without changing anything, for preflight before mutation.
pub(crate) fn check_agent_owner(installation: &AgentInstallation) -> Result<(), MmError> {
    installation.validate_owner(read_existing(&installation.owner_path)?.as_deref())
}

/// launchd does not create missing directories for `StandardOutPath`.
pub(crate) fn ensure_log_directory(home: &Path) -> Result<(), MmError> {
    let directory = home.join(crate::domain::target_ingress::MACOS_LOG_DIRECTORY);
    ensure_no_symlink_components(&directory)?;
    fs::create_dir_all(&directory).map_err(|source| MmError::Io {
        context: format!("creating mm log directory {}", directory.display()),
        source,
    })?;
    ensure_user_controlled_directory(&directory)
}

/// Unloads and removes an mm-owned agent. Absent agents are already removed.
pub(crate) async fn uninstall_agent(
    launchctl: &Launchctl,
    installation: &AgentInstallation,
) -> Result<(), MmError> {
    let previous_owner = read_existing(&installation.owner_path)?;
    let Some(previous_owner) = previous_owner else {
        installation.validate_owner(None)?;
        return Ok(());
    };
    installation.validate_owner(Some(&previous_owner))?;
    launchctl.bootout(&installation.label).await?;
    for file in installation.all_files() {
        restore_file(&file.destination, None, file.mode)?;
    }
    restore_file(&installation.owner_path, None, 0o600)
}

pub(crate) fn with_cleanup(
    operation: &str,
    error: MmError,
    cleanup: Vec<String>,
    recovery: &str,
) -> MmError {
    if cleanup.is_empty() {
        error
    } else {
        MmError::operation_failure(
            operation,
            crate::domain::error::MutationStatus::Unknown,
            error,
            cleanup,
            recovery,
        )
    }
}

fn retain(failures: &mut Vec<String>, result: Result<(), MmError>) {
    if let Err(error) = result {
        failures.push(error.to_string());
    }
}

fn prepare_parent(path: &Path) -> Result<(), MmError> {
    let parent = path.parent().ok_or_else(|| MmError::Security {
        message: "launchd agent destination has no parent directory".to_owned(),
    })?;
    ensure_no_symlink_components(parent)?;
    fs::create_dir_all(parent).map_err(|source| MmError::Io {
        context: format!("creating launchd agent directory {}", parent.display()),
        source,
    })?;
    ensure_user_controlled_directory(parent)
}

pub(crate) fn read_existing(path: &Path) -> Result<Option<Vec<u8>>, MmError> {
    ensure_no_symlink_components(path)?;
    let existing = read_regular_file(path, MAX_AGENT_FILE_BYTES)?;
    if existing.is_some() {
        ensure_user_controlled_file(path)?;
    }
    Ok(existing)
}

fn restore_file(path: &Path, previous: Option<&[u8]>, mode: u32) -> Result<(), MmError> {
    if let Some(previous) = previous {
        atomic_write(path, previous, mode)
    } else if path.exists() {
        ensure_user_controlled_file(path)?;
        fs::remove_file(path).map_err(|source| MmError::Io {
            context: format!("removing launchd agent file {}", path.display()),
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
        context: format!("opening launchd agent file {}", path.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing launchd agent file {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing launchd agent file {}", path.display()),
        source,
    })?;
    ensure_user_controlled_file(path)
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

#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;
