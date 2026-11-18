pub fn age_storage_descriptor() -> Result<StorageCapabilityDescriptor, MmError> {
    Ok(StorageCapabilityDescriptor {
        id: StorageProviderId::new("age").map_err(|message| MmError::Security { message })?,
        operations: vec![
            StorageOperation::InspectMetadata,
            StorageOperation::ResolveMaterial,
            StorageOperation::Create,
            StorageOperation::Replace,
            StorageOperation::ReplaceBatch,
            StorageOperation::StageCandidate,
            StorageOperation::PromoteCandidate,
            StorageOperation::Delete,
        ],
        revision_semantics: RevisionSemantics::CompareAndSwap,
        max_material_bytes: MAX_SECRET_BYTES,
        max_location_bytes: 1024,
        environment_scoping: StorageEnvironmentScoping::Isolated,
        material_kinds: vec![SecretMaterialKind::Text],
        rotation: RotationCapability::Staged,
        authentication: AuthenticationKind::LocalIdentity,
        // The current encrypted document must be decrypted as a unit even for metadata reads.
        metadata_without_material_resolution: false,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretDocument {
    version: u32,
    entries: BTreeMap<String, SecretEntry>,
    #[serde(default)]
    host: HostStaging,
}

impl Default for SecretDocument {
    fn default() -> Self {
        Self {
            version: CURRENT_SECRET_DOCUMENT_VERSION,
            entries: BTreeMap::new(),
            host: HostStaging::default(),
        }
    }
}

#[derive(Serialize)]
struct CurrentSecretDocument<'a> {
    version: u32,
    entries: &'a BTreeMap<String, SecretEntry>,
    #[serde(skip_serializing_if = "no_host_staging")]
    host: &'a HostStaging,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretEntry {
    active: StoredSecret,
    candidate: Option<StoredSecret>,
    previous: Option<StoredSecret>,
    retired: Vec<RetiredSecret>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSecret {
    value: Zeroizing<String>,
    revision: String,
    created_at_unix_ms: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetiredSecret {
    revision: String,
    retired_at_unix_ms: i64,
}

impl std::fmt::Debug for SecretDocument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretDocument")
            .field("entry_count", &self.entries.len())
            .finish()
    }
}

impl std::fmt::Debug for SecretEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretEntry")
            .field("active_revision", &self.active.revision)
            .field(
                "candidate_revision",
                &self.candidate.as_ref().map(|candidate| &candidate.revision),
            )
            .field(
                "previous_revision",
                &self.previous.as_ref().map(|previous| &previous.revision),
            )
            .field("retired_count", &self.retired.len())
            .finish()
    }
}

impl std::fmt::Debug for StoredSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoredSecret")
            .field("value", &"[REDACTED]")
            .field("revision", &self.revision)
            .field("created_at_unix_ms", &self.created_at_unix_ms)
            .finish()
    }
}

fn ensure_initialized(store: &AgeSecretStore) -> Result<(), MmError> {
    if store.initialized() {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: "Age secret provider is not initialized; run `mm secret init age`".to_owned(),
        })
    }
}

fn validate_material(material: &SecretMaterial) -> Result<(), MmError> {
    let valid = material.expose(|value| {
        !value.is_empty() && value.len() <= MAX_SECRET_BYTES && is_safe_environment_value(value)
    });
    if valid {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: format!(
                "secret values must be non-empty, contain no control characters, and not exceed {MAX_SECRET_BYTES} bytes"
            ),
        })
    }
}

fn stored_secret(material: SecretMaterial, created_at_unix_ms: i64) -> StoredSecret {
    StoredSecret {
        value: material.into_zeroizing(),
        revision: Uuid::new_v4().to_string(),
        created_at_unix_ms,
    }
}

fn stored_metadata(entry: &SecretEntry) -> SecretMetadata {
    SecretMetadata {
        status: if entry.active.value.is_empty() || !is_safe_environment_value(&entry.active.value)
        {
            SecretStatus::Invalid
        } else {
            SecretStatus::Set
        },
        revision: Some(entry.active.revision.clone()),
        created_at_unix_ms: Some(entry.active.created_at_unix_ms),
        candidate_revision: entry
            .candidate
            .as_ref()
            .map(|candidate| candidate.revision.clone()),
    }
}

fn unset_metadata() -> SecretMetadata {
    SecretMetadata {
        status: SecretStatus::Unset,
        revision: None,
        created_at_unix_ms: None,
        candidate_revision: None,
    }
}

fn ensure_expected_state(
    entry: Option<&SecretEntry>,
    expected: &SecretExpectedState,
) -> Result<(), MmError> {
    let actual = SecretExpectedState {
        active_revision: entry.map(|entry| entry.active.revision.clone()),
        candidate_revision: entry
            .and_then(|entry| entry.candidate.as_ref())
            .map(|candidate| candidate.revision.clone()),
    };
    if &actual == expected {
        Ok(())
    } else {
        Err(lifecycle_conflict())
    }
}

fn lifecycle_conflict() -> MmError {
    MmError::Secret {
        message: "secret lifecycle changed since it was inspected; retry from current metadata"
            .to_owned(),
    }
}

fn replace_active(
    entry: &mut SecretEntry,
    next: StoredSecret,
    now_unix_ms: i64,
) -> Result<(), MmError> {
    if entry.candidate.is_some() {
        return Err(MmError::Secret {
            message: "a secret with a staged candidate cannot be replaced".to_owned(),
        });
    }
    retire_previous(entry, now_unix_ms);
    let previous = std::mem::replace(&mut entry.active, next);
    entry.previous = Some(previous);
    Ok(())
}

fn promote_candidate(entry: &mut SecretEntry, candidate: StoredSecret, now_unix_ms: i64) {
    retire_previous(entry, now_unix_ms);
    let previous = std::mem::replace(&mut entry.active, candidate);
    entry.previous = Some(previous);
}

fn retire_previous(entry: &mut SecretEntry, now_unix_ms: i64) {
    if let Some(previous) = entry.previous.take() {
        retire(entry, previous.revision, now_unix_ms);
    }
}

fn retire(entry: &mut SecretEntry, revision: String, now_unix_ms: i64) {
    entry.retired.push(RetiredSecret {
        revision,
        retired_at_unix_ms: now_unix_ms,
    });
    if entry.retired.len() > MAX_RETIRED_REVISIONS {
        entry
            .retired
            .drain(..entry.retired.len() - MAX_RETIRED_REVISIONS);
    }
}

fn ensure_age(reference: &SecretReference) -> Result<(), MmError> {
    if reference.scheme() == "age" {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: "Age secret store received a non-age reference".to_owned(),
        })
    }
}

fn storage_key(scope: &SecretScope, reference: &SecretReference) -> String {
    let environment = scope.environment();
    debug_assert!(
        !environment.is_empty()
            && !environment.contains('/')
            && environment != "."
            && environment != ".."
    );
    format!("{environment}/{}", reference.locator())
}

fn parent(path: &Path) -> Result<&Path, MmError> {
    path.parent().ok_or_else(|| MmError::Secret {
        message: format!("path has no parent: {}", path.display()),
    })
}

fn atomic_write_private(path: &Path, contents: &[u8]) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    let mut options = AtomicWriteFile::options();
    configure_private_atomic_options(&mut options);
    let mut file = options.open(path).map_err(|source| MmError::Io {
        context: format!("opening private atomic file {}", path.display()),
        source,
    })?;
    file.write_all(contents)
        .and_then(|()| file.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing private file {}", path.display()),
            source,
        })?;
    file.commit().map_err(|source| MmError::Io {
        context: format!("publishing private file {}", path.display()),
        source,
    })?;
    set_private_file(path)
}

fn publish_new_identity(path: &Path, contents: &[u8]) -> Result<bool, MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    let parent = parent(path)?;
    let temporary = parent.join(format!(".mm-age-identity.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = private_open_options();
        let mut file = options
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| MmError::Io {
                context: format!("creating temporary Age identity {}", temporary.display()),
                source,
            })?;
        file.write_all(contents)
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("writing temporary Age identity {}", temporary.display()),
                source,
            })?;
        set_private_file(&temporary)?;
        match fs::hard_link(&temporary, path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(source) => Err(MmError::Io {
                context: format!("publishing Age identity {}", path.display()),
                source,
            }),
        }
    })();
    match fs::remove_file(&temporary) {
        Ok(()) => result,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => result,
        Err(source) => {
            let cleanup = MmError::Io {
                context: format!("removing temporary Age identity {}", temporary.display()),
                source,
            };
            match result {
                Ok(_) => Err(cleanup),
                Err(primary) => Err(MmError::State {
                    message: format!("{primary}; Age identity cleanup also failed: {cleanup}"),
                }),
            }
        }
    }
}

fn create_private_directory(path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()?;
    ensure_no_symlink_components(path)?;
    if path.exists() {
        ensure_owned_by_current_user(path)?;
    }
    create_directories(path).map_err(|source| MmError::Io {
        context: format!("creating private directory {}", path.display()),
        source,
    })?;
    set_private_directory(path)
}

#[cfg(unix)]
fn create_directories(path: &Path) -> Result<(), std::io::Error> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_directories(path: &Path) -> Result<(), std::io::Error> {
    fs::create_dir_all(path)
}

fn private_open_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    configure_private_open_options(&mut options);
    options
}

#[cfg(unix)]
fn configure_private_open_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_private_open_options(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn configure_private_atomic_options(options: &mut atomic_write_file::OpenOptions) {
    use atomic_write_file::unix::OpenOptionsExt;
    use std::os::unix::fs::OpenOptionsExt as _;
    options
        .mode(0o600)
        .preserve_mode(false)
        .preserve_owner(false);
}

#[cfg(not(unix))]
fn configure_private_atomic_options(_options: &mut atomic_write_file::OpenOptions) {}

#[cfg(unix)]
fn set_private_directory(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| MmError::Io {
        context: format!(
            "setting private directory permissions on {}",
            path.display()
        ),
        source,
    })
}

#[cfg(not(unix))]
fn set_private_directory(path: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: format!(
            "cannot enforce private directory permissions for '{}' on this platform",
            path.display()
        ),
    })
}

#[cfg(unix)]
fn set_private_file(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| MmError::Io {
        context: format!("setting private file permissions on {}", path.display()),
        source,
    })?;
    ensure_private_file(path)
}

#[cfg(all(test, unix))]
#[path = "private_files/tests.rs"]
mod tests;

#[cfg(not(unix))]
fn set_private_file(path: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: format!(
            "cannot enforce private file permissions for '{}' on this platform",
            path.display()
        ),
    })
}
