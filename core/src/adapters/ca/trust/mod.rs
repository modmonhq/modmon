mod linux;
mod macos;
mod windows;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_user_controlled_directory, read_regular_file,
};
use crate::app::ports::{CommandOutput, CommandSpec, ProcessRunner, TrustStore};
use crate::domain::ca::{TrustScope, TrustStoreStatus, certificate_fingerprint};
use crate::domain::error::MmError;

const OWNERSHIP_FILE: &str = "ownership.json";
const MAX_OWNERSHIP_BYTES: usize = 64 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TrustOwnership {
    entries: BTreeMap<String, OwnedTrustEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OwnedTrustEntry {
    platform: String,
    scope: TrustScope,
    entry: String,
}

#[derive(Clone)]
pub struct PlatformTrustStore {
    root: PathBuf,
    process: Arc<dyn ProcessRunner>,
    scope: TrustScope,
    platform: Platform,
    #[cfg(test)]
    test_status: Option<BackendStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Platform {
    Linux,
    Macos,
    Windows,
    Unsupported,
    #[cfg(test)]
    Test,
}

impl std::fmt::Debug for PlatformTrustStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformTrustStore")
            .field("root", &self.root)
            .field("scope", &self.scope)
            .field("platform", &self.platform_name())
            .finish_non_exhaustive()
    }
}

impl PlatformTrustStore {
    /// Called only by an interactive setup presenter. Sudo owns the password
    /// prompt; credentials never enter mm's input buffers or command arguments.
    pub async fn authorize_interactive(&self) -> Result<(), MmError> {
        if self.scope != TrustScope::System || !matches!(self.platform, Platform::Linux | Platform::Macos) {
            return Ok(());
        }
        let output = self.process.run(CommandSpec::new("/usr/bin/sudo")
            .args(["-v"]).inherit_stdin().timeout(Duration::from_secs(120))).await?;
        if output.success() { Ok(()) } else {
            Err(MmError::Security { message: "sudo authorization was cancelled or failed; run `sudo -v` in this terminal and retry setup".to_owned() })
        }
    }

    #[must_use]
    pub fn new(root: PathBuf, process: Arc<dyn ProcessRunner>, scope: TrustScope) -> Self {
        Self {
            root,
            process,
            scope,
            platform: current_platform(),
            #[cfg(test)]
            test_status: None,
        }
    }

    #[cfg(test)]
    fn for_platform(
        root: PathBuf,
        process: Arc<dyn ProcessRunner>,
        scope: TrustScope,
        platform: Platform,
    ) -> Self {
        Self {
            root,
            process,
            scope,
            platform,
            test_status: None,
        }
    }

    #[cfg(test)]
    fn for_test(root: PathBuf, process: Arc<dyn ProcessRunner>, status: BackendStatus) -> Self {
        Self {
            root,
            process,
            scope: TrustScope::CurrentUser,
            platform: Platform::Test,
            test_status: Some(status),
        }
    }

    fn platform_name(&self) -> &'static str {
        match self.platform {
            Platform::Linux => "linux",
            Platform::Macos => "macos",
            Platform::Windows => "windows",
            Platform::Unsupported => "unsupported",
            #[cfg(test)]
            Platform::Test => "test",
        }
    }

    fn load_ownership(&self) -> Result<TrustOwnership, MmError> {
        let Some(bytes) = read_regular_file(&self.root.join(OWNERSHIP_FILE), MAX_OWNERSHIP_BYTES)?
        else {
            return Ok(TrustOwnership::default());
        };
        serde_json::from_slice(&bytes).map_err(|error| MmError::State {
            message: format!("trust-store ownership is invalid: {error}"),
        })
    }

    fn save_ownership(&self, ownership: &TrustOwnership) -> Result<(), MmError> {
        self.prepare_root()?;
        let mut bytes = serde_json::to_vec_pretty(ownership).map_err(|error| MmError::State {
            message: format!("could not serialize trust ownership: {error}"),
        })?;
        bytes.push(b'\n');
        write_private(&self.root.join(OWNERSHIP_FILE), &bytes)
    }

    fn prepare_certificate(&self, pem: &str, fingerprint: &str) -> Result<PathBuf, MmError> {
        let actual = certificate_fingerprint(pem.as_bytes()).map_err(trust_error)?;
        if actual.to_string() != fingerprint {
            return Err(MmError::Security {
                message: "refusing to install a CA whose DER fingerprint does not match the reviewed fingerprint"
                    .to_owned(),
            });
        }
        self.prepare_root()?;
        let path = self
            .root
            .join(format!("{}.crt", fingerprint_token(fingerprint)?));
        write_private(&path, pem.as_bytes())?;
        Ok(path)
    }

    fn prepare_root(&self) -> Result<(), MmError> {
        ensure_no_symlink_components(&self.root)?;
        fs::create_dir_all(&self.root).map_err(|source| MmError::Io {
            context: format!("creating trust metadata directory {}", self.root.display()),
            source,
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.root, fs::Permissions::from_mode(0o700)).map_err(
                |source| MmError::Io {
                    context: format!("setting trust metadata permissions {}", self.root.display()),
                    source,
                },
            )?;
        }
        ensure_user_controlled_directory(&self.root)
    }

    async fn actual_status(&self, fingerprint: &str) -> Result<BackendStatus, MmError> {
        match self.platform {
            Platform::Linux => linux::status(fingerprint).await,
            Platform::Macos => macos::status(self, fingerprint).await,
            Platform::Windows => windows::status(self, fingerprint).await,
            Platform::Unsupported => Ok(BackendStatus::unsupported()),
            #[cfg(test)]
            Platform::Test => self.test_status.clone().ok_or_else(|| MmError::State {
                message: "fake trust status is missing".to_owned(),
            }),
        }
    }

    async fn install_backend(
        &self,
        certificate: &Path,
        fingerprint: &str,
    ) -> Result<BackendStatus, MmError> {
        match self.platform {
            Platform::Linux => linux::install(self, certificate, fingerprint).await,
            Platform::Macos => macos::install(self, certificate, fingerprint).await,
            Platform::Windows => windows::install(self, certificate, fingerprint).await,
            Platform::Unsupported => Err(unsupported_error()),
            #[cfg(test)]
            Platform::Test => Err(MmError::State {
                message: "fake trust mutation should not be reached".to_owned(),
            }),
        }
    }

    async fn uninstall_backend(&self, fingerprint: &str, entry: &str) -> Result<(), MmError> {
        match self.platform {
            Platform::Linux => linux::uninstall(self, fingerprint, entry).await,
            Platform::Macos => macos::uninstall(self, fingerprint).await,
            Platform::Windows => windows::uninstall(self, fingerprint).await,
            Platform::Unsupported => Err(unsupported_error()),
            #[cfg(test)]
            Platform::Test => Err(MmError::State {
                message: "fake trust deletion should not be reached".to_owned(),
            }),
        }
    }

    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let output = self
            .process
            .run(specification.timeout(COMMAND_TIMEOUT))
            .await?;
        if output.truncated {
            return Err(MmError::Security {
                message:
                    "trust-store command output exceeded mm's safety bound; no result was accepted"
                        .to_owned(),
            });
        }
        Ok(output)
    }

    async fn run_checked(
        &self,
        specification: CommandSpec,
        action: &str,
    ) -> Result<CommandOutput, MmError> {
        let program = specification.redacted_record().program;
        let output = self.run(specification).await?;
        if !output.success() {
            return Err(MmError::Target {
                message: format!(
                    "{action} failed using allowlisted executable '{program}' (exit {:?}): {}",
                    output.code,
                    bounded_detail(&output.stderr)
                ),
            });
        }
        Ok(output)
    }

    fn status_value(
        &self,
        expected: Option<&str>,
        backend: BackendStatus,
    ) -> Result<TrustStoreStatus, MmError> {
        let ownership = self.load_ownership()?;
        let owned = expected.and_then(|value| ownership.entries.get(value));
        let owned_for_platform = owned.is_some_and(|entry| {
            entry.platform == self.platform_name() && entry.scope == self.scope
        });
        let installed = backend.installed && owned_for_platform;
        Ok(TrustStoreStatus {
            supported: backend.supported,
            installed,
            platform: self.platform_name().to_owned(),
            scope: self.scope,
            fingerprint: installed.then(|| expected.unwrap_or_default().to_owned()),
            mismatch: backend.mismatch || (backend.installed && !owned_for_platform),
            needs_privilege: backend.needs_privilege,
            entry: installed.then_some(backend.entry),
        })
    }
}

#[async_trait]
impl TrustStore for PlatformTrustStore {
    async fn secure_private_keys(&self) -> Result<(), MmError> {
        match self.platform {
            Platform::Windows => windows::secure_private_keys(self).await,
            Platform::Linux | Platform::Macos => {
                secure_private_keys_unix(&self.root)?;
                Ok(())
            }
            Platform::Unsupported => Err(unsupported_error()),
            #[cfg(test)]
            Platform::Test => Ok(()),
        }
    }

    async fn private_keys_valid(&self) -> Result<bool, MmError> {
        match self.platform {
            Platform::Windows => windows::private_keys_valid(self).await,
            Platform::Linux | Platform::Macos => private_keys_valid_unix(&self.root),
            Platform::Unsupported => Ok(false),
            #[cfg(test)]
            Platform::Test => Ok(true),
        }
    }

    async fn status(&self, fingerprint: Option<&str>) -> Result<TrustStoreStatus, MmError> {
        let backend = match fingerprint {
            Some(fingerprint) => self.actual_status(fingerprint).await?,
            None => BackendStatus::empty(self.platform != Platform::Unsupported),
        };
        self.status_value(fingerprint, backend)
    }

    async fn install(&self, pem: &str, fingerprint: &str) -> Result<TrustStoreStatus, MmError> {
        let current = self.actual_status(fingerprint).await?;
        let ownership = self.load_ownership()?;
        if ownership.entries.get(fingerprint).is_some_and(|owned| {
            owned.platform != self.platform_name() || owned.scope != self.scope
        }) {
            return Err(MmError::Security {
                message: "the CA is already owned in another platform trust scope; use that scope for lifecycle operations"
                    .to_owned(),
            });
        }
        if current.installed {
            if ownership.entries.get(fingerprint).is_some_and(|owned| {
                owned.platform == self.platform_name()
                    && owned.scope == self.scope
                    && owned.entry == current.entry
            }) {
                return self.status_value(Some(fingerprint), current);
            }
            return Err(MmError::Security {
                message: "the exact certificate is already trusted without matching mm ownership metadata; refusing to adopt it"
                    .to_owned(),
            });
        }
        if current.mismatch {
            return Err(MmError::Security {
                message: "the mm trust-store entry contains a different certificate; refusing to replace a foreign same-name entry"
                    .to_owned(),
            });
        }
        if !current.supported {
            return Err(unsupported_error());
        }
        let certificate = self.prepare_certificate(pem, fingerprint)?;
        let installed = match self.install_backend(&certificate, fingerprint).await {
            Ok(installed) => installed,
            Err(error) => {
                return Err(with_prepared_cleanup(error, &self.root, fingerprint));
            }
        };
        if !installed.installed {
            return Err(with_prepared_cleanup(
                MmError::Security {
                    message: "the platform trust command succeeded but the actual store did not contain the expected DER certificate"
                        .to_owned(),
                },
                &self.root,
                fingerprint,
            ));
        }
        let mut ownership = ownership;
        ownership.entries.insert(
            fingerprint.to_owned(),
            OwnedTrustEntry {
                platform: self.platform_name().to_owned(),
                scope: self.scope,
                entry: installed.entry.clone(),
            },
        );
        if let Err(error) = self.save_ownership(&ownership) {
            let rollback = self.uninstall_backend(fingerprint, &installed.entry).await;
            return match rollback {
                Ok(()) if !self.actual_status(fingerprint).await?.installed => {
                    Err(with_prepared_cleanup(error, &self.root, fingerprint))
                }
                Ok(()) => Err(MmError::Security {
                    message: format!(
                        "trust installation ownership failed with {} and the exact certificate remained trusted after rollback",
                        error.code()
                    ),
                }),
                Err(rollback) => Err(MmError::Security {
                    message: format!(
                        "trust installation ownership failed with {} and exact trust rollback failed with {}",
                        error.code(),
                        rollback.code()
                    ),
                }),
            };
        }
        self.status_value(Some(fingerprint), installed)
    }

    async fn repair(&self, pem: &str, fingerprint: &str) -> Result<TrustStoreStatus, MmError> {
        let status = self.status(Some(fingerprint)).await?;
        if status.installed {
            return Ok(status);
        }
        if status.mismatch {
            return Err(MmError::Security {
                message: "trust repair refused a mismatched same-name platform entry".to_owned(),
            });
        }
        self.install(pem, fingerprint).await
    }

    async fn uninstall(&self, fingerprint: &str) -> Result<TrustStoreStatus, MmError> {
        let mut ownership = self.load_ownership()?;
        let Some(owned) = ownership.entries.get(fingerprint).cloned() else {
            let actual = self.actual_status(fingerprint).await?;
            if actual.installed {
                return Err(MmError::Security {
                    message: "the requested certificate exists in the platform store but has no mm ownership record; refusing removal"
                        .to_owned(),
                });
            }
            remove_prepared_certificate(&self.root, fingerprint)?;
            return self.status_value(Some(fingerprint), actual);
        };
        if owned.platform != self.platform_name() || owned.scope != self.scope {
            return Err(MmError::Security {
                message: "the owned trust entry belongs to another platform or trust scope"
                    .to_owned(),
            });
        }
        let before = self.actual_status(fingerprint).await?;
        if before.mismatch {
            return Err(MmError::Security {
                message: "the owned trust entry was replaced by a different certificate; refusing removal"
                    .to_owned(),
            });
        }
        if before.installed {
            self.uninstall_backend(fingerprint, &owned.entry).await?;
        }
        let after = self.actual_status(fingerprint).await?;
        if after.installed {
            return Err(MmError::Security {
                message: "platform trust removal returned success but the exact certificate remains installed"
                    .to_owned(),
            });
        }
        ownership.entries.remove(fingerprint);
        self.save_ownership(&ownership)?;
        remove_prepared_certificate(&self.root, fingerprint)?;
        self.status_value(Some(fingerprint), after)
    }
}

#[derive(Debug, Clone)]
struct BackendStatus {
    supported: bool,
    installed: bool,
    mismatch: bool,
    needs_privilege: bool,
    entry: String,
}

impl BackendStatus {
    fn empty(supported: bool) -> Self {
        Self {
            supported,
            installed: false,
            mismatch: false,
            needs_privilege: false,
            entry: String::new(),
        }
    }

    fn unsupported() -> Self {
        Self::empty(false)
    }
}

fn current_platform() -> Platform {
    if cfg!(target_os = "linux") {
        Platform::Linux
    } else if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(target_os = "windows") {
        Platform::Windows
    } else {
        Platform::Unsupported
    }
}

fn fingerprint_token(fingerprint: &str) -> Result<&str, MmError> {
    let token = fingerprint
        .strip_prefix("sha256:")
        .ok_or_else(|| MmError::Security {
            message: "trust-store fingerprint does not use the sha256 prefix".to_owned(),
        })?;
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(MmError::Security {
            message: "trust-store fingerprint is not a SHA-256 digest".to_owned(),
        });
    }
    Ok(token)
}

fn with_prepared_cleanup(primary: MmError, root: &Path, fingerprint: &str) -> MmError {
    match remove_prepared_certificate(root, fingerprint) {
        Ok(()) => primary,
        Err(cleanup) => MmError::operation_failure(
            fingerprint,
            crate::domain::error::MutationStatus::Unknown,
            primary,
            vec![cleanup.to_string()],
            "remove the prepared CA certificate and verify the platform trust store before retrying",
        ),
    }
}

fn remove_prepared_certificate(root: &Path, fingerprint: &str) -> Result<(), MmError> {
    let path = root.join(format!("{}.crt", fingerprint_token(fingerprint)?));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(MmError::Io {
            context: format!("removing trust metadata certificate {}", path.display()),
            source,
        }),
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    let temporary = path.with_extension("tmp");
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|source| MmError::Io {
            context: format!("removing incomplete trust metadata {}", temporary.display()),
            source,
        })?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    let mut file = options.open(&temporary).map_err(|source| MmError::Io {
        context: format!("creating trust metadata {}", temporary.display()),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing trust metadata {}", temporary.display()),
            source,
        })?;
    fs::rename(&temporary, path).map_err(|source| MmError::Io {
        context: format!("publishing trust metadata {}", path.display()),
        source,
    })
}

fn bounded_detail(value: &str) -> String {
    match value.lines().next() {
        Some(line) => crate::domain::error::printed(&line.chars().take(512).collect::<String>()),
        None => "no diagnostic output".to_owned(),
    }
}

fn trust_error(error: impl std::fmt::Display) -> MmError {
    MmError::Target {
        message: format!("trust-store operation failed: {error}"),
    }
}

fn unsupported_error() -> MmError {
    MmError::Target {
        message: "this platform has no implemented mm trust-store adapter; trust was not installed"
            .to_owned(),
    }
}

#[cfg(unix)]
fn private_key_paths_unix(trust_root: &Path) -> Result<Vec<PathBuf>, MmError> {
    let ca_root = trust_root
        .parent()
        .ok_or_else(|| MmError::Security {
            message: "trust metadata root has no MM_HOME parent".to_owned(),
        })?
        .join("ca");
    ensure_no_symlink_components(&ca_root)?;
    let mut keys = vec![ca_root.join("ca.key")];
    let issued = ca_root.join("issued");
    ensure_no_symlink_components(&issued)?;
    if issued.is_dir() {
        for entry in fs::read_dir(&issued).map_err(|source| MmError::Io {
            context: format!("reading local certificate directory {}", issued.display()),
            source,
        })? {
            let entry = entry.map_err(|source| MmError::Io {
                context: format!("reading local certificate directory {}", issued.display()),
                source,
            })?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "key")
            {
                ensure_no_symlink_components(&entry.path())?;
                keys.push(entry.path());
            }
        }
    }
    Ok(keys)
}

#[cfg(unix)]
fn secure_private_keys_unix(trust_root: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    for key in private_key_paths_unix(trust_root)? {
        let metadata = fs::symlink_metadata(&key).map_err(|source| MmError::Io {
            context: format!("inspecting private key {}", key.display()),
            source,
        })?;
        if !metadata.file_type().is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.nlink() != 1
        {
            return Err(MmError::Security {
                message: format!(
                    "local CA key '{}' must be a current-user-owned regular file with one link",
                    key.display()
                ),
            });
        }
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).map_err(|source| {
            MmError::Io {
                context: format!("setting private key permissions on {}", key.display()),
                source,
            }
        })?;
    }
    Ok(())
}

#[cfg(unix)]
fn private_keys_valid_unix(trust_root: &Path) -> Result<bool, MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    for key in private_key_paths_unix(trust_root)? {
        let metadata = match fs::symlink_metadata(&key) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("inspecting private key {}", key.display()),
                    source,
                });
            }
        };
        if !metadata.file_type().is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(not(unix))]
fn secure_private_keys_unix(_trust_root: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: "Unix private-key permissions are unavailable on this platform".to_owned(),
    })
}

#[cfg(not(unix))]
fn private_keys_valid_unix(_trust_root: &Path) -> Result<bool, MmError> {
    Ok(false)
}

#[cfg(test)]
#[path = "tests/tests.rs"]
mod tests;
