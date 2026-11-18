use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use age::secrecy::ExposeSecret as _;
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_private_file, ensure_private_permissions_supported,
    ensure_safe_identity_parent, ensure_user_controlled_directory, read_regular_file,
};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

const DOCUMENT_VERSION: u32 = 1;
const MAX_REGISTRY_BYTES: usize = 4 * 1024 * 1024;
const MAX_BLOB_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaultMode {
    Managed,
    External,
}

impl From<crate::domain::secret_backend::ProviderMode> for VaultMode {
    fn from(value: crate::domain::secret_backend::ProviderMode) -> Self {
        match value {
            crate::domain::secret_backend::ProviderMode::Managed => Self::Managed,
            crate::domain::secret_backend::ProviderMode::External => Self::External,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultAppRoleCredential {
    pub role_id: Zeroizing<String>,
    pub secret_id: Zeroizing<String>,
}

impl std::fmt::Debug for VaultAppRoleCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultAppRoleCredential")
            .field("role_id", &"[REDACTED]")
            .field("secret_id", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultConnection {
    pub mode: VaultMode,
    pub address: String,
    pub namespace: Option<String>,
    pub mount: String,
    pub auth_mount: String,
    pub ca_pem: String,
    pub ca_sha256: String,
    pub project_id: ProjectId,
    pub credentials: BTreeMap<String, VaultAppRoleCredential>,
    pub allowed_locations: BTreeMap<String, Vec<String>>,
    pub managed_installation: Option<String>,
}

impl std::fmt::Debug for VaultConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultConnection")
            .field("mode", &self.mode)
            .field("address", &"[PRIVATE]")
            .field("namespace", &self.namespace.as_ref().map(|_| "[PRIVATE]"))
            .field("mount", &self.mount)
            .field("auth_mount", &self.auth_mount)
            .field("ca_pem", &"[PRIVATE]")
            .field("ca_sha256", &self.ca_sha256)
            .field("project_id", &self.project_id)
            .field("environments", &self.credentials.keys().collect::<Vec<_>>())
            .field("managed_installation", &self.managed_installation)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultRecoveryMaterial {
    pub root_token: Zeroizing<String>,
    pub unseal_key: Zeroizing<String>,
}

impl std::fmt::Debug for VaultRecoveryMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultRecoveryMaterial")
            .field("root_token", &"[REDACTED]")
            .field("unseal_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    connections: BTreeMap<String, VaultConnection>,
    recovery: BTreeMap<String, VaultRecoveryMaterial>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: DOCUMENT_VERSION,
            connections: BTreeMap::new(),
            recovery: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VaultConnectionStore {
    identity_path: PathBuf,
    store_path: PathBuf,
    lock_path: PathBuf,
}

impl VaultConnectionStore {
    #[must_use]
    pub fn new(identity_path: PathBuf, store_path: PathBuf) -> Self {
        let lock_path = store_path.with_extension("lock");
        Self {
            identity_path,
            store_path,
            lock_path,
        }
    }

    pub fn connection(&self, name: &str) -> Result<VaultConnection, MmError> {
        validate_name(name)?;
        let _lock = self.lock()?;
        self.read_document()?
            .connections
            .remove(name)
            .ok_or_else(|| MmError::Secret {
                message: format!("private Vault connection '{name}' is not configured"),
            })
    }

    pub fn has_connection(&self, name: &str) -> Result<bool, MmError> {
        validate_name(name)?;
        let _lock = self.lock()?;
        Ok(self.read_or_default()?.connections.contains_key(name))
    }

    pub fn save_connection(&self, name: &str, connection: VaultConnection) -> Result<(), MmError> {
        validate_name(name)?;
        validate_connection(&connection)?;
        let _lock = self.lock()?;
        let mut document = self.read_or_default()?;
        document.connections.insert(name.to_owned(), connection);
        self.write_document(&document)
    }

    pub fn remove_connection(&self, name: &str) -> Result<bool, MmError> {
        validate_name(name)?;
        let _lock = self.lock()?;
        let mut document = self.read_or_default()?;
        let removed = document.connections.remove(name).is_some();
        if removed {
            self.write_document(&document)?;
        }
        Ok(removed)
    }

    pub fn save_recovery(
        &self,
        installation: &str,
        value: VaultRecoveryMaterial,
    ) -> Result<(), MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        let mut document = self.read_or_default()?;
        document.recovery.insert(installation.to_owned(), value);
        self.write_document(&document)
    }

    pub fn recovery(&self, installation: &str) -> Result<VaultRecoveryMaterial, MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        self.read_document()?.recovery.remove(installation).ok_or_else(|| MmError::Security {
            message: format!("managed Vault recovery material for '{installation}' is unavailable off-instance"),
        })
    }

    pub fn has_recovery(&self, installation: &str) -> Result<bool, MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        Ok(self.read_or_default()?.recovery.contains_key(installation))
    }

    pub fn remove_recovery(&self, installation: &str) -> Result<bool, MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        let mut document = self.read_or_default()?;
        let removed = document.recovery.remove(installation).is_some();
        if removed {
            self.write_document(&document)?;
        }
        Ok(removed)
    }

    pub fn encrypt_integrity_checked(&self, bytes: &[u8]) -> Result<Vec<u8>, MmError> {
        if bytes.len() > MAX_BLOB_BYTES {
            return Err(MmError::Security {
                message: "Vault backup exceeds the 128 MiB limit".to_owned(),
            });
        }
        let identity = self.load_or_create_identity()?;
        let mut plaintext = Zeroizing::new(Vec::with_capacity(32 + bytes.len()));
        plaintext.extend_from_slice(blake3::hash(bytes).as_bytes());
        plaintext.extend_from_slice(bytes);
        age::encrypt(&identity.to_public(), &plaintext).map_err(|error| MmError::Secret {
            message: format!("could not encrypt Vault backup: {error}"),
        })
    }

    pub fn decrypt_integrity_checked(&self, bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, MmError> {
        if bytes.len() > MAX_BLOB_BYTES {
            return Err(MmError::Security {
                message: "encrypted Vault backup exceeds the 128 MiB limit".to_owned(),
            });
        }
        let plaintext =
            Zeroizing::new(age::decrypt(&self.read_identity()?, bytes).map_err(|_| {
                MmError::Security {
                    message: "Vault backup authentication or decryption failed".to_owned(),
                }
            })?);
        let (digest, snapshot) =
            plaintext
                .split_at_checked(32)
                .ok_or_else(|| MmError::Security {
                    message: "Vault backup envelope is truncated".to_owned(),
                })?;
        if blake3::hash(snapshot).as_bytes() != digest {
            return Err(MmError::Security {
                message: "Vault backup integrity check failed".to_owned(),
            });
        }
        Ok(Zeroizing::new(snapshot.to_vec()))
    }

    fn read_or_default(&self) -> Result<Document, MmError> {
        if self.store_path.exists() {
            self.read_document()
        } else {
            Ok(Document::default())
        }
    }

    fn read_document(&self) -> Result<Document, MmError> {
        ensure_private_file(&self.store_path)?;
        let encrypted =
            read_regular_file(&self.store_path, MAX_REGISTRY_BYTES)?.ok_or_else(|| {
                MmError::Secret {
                    message: "encrypted Vault connection registry disappeared".to_owned(),
                }
            })?;
        let plaintext = Zeroizing::new(age::decrypt(&self.read_identity()?, &encrypted).map_err(
            |_| MmError::Secret {
                message: "could not decrypt Vault connection registry".to_owned(),
            },
        )?);
        let document: Document =
            serde_json::from_slice(&plaintext).map_err(|error| MmError::Secret {
                message: format!("Vault connection registry is invalid: {error}"),
            })?;
        if document.version != DOCUMENT_VERSION {
            return Err(MmError::Secret {
                message: format!(
                    "Vault connection registry version {} is unsupported; expected {DOCUMENT_VERSION}",
                    document.version
                ),
            });
        }
        Ok(document)
    }

    fn write_document(&self, document: &Document) -> Result<(), MmError> {
        let identity = self.load_or_create_identity()?;
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(document).map_err(|error| MmError::State {
                    message: format!("could not serialize Vault connection registry: {error}"),
                })?,
            );
        let encrypted =
            age::encrypt(&identity.to_public(), &plaintext).map_err(|error| MmError::Secret {
                message: format!("could not encrypt Vault connection registry: {error}"),
            })?;
        if encrypted.len() > MAX_REGISTRY_BYTES {
            return Err(MmError::Security {
                message: "encrypted Vault connection registry exceeds its limit".to_owned(),
            });
        }
        write_private(&self.store_path, &encrypted)
    }

    fn load_or_create_identity(&self) -> Result<age::x25519::Identity, MmError> {
        if self.identity_path.exists() {
            return self.read_identity();
        }
        ensure_private_permissions_supported()?;
        let parent = parent(&self.identity_path)?;
        prepare_directory(parent)?;
        ensure_safe_identity_parent(parent)?;
        ensure_no_symlink_components(&self.identity_path)?;
        let identity = age::x25519::Identity::generate();
        let encoded = identity.to_string();
        match private_options()
            .write(true)
            .create_new(true)
            .open(&self.identity_path)
        {
            Ok(mut file) => {
                file.write_all(encoded.expose_secret().as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|source| MmError::Io {
                        context: "writing Vault registry identity".to_owned(),
                        source,
                    })?;
                ensure_private_file(&self.identity_path)?;
                Ok(identity)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => self.read_identity(),
            Err(source) => Err(MmError::Io {
                context: "creating Vault registry identity".to_owned(),
                source,
            }),
        }
    }

    fn read_identity(&self) -> Result<age::x25519::Identity, MmError> {
        ensure_private_file(&self.identity_path)?;
        let bytes = Zeroizing::new(
            read_regular_file(&self.identity_path, 16 * 1024)?.ok_or_else(|| MmError::Secret {
                message: "Vault registry identity disappeared".to_owned(),
            })?,
        );
        std::str::from_utf8(&bytes)
            .map_err(|_| MmError::Secret {
                message: "Vault registry identity is not UTF-8".to_owned(),
            })?
            .trim()
            .parse()
            .map_err(|message: &'static str| MmError::Secret {
                message: format!("Vault registry identity is invalid: {message}"),
            })
    }

    fn lock(&self) -> Result<File, MmError> {
        prepare_directory(parent(&self.lock_path)?)?;
        ensure_no_symlink_components(&self.lock_path)?;
        let file = private_options()
            .read(true)
            .write(true)
            .create(true)
            .open(&self.lock_path)
            .map_err(|source| MmError::Io {
                context: "opening Vault registry lock".to_owned(),
                source,
            })?;
        crate::adapters::file_lock::exclusive(&file, &self.lock_path)?;
        Ok(file)
    }
}

pub fn ca_sha256(ca_pem: &str) -> String {
    use sha2::Digest as _;
    format!(
        "sha256:{}",
        sha2::Sha256::digest(ca_pem.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn validate_name(value: &str) -> Result<(), MmError> {
    validate_segment(value, "Vault connection name")
}

pub fn validate_segment(value: &str, kind: &str) -> Result<(), MmError> {
    let valid = !value.is_empty()
        && value.len() <= 63
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("{kind} must be one safe 1-63 byte ASCII path segment"),
        })
    }
}

fn validate_connection(connection: &VaultConnection) -> Result<(), MmError> {
    let address = reqwest::Url::parse(&connection.address).map_err(|_| MmError::Security {
        message: "Vault address is invalid".to_owned(),
    })?;
    if address.scheme() != "https" || !address.username().is_empty() || address.password().is_some()
    {
        return Err(MmError::Security {
            message: "Vault address must be HTTPS without embedded credentials".to_owned(),
        });
    }
    validate_segment(&connection.mount, "Vault KV mount")?;
    validate_segment(&connection.auth_mount, "Vault AppRole mount")?;
    if connection.ca_pem.is_empty() || ca_sha256(&connection.ca_pem) != connection.ca_sha256 {
        return Err(MmError::Security {
            message: "Vault CA trust-bundle pin mismatch".to_owned(),
        });
    }
    if connection.credentials.is_empty()
        || connection
            .credentials
            .keys()
            .ne(connection.allowed_locations.keys())
    {
        return Err(MmError::Security {
            message: "Vault environment credentials and location scopes must match".to_owned(),
        });
    }
    for (environment, credential) in &connection.credentials {
        validate_segment(environment, "Vault environment")?;
        if credential.role_id.is_empty()
            || credential.secret_id.is_empty()
            || connection.allowed_locations[environment].is_empty()
        {
            return Err(MmError::Security {
                message: "Vault AppRole credential or exact location scope is incomplete"
                    .to_owned(),
            });
        }
    }
    Ok(())
}

fn parent(path: &Path) -> Result<&Path, MmError> {
    path.parent().ok_or_else(|| MmError::State {
        message: "private Vault path has no parent".to_owned(),
    })
}

fn prepare_directory(path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|source| MmError::Io {
                context: format!("creating private Vault directory {}", path.display()),
                source,
            })?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!("securing private Vault directory {}", path.display()),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating private Vault directory {}", path.display()),
        source,
    })?;
    ensure_user_controlled_directory(path)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    prepare_directory(parent(path)?)?;
    ensure_no_symlink_components(path)?;
    let mut options = AtomicWriteFile::options();
    #[cfg(unix)]
    {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .preserve_mode(false)
            .preserve_owner(false);
    }
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: "opening private Vault registry".to_owned(),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: "writing private Vault registry".to_owned(),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: "publishing private Vault registry".to_owned(),
        source,
    })?;
    ensure_private_file(path)
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
}

#[cfg(test)]
#[path = "connection_store_tests.rs"]
mod tests;
