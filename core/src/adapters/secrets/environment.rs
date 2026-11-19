use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use async_trait::async_trait;

use crate::app::ports::{SecretMaterialResolver, SecretMetadataInspector};
use crate::domain::environment::is_safe_environment_value;
use crate::domain::error::MmError;
use crate::domain::runtime::reserved_runtime_environment_names;
use crate::domain::secret::{
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretScope, SecretStatus,
};
use crate::domain::secret_contract::{
    AuthenticationKind, RevisionSemantics, RotationCapability, SecretMaterialKind,
    StorageCapabilityDescriptor, StorageEnvironmentScoping, StorageOperation, StorageProviderId,
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone)]
pub struct EnvironmentSecretStore {
    revision_key: [u8; 32],
}

impl EnvironmentSecretStore {
    #[must_use]
    pub const fn new(revision_key: [u8; 32]) -> Self {
        Self { revision_key }
    }

    fn revision(&self, reference: &SecretReference, value: &str) -> String {
        keyed_revision(&self.revision_key, reference, value)
    }
}

pub(super) fn keyed_revision(key: &[u8; 32], reference: &SecretReference, value: &str) -> String {
    let mut input = Zeroizing::new(Vec::with_capacity(
        reference.locator().len() + value.len() + 1,
    ));
    input.extend_from_slice(reference.locator().as_bytes());
    input.push(0);
    input.extend_from_slice(value.as_bytes());
    format!("keyed-blake3:{}", blake3::keyed_hash(key, &input).to_hex())
}

impl std::fmt::Debug for EnvironmentSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnvironmentSecretStore")
            .field("revision_key", &"[REDACTED]")
            .finish()
    }
}

impl Drop for EnvironmentSecretStore {
    fn drop(&mut self) {
        self.revision_key.zeroize();
    }
}

#[async_trait]
impl SecretMetadataInspector for EnvironmentSecretStore {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        validate(reference)?;
        let (status, revision) = match std::env::var_os(reference.locator()) {
            None => (SecretStatus::Unset, None),
            Some(value) if value.is_empty() => (SecretStatus::Unset, None),
            Some(value) => match value.into_string() {
                Ok(value) => {
                    let value = Zeroizing::new(value);
                    if is_safe_environment_value(&value) {
                        (SecretStatus::Set, Some(self.revision(reference, &value)))
                    } else {
                        (SecretStatus::Invalid, None)
                    }
                }
                Err(_) => (SecretStatus::Invalid, None),
            },
        };
        Ok(SecretMetadata {
            status,
            revision,
            created_at_unix_ms: None,
            candidate_revision: None,
        })
    }

    async fn invalid_reason(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Option<String> {
        if validate(reference).is_err() {
            return None;
        }
        let name = reference.locator();
        let Ok(value) = std::env::var_os(name)?.into_string() else {
            return Some(format!("environment variable {name} is not UTF-8 text"));
        };
        let value = Zeroizing::new(value);
        if value.contains(['\n', '\r']) {
            Some(format!(
                "environment variable {name} contains a newline; use the file provider for multi-line values"
            ))
        } else if is_safe_environment_value(&value) {
            None
        } else {
            Some(format!(
                "environment variable {name} contains a control character"
            ))
        }
    }
}

#[async_trait]
impl SecretMaterialResolver for EnvironmentSecretStore {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        validate(reference)?;
        match std::env::var(reference.locator()) {
            Ok(value) if value.is_empty() => Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Unset,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                },
                material: None,
            }),
            Ok(value) if is_safe_environment_value(&value) => Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Set,
                    revision: Some(self.revision(reference, &value)),
                    created_at_unix_ms: None,
                    candidate_revision: None,
                },
                material: Some(
                    SecretMaterial::new(value).map_err(|message| MmError::Secret { message })?,
                ),
            }),
            Ok(_) | Err(std::env::VarError::NotUnicode(_)) => Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Invalid,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                },
                material: None,
            }),
            Err(std::env::VarError::NotPresent) => Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Unset,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                },
                material: None,
            }),
        }
    }
}

pub fn environment_storage_descriptor() -> Result<StorageCapabilityDescriptor, MmError> {
    Ok(StorageCapabilityDescriptor {
        id: StorageProviderId::new("env").map_err(|message| MmError::Security { message })?,
        operations: vec![
            StorageOperation::InspectMetadata,
            StorageOperation::ResolveMaterial,
        ],
        revision_semantics: RevisionSemantics::Observed,
        max_material_bytes: crate::domain::secret_contract::MAX_SECRET_MATERIAL_BYTES,
        max_location_bytes: 255,
        environment_scoping: StorageEnvironmentScoping::ProcessGlobal,
        material_kinds: vec![SecretMaterialKind::Text],
        rotation: RotationCapability::None,
        authentication: AuthenticationKind::ProcessEnvironment,
        metadata_without_material_resolution: true,
    })
}

pub fn load_or_create_environment_revision_key(path: &Path) -> Result<[u8; 32], MmError> {
    crate::adapters::private_fs::ensure_no_symlink_components(path)?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            create_private_key_directory(parent)?;
        }
        crate::adapters::private_fs::ensure_owned_by_current_user(parent)?;
    }
    let mut generated = [0_u8; 32];
    getrandom::fill(&mut generated).map_err(|error| MmError::Security {
        message: format!("could not generate environment revision key: {error}"),
    })?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    configure_private_key_options(&mut options);
    match options.open(path) {
        Ok(mut file) => {
            let result = file.write_all(&generated).and_then(|()| file.sync_all());
            generated.zeroize();
            result.map_err(|source| MmError::Io {
                context: "writing private environment revision key".to_owned(),
                source,
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => generated.zeroize(),
        Err(source) => {
            generated.zeroize();
            return Err(MmError::Io {
                context: "creating private environment revision key".to_owned(),
                source,
            });
        }
    }
    crate::adapters::private_fs::ensure_private_file(path)?;
    let mut file = fs::File::open(path).map_err(|source| MmError::Io {
        context: "opening private environment revision key".to_owned(),
        source,
    })?;
    let mut key = [0_u8; 32];
    file.read_exact(&mut key).map_err(|source| MmError::Io {
        context: "reading private environment revision key".to_owned(),
        source,
    })?;
    let mut extra = [0_u8; 1];
    if file.read(&mut extra).map_err(|source| MmError::Io {
        context: "validating private environment revision key".to_owned(),
        source,
    })? != 0
    {
        key.zeroize();
        return Err(MmError::Security {
            message: "private environment revision key has an invalid length".to_owned(),
        });
    }
    Ok(key)
}

#[cfg(unix)]
fn configure_private_key_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_private_key_options(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn create_private_key_directory(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    crate::adapters::private_fs::ensure_no_symlink_components(path)?;
    let mut builder = fs::DirBuilder::new();
    builder
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|source| MmError::Io {
            context: "creating private environment revision key directory".to_owned(),
            source,
        })?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| MmError::Io {
        context: "setting private environment revision key directory permissions".to_owned(),
        source,
    })
}

#[cfg(not(unix))]
fn create_private_key_directory(path: &Path) -> Result<(), MmError> {
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: "creating private environment revision key directory".to_owned(),
        source,
    })
}

fn validate(reference: &SecretReference) -> Result<(), MmError> {
    if reference.scheme() != "env" {
        return Err(MmError::Secret {
            message: "environment secret store received a non-env reference".to_owned(),
        });
    }
    let locator = reference.locator();
    let valid = locator
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
        && locator
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if !valid {
        return Err(MmError::Secret {
            message: "env:// locators must be environment variable names".to_owned(),
        });
    }
    if reserved_runtime_environment_names().any(|name| name == locator) {
        return Err(MmError::Secret {
            message: "env storage locations may not use runtime-reserved environment variables"
                .to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "environment/tests.rs"]
mod tests;
