use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use age::secrecy::ExposeSecret;
use async_trait::async_trait;
use atomic_write_file::AtomicWriteFile;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_owned_by_current_user, ensure_private_file,
    ensure_private_permissions_supported, ensure_safe_identity_parent, read_regular_file,
};
use crate::app::ports::{
    SecretBatchReplacer, SecretCandidateLifecycle, SecretCreator, SecretMaterialResolver,
    SecretMetadataInspector, SecretRemover, SecretReplacer,
};
use crate::domain::environment::is_safe_environment_value;
use crate::domain::error::MmError;
use crate::domain::secret::{
    InitializedAgeStore, SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference,
    SecretReplacement, SecretResolution, SecretScope, SecretStatus, SecretWriteResult,
};
use crate::domain::secret_contract::{
    AuthenticationKind, RevisionSemantics, RotationCapability, SecretMaterialKind,
    StorageCapabilityDescriptor, StorageEnvironmentScoping, StorageOperation, StorageProviderId,
};

const MAX_SECRET_BYTES: usize = 1024 * 1024;
const MAX_STORE_BYTES: usize = 16 * 1024 * 1024;
const MAX_RETIRED_REVISIONS: usize = 32;
const CURRENT_SECRET_DOCUMENT_VERSION: u32 = 1;

#[derive(Debug)]
pub struct AgeSecretStore {
    identity_path: PathBuf,
    store_path: PathBuf,
    lock_path: PathBuf,
}

impl AgeSecretStore {
    #[must_use]
    pub fn new(identity_path: PathBuf, store_path: PathBuf) -> Self {
        let lock_path = store_path.with_extension("lock");
        Self {
            identity_path,
            store_path,
            lock_path,
        }
    }

    pub fn initialize(&self) -> Result<InitializedAgeStore, MmError> {
        ensure_private_permissions_supported()?;
        ensure_no_symlink_components(&self.identity_path)?;
        ensure_no_symlink_components(&self.store_path)?;
        let identity_parent = parent(&self.identity_path)?;
        if !identity_parent.exists() {
            create_private_directory(identity_parent)?;
        }
        ensure_safe_identity_parent(identity_parent)?;
        create_private_directory(parent(&self.store_path)?)?;
        let _store_lock = self.exclusive_lock()?;
        if self.store_path.exists() {
            return Err(MmError::Secret {
                message: "Age secret store already exists for this project".to_owned(),
            });
        }

        let identity = if self.identity_path.exists() {
            self.read_identity()?
        } else {
            let identity = age::x25519::Identity::generate();
            let encoded = identity.to_string();
            if publish_new_identity(&self.identity_path, encoded.expose_secret().as_bytes())? {
                identity
            } else {
                self.read_identity()?
            }
        };
        let recipient = identity.to_public();
        let document = SecretDocument::default();
        self.write_document(&identity, &document)?;

        Ok(InitializedAgeStore {
            identity_path: self.identity_path.clone(),
            store_path: self.store_path.clone(),
            recipient: recipient.to_string(),
        })
    }

    fn read_identity(&self) -> Result<age::x25519::Identity, MmError> {
        ensure_private_file(&self.identity_path)?;
        let encoded = Zeroizing::new(
            read_regular_file(&self.identity_path, 16 * 1024)?.ok_or_else(|| MmError::Secret {
                message: "Age identity disappeared while it was being read".to_owned(),
            })?,
        );
        let encoded = std::str::from_utf8(&encoded).map_err(|_| MmError::Secret {
            message: "Age identity is not valid UTF-8".to_owned(),
        })?;
        encoded
            .trim()
            .parse()
            .map_err(|message: &'static str| MmError::Secret {
                message: format!("Age identity is invalid: {message}"),
            })
    }

    fn read_document(&self) -> Result<SecretDocument, MmError> {
        ensure_private_file(&self.store_path)?;
        let identity = self.read_identity()?;
        let ciphertext =
            read_regular_file(&self.store_path, MAX_STORE_BYTES)?.ok_or_else(|| {
                MmError::Secret {
                    message: "Age secret store disappeared while it was being read".to_owned(),
                }
            })?;
        let plaintext = Zeroizing::new(age::decrypt(&identity, &ciphertext).map_err(|error| {
            MmError::Secret {
                message: format!("could not decrypt Age secret store: {error}"),
            }
        })?);
        let document = serde_json::from_slice::<SecretDocument>(&plaintext).map_err(|error| {
            MmError::Secret {
                message: format!("Age secret store has invalid contents: {error}"),
            }
        })?;
        if document.version != CURRENT_SECRET_DOCUMENT_VERSION {
            return Err(MmError::Secret {
                message: format!(
                    "Age secret state version {} is unsupported; expected {CURRENT_SECRET_DOCUMENT_VERSION}",
                    document.version
                ),
            });
        }
        Ok(document)
    }

    fn write_document(
        &self,
        identity: &age::x25519::Identity,
        document: &SecretDocument,
    ) -> Result<(), MmError> {
        let current = CurrentSecretDocument {
            version: CURRENT_SECRET_DOCUMENT_VERSION,
            entries: &document.entries,
            host: &document.host,
        };
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(&current).map_err(|error| MmError::Secret {
                    message: format!("could not serialize Age secret store: {error}"),
                })?,
            );
        if plaintext.len() > MAX_STORE_BYTES {
            return Err(MmError::Secret {
                message: format!(
                    "Age secret store exceeds the {MAX_STORE_BYTES}-byte safety limit"
                ),
            });
        }
        let ciphertext =
            age::encrypt(&identity.to_public(), &plaintext).map_err(|error| MmError::Secret {
                message: format!("could not encrypt Age secret store: {error}"),
            })?;
        if ciphertext.len() > MAX_STORE_BYTES {
            return Err(MmError::Secret {
                message: format!(
                    "encrypted Age secret store exceeds the {MAX_STORE_BYTES}-byte safety limit"
                ),
            });
        }
        atomic_write_private(&self.store_path, &ciphertext)
    }

    fn save_document(&self, document: &SecretDocument) -> Result<(), MmError> {
        let identity = self.read_identity()?;
        self.write_document(&identity, document)
    }

    fn exclusive_lock(&self) -> Result<File, MmError> {
        self.exclusive_lock_path(&self.lock_path)
    }

    fn exclusive_lock_path(&self, path: &Path) -> Result<File, MmError> {
        let parent = parent(path)?;
        create_private_directory(parent)?;
        ensure_no_symlink_components(path)?;
        let file = private_open_options()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map_err(|source| MmError::Io {
                context: format!("opening Age store lock {}", path.display()),
                source,
            })?;
        crate::adapters::file_lock::exclusive(&file, path)?;
        Ok(file)
    }

    fn replace_many(
        &self,
        replacements: Vec<SecretReplacement>,
        now_unix_ms: i64,
    ) -> Result<Vec<String>, MmError> {
        if replacements.is_empty() {
            return Ok(Vec::new());
        }
        ensure_initialized(self)?;
        let mut locators = BTreeSet::new();
        for replacement in &replacements {
            ensure_age(&replacement.reference)?;
            validate_material(&replacement.material)?;
            if replacement.expected.active_revision.is_none()
                || replacement.expected.candidate_revision.is_some()
            {
                return Err(MmError::Secret {
                    message: "active replacement requires a set secret with no staged candidate"
                        .to_owned(),
                });
            }
            if !locators.insert(storage_key(&replacement.scope, &replacement.reference)) {
                return Err(MmError::Secret {
                    message: "a secret replacement batch contains a duplicate provider location"
                        .to_owned(),
                });
            }
        }

        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        for replacement in &replacements {
            ensure_expected_state(
                document
                    .entries
                    .get(&storage_key(&replacement.scope, &replacement.reference)),
                &replacement.expected,
            )?;
        }

        let mut revisions = Vec::with_capacity(replacements.len());
        for replacement in replacements {
            let next = stored_secret(replacement.material, now_unix_ms);
            revisions.push(next.revision.clone());
            let key = storage_key(&replacement.scope, &replacement.reference);
            let entry = document
                .entries
                .get_mut(&key)
                .ok_or_else(lifecycle_conflict)?;
            replace_active(entry, next, now_unix_ms)?;
        }
        self.save_document(&document)?;
        Ok(revisions)
    }

    pub(crate) fn initialized(&self) -> bool {
        self.identity_path.is_file() && self.store_path.is_file()
    }
}

#[async_trait]
impl SecretMetadataInspector for AgeSecretStore {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        ensure_age(reference)?;
        if !self.initialized() {
            return Ok(SecretMetadata {
                status: SecretStatus::ProviderUnavailable,
                revision: None,
                created_at_unix_ms: None,
                candidate_revision: None,
            });
        }
        let _lock = self.exclusive_lock()?;
        let document = self.read_document()?;
        if let Some(staged) = document.host.staged.get(&secret_slot(scope, reference)) {
            return Ok(staged.metadata());
        }
        let entry = document.entries.get(&storage_key(scope, reference));
        Ok(entry.map_or_else(unset_metadata, stored_metadata))
    }

    /// One decryption of the store for every reference.
    async fn inspect_many(
        &self,
        scope: &SecretScope,
        references: &[SecretReference],
    ) -> Result<Vec<SecretMetadata>, MmError> {
        for reference in references {
            ensure_age(reference)?;
        }
        if references.is_empty() {
            return Ok(Vec::new());
        }
        if !self.initialized() {
            return Ok(references
                .iter()
                .map(|_| SecretMetadata {
                    status: SecretStatus::ProviderUnavailable,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                })
                .collect());
        }
        let _lock = self.exclusive_lock()?;
        let document = self.read_document()?;
        Ok(references
            .iter()
            .map(|reference| {
                document
                    .entries
                    .get(&storage_key(scope, reference))
                    .map_or_else(unset_metadata, stored_metadata)
            })
            .collect())
    }
}

#[async_trait]
impl SecretMaterialResolver for AgeSecretStore {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        ensure_age(reference)?;
        if !self.initialized() {
            return Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::ProviderUnavailable,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                },
                material: None,
            });
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        if let Some(staged) = document.host.staged.remove(&secret_slot(scope, reference)) {
            return staged.resolution();
        }
        let Some(entry) = document.entries.remove(&storage_key(scope, reference)) else {
            return Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Unset,
                    revision: None,
                    created_at_unix_ms: None,
                    candidate_revision: None,
                },
                material: None,
            });
        };
        let candidate_revision = entry
            .candidate
            .as_ref()
            .map(|candidate| candidate.revision.clone());
        let revision = entry.active.revision;
        let created_at_unix_ms = entry.active.created_at_unix_ms;
        if entry.active.value.is_empty() || !is_safe_environment_value(&entry.active.value) {
            return Ok(SecretResolution {
                metadata: SecretMetadata {
                    status: SecretStatus::Invalid,
                    revision: Some(revision),
                    created_at_unix_ms: Some(created_at_unix_ms),
                    candidate_revision,
                },
                material: None,
            });
        }
        Ok(SecretResolution {
            metadata: SecretMetadata {
                status: SecretStatus::Set,
                revision: Some(revision),
                created_at_unix_ms: Some(created_at_unix_ms),
                candidate_revision,
            },
            material: Some(
                SecretMaterial::from_zeroizing(entry.active.value)
                    .map_err(|message| MmError::Secret { message })?,
            ),
        })
    }
}

#[async_trait]
impl SecretCreator for AgeSecretStore {
    async fn create(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        now_unix_ms: i64,
    ) -> Result<SecretWriteResult, MmError> {
        ensure_age(reference)?;
        ensure_initialized(self)?;
        validate_material(&material)?;
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let key = storage_key(scope, reference);
        if let Some(entry) = document.entries.get(&key) {
            return Ok(SecretWriteResult {
                revision: entry.active.revision.clone(),
                changed: false,
            });
        }
        let active = stored_secret(material, now_unix_ms);
        let revision = active.revision.clone();
        document.entries.insert(
            key,
            SecretEntry {
                active,
                candidate: None,
                previous: None,
                retired: Vec::new(),
            },
        );
        self.save_document(&document)?;
        Ok(SecretWriteResult {
            revision,
            changed: true,
        })
    }
}

#[async_trait]
impl SecretReplacer for AgeSecretStore {
    async fn replace(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        self.replace_many(
            vec![SecretReplacement {
                scope: scope.clone(),
                reference: reference.clone(),
                material,
                expected,
            }],
            now_unix_ms,
        )?
        .into_iter()
        .next()
        .ok_or_else(|| MmError::Secret {
            message: "secret replacement returned no revision".to_owned(),
        })
    }
}

#[async_trait]
impl SecretBatchReplacer for AgeSecretStore {
    async fn replace_batch(
        &self,
        replacements: Vec<SecretReplacement>,
        now_unix_ms: i64,
    ) -> Result<Vec<String>, MmError> {
        self.replace_many(replacements, now_unix_ms)
    }
}

#[async_trait]
impl SecretCandidateLifecycle for AgeSecretStore {
    async fn stage(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        ensure_age(reference)?;
        ensure_initialized(self)?;
        validate_material(&material)?;
        if expected.active_revision.is_none() || expected.candidate_revision.is_some() {
            return Err(MmError::Secret {
                message: "candidate staging requires a set secret with no staged candidate"
                    .to_owned(),
            });
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let key = storage_key(scope, reference);
        ensure_expected_state(document.entries.get(&key), &expected)?;
        let entry = document
            .entries
            .get_mut(&key)
            .ok_or_else(lifecycle_conflict)?;
        let candidate = stored_secret(material, now_unix_ms);
        let revision = candidate.revision.clone();
        entry.candidate = Some(candidate);
        self.save_document(&document)?;
        Ok(revision)
    }

    async fn promote(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
        now_unix_ms: i64,
    ) -> Result<String, MmError> {
        ensure_age(reference)?;
        ensure_initialized(self)?;
        if expected.active_revision.is_none() || expected.candidate_revision.is_none() {
            return Err(MmError::Secret {
                message: "candidate promotion requires active and candidate revisions".to_owned(),
            });
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let key = storage_key(scope, reference);
        ensure_expected_state(document.entries.get(&key), &expected)?;
        let entry = document
            .entries
            .get_mut(&key)
            .ok_or_else(lifecycle_conflict)?;
        let candidate = entry.candidate.take().ok_or_else(lifecycle_conflict)?;
        let revision = candidate.revision.clone();
        promote_candidate(entry, candidate, now_unix_ms);
        self.save_document(&document)?;
        Ok(revision)
    }
}

#[async_trait]
impl SecretRemover for AgeSecretStore {
    async fn unset(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        ensure_age(reference)?;
        ensure_initialized(self)?;
        if expected.candidate_revision.is_some() {
            return Err(MmError::Secret {
                message: "a secret with a staged candidate cannot be removed".to_owned(),
            });
        }
        let _lock = self.exclusive_lock()?;
        let mut document = self.read_document()?;
        let key = storage_key(scope, reference);
        ensure_expected_state(document.entries.get(&key), &expected)?;
        if expected.active_revision.is_none() {
            return Ok(false);
        }
        let removed = document.entries.remove(&key).is_some();
        if removed {
            self.save_document(&document)?;
        }
        Ok(removed)
    }
}

#[cfg(test)]
#[path = "store/tests.rs"]
mod store_tests;
