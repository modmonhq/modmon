use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use age::secrecy::ExposeSecret as _;
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_private_file, ensure_private_permissions_supported,
    ensure_safe_identity_parent, ensure_user_controlled_directory, read_regular_file,
};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

const CONNECTION_DOCUMENT_VERSION: u32 = 1;
const MAX_CONNECTION_STORE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENCRYPTED_BLOB_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenBaoMode {
    Managed,
    External,
}

impl From<crate::domain::secret_backend::ProviderMode> for OpenBaoMode {
    fn from(value: crate::domain::secret_backend::ProviderMode) -> Self {
        match value {
            crate::domain::secret_backend::ProviderMode::Managed => Self::Managed,
            crate::domain::secret_backend::ProviderMode::External => Self::External,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppRoleCredential {
    pub role_id: Zeroizing<String>,
    pub secret_id: Zeroizing<String>,
}

impl std::fmt::Debug for AppRoleCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppRoleCredential")
            .field("role_id", &"[REDACTED]")
            .field("secret_id", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenBaoConnection {
    pub mode: OpenBaoMode,
    pub address: String,
    pub namespace: Option<String>,
    pub mount: String,
    pub auth_mount: String,
    pub ca_pem: String,
    pub ca_sha256: String,
    pub project_id: ProjectId,
    pub credentials: BTreeMap<String, AppRoleCredential>,
    pub allowed_locations: BTreeMap<String, Vec<String>>,
    pub managed_installation: Option<String>,
}

impl std::fmt::Debug for OpenBaoConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenBaoConnection")
            .field("mode", &self.mode)
            .field("address", &"[PRIVATE]")
            .field("namespace", &self.namespace.as_ref().map(|_| "[PRIVATE]"))
            .field("mount", &self.mount)
            .field("auth_mount", &self.auth_mount)
            .field("ca_pem", &"[PRIVATE]")
            .field("ca_sha256", &self.ca_sha256)
            .field("project_id", &self.project_id)
            .field("environments", &self.credentials.keys().collect::<Vec<_>>())
            .field(
                "allowed_locations",
                &format_args!("{} environment(s)", self.allowed_locations.len()),
            )
            .field("managed_installation", &self.managed_installation)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagedRecoveryMaterial {
    pub root_token: Zeroizing<String>,
    pub unseal_key: Zeroizing<String>,
}

impl std::fmt::Debug for ManagedRecoveryMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ManagedRecoveryMaterial")
            .field("root_token", &"[REDACTED]")
            .field("unseal_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectionDocument {
    version: u32,
    connections: BTreeMap<String, OpenBaoConnection>,
    managed_recovery: BTreeMap<String, ManagedRecoveryMaterial>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    cluster_stores: BTreeMap<String, crate::domain::secret_store::StoreRecovery>,
}

impl Default for ConnectionDocument {
    fn default() -> Self {
        Self {
            version: CONNECTION_DOCUMENT_VERSION,
            connections: BTreeMap::new(),
            managed_recovery: BTreeMap::new(),
            cluster_stores: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenBaoConnectionStore {
    identity_path: PathBuf,
    store_path: PathBuf,
    lock_path: PathBuf,
}

impl OpenBaoConnectionStore {
    #[must_use]
    pub fn new(identity_path: PathBuf, store_path: PathBuf) -> Self {
        let lock_path = store_path.with_extension("lock");
        Self {
            identity_path,
            store_path,
            lock_path,
        }
    }

    pub fn connection(&self, name: &str) -> Result<OpenBaoConnection, MmError> {
        validate_name(name)?;
        let _lock = self.lock()?;
        self.read_document()?
            .connections
            .remove(name)
            .ok_or_else(|| MmError::Secret {
                message: format!(
                    "private secret connection '{name}' is not configured; run the provider connection setup flow"
                ),
            })
    }

    pub fn has_connection(&self, name: &str) -> Result<bool, MmError> {
        validate_name(name)?;
        let _lock = self.lock()?;
        Ok(self
            .read_document_or_default()?
            .connections
            .contains_key(name))
    }

    pub fn save_connection(
        &self,
        name: &str,
        connection: OpenBaoConnection,
    ) -> Result<(), MmError> {
        validate_name(name)?;
        validate_connection(&connection)?;
        let _lock = self.lock()?;
        let mut document = self.read_document_or_default()?;
        document.connections.insert(name.to_owned(), connection);
        self.write_document(&document)
    }

    pub fn remove_connection(&self, name: &str) -> Result<bool, MmError> {
        validate_name(name)?;
        let _lock = self.lock()?;
        let mut document = self.read_document_or_default()?;
        let removed = document.connections.remove(name).is_some();
        if removed {
            self.write_document(&document)?;
        }
        Ok(removed)
    }

    pub fn save_recovery(
        &self,
        installation: &str,
        recovery: ManagedRecoveryMaterial,
    ) -> Result<(), MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        let mut document = self.read_document_or_default()?;
        document
            .managed_recovery
            .insert(installation.to_owned(), recovery);
        self.write_document(&document)
    }

    pub fn recovery(&self, installation: &str) -> Result<ManagedRecoveryMaterial, MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        self.read_document()?
            .managed_recovery
            .remove(installation)
            .ok_or_else(|| MmError::Security {
                message: format!(
                    "managed provider recovery material for '{installation}' is unavailable outside the sealed instance"
                ),
            })
    }

    pub fn has_recovery(&self, installation: &str) -> Result<bool, MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        Ok(self
            .read_document_or_default()?
            .managed_recovery
            .contains_key(installation))
    }

    pub fn remove_recovery(&self, installation: &str) -> Result<bool, MmError> {
        validate_name(installation)?;
        let _lock = self.lock()?;
        let mut document = self.read_document_or_default()?;
        let removed = document.managed_recovery.remove(installation).is_some();
        if removed {
            self.write_document(&document)?;
        }
        Ok(removed)
    }

    /// Keeps what this node unseals a managed cluster store's member with,
    /// and the connection its mmd reads the environment's values through.
    pub fn save_cluster_store(
        &self,
        id: &str,
        recovery: crate::domain::secret_store::StoreRecovery,
        (name, connection): (&str, OpenBaoConnection),
    ) -> Result<(), MmError> {
        validate_name(name)?;
        validate_connection(&connection)?;
        let _lock = self.lock()?;
        let mut document = self.read_document_or_default()?;
        document.cluster_stores.insert(id.to_owned(), recovery);
        document.connections.insert(name.to_owned(), connection);
        self.write_document(&document)
    }

    pub fn cluster_store(
        &self,
        id: &str,
    ) -> Result<Option<crate::domain::secret_store::StoreRecovery>, MmError> {
        let _lock = self.lock()?;
        Ok(self.read_document_or_default()?.cluster_stores.remove(id))
    }

    pub fn encrypt_integrity_checked(&self, bytes: &[u8]) -> Result<Vec<u8>, MmError> {
        if bytes.len() > MAX_ENCRYPTED_BLOB_BYTES {
            return Err(MmError::Security {
                message: "provider backup exceeds the 128 MiB safety limit".to_owned(),
            });
        }
        let identity = self.load_or_create_identity()?;
        let digest = blake3::hash(bytes);
        let mut plaintext = Zeroizing::new(Vec::with_capacity(32 + bytes.len()));
        plaintext.extend_from_slice(digest.as_bytes());
        plaintext.extend_from_slice(bytes);
        age::encrypt(&identity.to_public(), &plaintext).map_err(|error| MmError::Secret {
            message: format!("could not encrypt provider backup: {error}"),
        })
    }

    pub fn decrypt_integrity_checked(&self, bytes: &[u8]) -> Result<Zeroizing<Vec<u8>>, MmError> {
        if bytes.len() > MAX_ENCRYPTED_BLOB_BYTES {
            return Err(MmError::Security {
                message: "encrypted provider backup exceeds the 128 MiB safety limit".to_owned(),
            });
        }
        let identity = self.read_identity()?;
        let plaintext =
            Zeroizing::new(
                age::decrypt(&identity, bytes).map_err(|_| MmError::Security {
                    message: "provider backup authentication or decryption failed".to_owned(),
                })?,
            );
        let (expected, snapshot) =
            plaintext
                .split_at_checked(32)
                .ok_or_else(|| MmError::Security {
                    message: "provider backup envelope is truncated".to_owned(),
                })?;
        if blake3::hash(snapshot).as_bytes() != expected {
            return Err(MmError::Security {
                message: "provider backup integrity check failed".to_owned(),
            });
        }
        Ok(Zeroizing::new(snapshot.to_vec()))
    }

    fn read_document_or_default(&self) -> Result<ConnectionDocument, MmError> {
        if self.store_path.exists() {
            self.read_document()
        } else {
            Ok(ConnectionDocument::default())
        }
    }

    fn read_document(&self) -> Result<ConnectionDocument, MmError> {
        ensure_private_file(&self.store_path)?;
        let identity = self.read_identity()?;
        let ciphertext = read_regular_file(&self.store_path, MAX_CONNECTION_STORE_BYTES)?
            .ok_or_else(|| MmError::Secret {
                message: "encrypted provider connection registry disappeared".to_owned(),
            })?;
        let plaintext = Zeroizing::new(age::decrypt(&identity, &ciphertext).map_err(|error| {
            MmError::Secret {
                message: format!("could not decrypt provider connection registry: {error}"),
            }
        })?);
        let document =
            serde_json::from_slice::<ConnectionDocument>(&plaintext).map_err(|error| {
                MmError::Secret {
                    message: format!("provider connection registry is invalid: {error}"),
                }
            })?;
        if document.version != CONNECTION_DOCUMENT_VERSION {
            return Err(MmError::Secret {
                message: format!(
                    "provider connection registry version {} is unsupported; expected {CONNECTION_DOCUMENT_VERSION}",
                    document.version
                ),
            });
        }
        Ok(document)
    }

    fn write_document(&self, document: &ConnectionDocument) -> Result<(), MmError> {
        let identity = self.load_or_create_identity()?;
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(document).map_err(|error| MmError::Secret {
                    message: format!("could not serialize provider connection registry: {error}"),
                })?,
            );
        let ciphertext =
            age::encrypt(&identity.to_public(), &plaintext).map_err(|error| MmError::Secret {
                message: format!("could not encrypt provider connection registry: {error}"),
            })?;
        if ciphertext.len() > MAX_CONNECTION_STORE_BYTES {
            return Err(MmError::Secret {
                message: "encrypted provider connection registry exceeds its safety limit"
                    .to_owned(),
            });
        }
        write_private(&self.store_path, &ciphertext)
    }

    fn load_or_create_identity(&self) -> Result<age::x25519::Identity, MmError> {
        if self.identity_path.exists() {
            return self.read_identity();
        }
        ensure_private_permissions_supported()?;
        let parent = parent(&self.identity_path)?;
        prepare_private_directory(parent)?;
        ensure_safe_identity_parent(parent)?;
        ensure_no_symlink_components(&self.identity_path)?;
        let identity = age::x25519::Identity::generate();
        let encoded = identity.to_string();
        let mut options = private_open_options();
        match options
            .write(true)
            .create_new(true)
            .open(&self.identity_path)
        {
            Ok(mut file) => {
                file.write_all(encoded.expose_secret().as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(|source| MmError::Io {
                        context: "writing provider connection encryption identity".to_owned(),
                        source,
                    })?;
                ensure_private_file(&self.identity_path)?;
                Ok(identity)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => self.read_identity(),
            Err(source) => Err(MmError::Io {
                context: "creating provider connection encryption identity".to_owned(),
                source,
            }),
        }
    }

    fn read_identity(&self) -> Result<age::x25519::Identity, MmError> {
        ensure_private_file(&self.identity_path)?;
        let bytes = Zeroizing::new(
            read_regular_file(&self.identity_path, 16 * 1024)?.ok_or_else(|| MmError::Secret {
                message: "provider connection encryption identity disappeared".to_owned(),
            })?,
        );
        std::str::from_utf8(&bytes)
            .map_err(|_| MmError::Secret {
                message: "provider connection encryption identity is not UTF-8".to_owned(),
            })?
            .trim()
            .parse()
            .map_err(|message: &'static str| MmError::Secret {
                message: format!("provider connection encryption identity is invalid: {message}"),
            })
    }

    fn lock(&self) -> Result<File, MmError> {
        prepare_private_directory(parent(&self.lock_path)?)?;
        ensure_no_symlink_components(&self.lock_path)?;
        let file = private_open_options()
            .read(true)
            .write(true)
            .create(true)
            .open(&self.lock_path)
            .map_err(|source| MmError::Io {
                context: "opening provider connection registry lock".to_owned(),
                source,
            })?;
        crate::adapters::file_lock::exclusive(&file, &self.lock_path)?;
        Ok(file)
    }
}

pub fn ca_sha256(ca_pem: &str) -> String {
    let digest = <sha2::Sha256 as sha2::Digest>::digest(ca_pem.as_bytes());
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn validate_connection(connection: &OpenBaoConnection) -> Result<(), MmError> {
    let address = reqwest::Url::parse(&connection.address).map_err(|_| MmError::Security {
        message: "provider address is not a valid URL".to_owned(),
    })?;
    if address.scheme() != "https" || address.username() != "" || address.password().is_some() {
        return Err(MmError::Security {
            message: "provider address must be HTTPS and contain no embedded credentials"
                .to_owned(),
        });
    }
    validate_path_segment(&connection.mount, "secret mount")?;
    validate_path_segment(&connection.auth_mount, "AppRole auth mount")?;
    if connection.ca_pem.is_empty() || ca_sha256(&connection.ca_pem) != connection.ca_sha256 {
        return Err(MmError::Security {
            message: "provider CA trust-bundle pin does not match its configured certificate"
                .to_owned(),
        });
    }
    if connection.credentials.is_empty() {
        return Err(MmError::Security {
            message: "provider connection has no environment-scoped AppRole credential".to_owned(),
        });
    }
    for (environment, credential) in &connection.credentials {
        validate_path_segment(environment, "environment")?;
        if credential.role_id.is_empty() || credential.secret_id.is_empty() {
            return Err(MmError::Security {
                message: "provider AppRole credential is incomplete".to_owned(),
            });
        }
    }
    if connection
        .allowed_locations
        .keys()
        .ne(connection.credentials.keys())
        || connection
            .allowed_locations
            .values()
            .any(|locations| locations.is_empty())
    {
        return Err(MmError::Security {
            message: "provider connection location scopes do not match its environment credentials"
                .to_owned(),
        });
    }
    Ok(())
}

pub fn validate_name(name: &str) -> Result<(), MmError> {
    validate_path_segment(name, "connection name")
}

pub fn validate_path_segment(value: &str, kind: &str) -> Result<(), MmError> {
    let valid = !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !matches!(value, "." | "..");
    if valid {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("{kind} must be one safe 1-63 byte ASCII path segment"),
        })
    }
}

fn parent(path: &Path) -> Result<&Path, MmError> {
    path.parent().ok_or_else(|| MmError::State {
        message: "private provider path has no parent".to_owned(),
    })
}

fn prepare_private_directory(path: &Path) -> Result<(), MmError> {
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
                context: format!("creating private provider directory {}", path.display()),
                source,
            })?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!("securing provider directory {}", path.display()),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating private provider directory {}", path.display()),
        source,
    })?;
    ensure_user_controlled_directory(path)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    prepare_private_directory(parent(path)?)?;
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
        context: "opening encrypted provider connection registry".to_owned(),
        source,
    })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: "writing encrypted provider connection registry".to_owned(),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: "publishing encrypted provider connection registry".to_owned(),
        source,
    })?;
    ensure_private_file(path)
}

fn private_open_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
}

pub fn random_installation_name() -> String {
    format!("bao-{}", &Uuid::new_v4().simple().to_string()[..20])
}

#[cfg(test)]
#[path = "connection_store_tests.rs"]
mod tests;
