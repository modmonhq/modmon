use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use zeroize::{Zeroize, Zeroizing};

use crate::app::ports::{SecretMaterialResolver, SecretMetadataInspector};
use crate::domain::error::MmError;
use crate::domain::secret::{
    FileSecretLocation, SecretMaterial, SecretMetadata, SecretReference, SecretResolution,
    SecretScope, SecretStatus, file_secret_location,
};
use crate::domain::secret_contract::{
    AuthenticationKind, MAX_SECRET_MATERIAL_BYTES, RevisionSemantics, RotationCapability,
    SecretMaterialKind, StorageCapabilityDescriptor, StorageEnvironmentScoping, StorageOperation,
    StorageProviderId,
};

/// Secrets read, when an operation needs them, from a file on the machine
/// that runs mm, such as the credentials file of the tool that uses them.
#[derive(Clone)]
pub struct FileSecretStore {
    revision_key: [u8; 32],
    mm_home: PathBuf,
}

/// The `file` provider of a host's `mmd`, which never reads a file a
/// project names.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostFileSecrets;

enum FileRead {
    Missing,
    Unusable(String),
    Value(Zeroizing<String>),
}

impl FileSecretStore {
    #[must_use]
    pub const fn new(revision_key: [u8; 32], mm_home: PathBuf) -> Self {
        Self {
            revision_key,
            mm_home,
        }
    }

    fn read(&self, reference: &SecretReference) -> Result<FileRead, MmError> {
        validate(reference)?;
        let location = reference.locator();
        let path =
            match file_secret_location(location).map_err(|message| MmError::Secret { message })? {
                FileSecretLocation::Absolute(path) => path.to_path_buf(),
                FileSecretLocation::Home(path) => user_home()?.join(path),
            };
        let real = match fs::canonicalize(&path) {
            Ok(real) => real,
            Err(error) if gone(&error) => return Ok(FileRead::Missing),
            Err(error) => return Ok(cannot_read(location, &error)),
        };
        if let Some(home) = self
            .mm_homes()
            .into_iter()
            .find(|home| real.starts_with(home))
        {
            return Ok(FileRead::Unusable(format!(
                "{location} is inside mm's home {}, which holds mm's own keys and stores; keep the value in a file outside it",
                home.display()
            )));
        }
        read_regular(location, &real)
    }

    fn mm_homes(&self) -> Vec<PathBuf> {
        let mut homes = vec![self.mm_home.clone()];
        if let Ok(home) = user_home() {
            homes.push(home.join(".mm"));
        }
        let mut real = Vec::with_capacity(homes.len());
        for home in homes {
            if let Ok(home) = fs::canonicalize(home) {
                real.push(home);
            }
        }
        real
    }

    fn metadata(&self, reference: &SecretReference, read: &FileRead) -> SecretMetadata {
        let (status, revision) = match read {
            FileRead::Missing => (SecretStatus::Unset, None),
            FileRead::Unusable(_) => (SecretStatus::Invalid, None),
            FileRead::Value(value) => (
                SecretStatus::Set,
                Some(super::environment::keyed_revision(
                    &self.revision_key,
                    reference,
                    value,
                )),
            ),
        };
        SecretMetadata {
            status,
            revision,
            created_at_unix_ms: None,
            candidate_revision: None,
        }
    }
}

impl std::fmt::Debug for FileSecretStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileSecretStore")
            .field("revision_key", &"[REDACTED]")
            .field("mm_home", &self.mm_home)
            .finish()
    }
}

impl Drop for FileSecretStore {
    fn drop(&mut self) {
        self.revision_key.zeroize();
    }
}

#[async_trait]
impl SecretMetadataInspector for FileSecretStore {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        let read = self.read(reference)?;
        Ok(self.metadata(reference, &read))
    }

    async fn invalid_reason(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Option<String> {
        match self.read(reference) {
            Ok(FileRead::Unusable(reason)) => Some(reason),
            Ok(FileRead::Missing | FileRead::Value(_)) => None,
            Err(error) => Some(error.to_string()),
        }
    }
}

#[async_trait]
impl SecretMaterialResolver for FileSecretStore {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        let read = self.read(reference)?;
        let metadata = self.metadata(reference, &read);
        let material = match read {
            FileRead::Value(value) => Some(
                SecretMaterial::from_zeroizing(value)
                    .map_err(|message| MmError::Secret { message })?,
            ),
            FileRead::Missing | FileRead::Unusable(_) => None,
        };
        Ok(SecretResolution { metadata, material })
    }
}

#[async_trait]
impl SecretMetadataInspector for HostFileSecrets {
    async fn inspect(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Err(read_on_host(scope, reference))
    }
}

#[async_trait]
impl SecretMaterialResolver for HostFileSecrets {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Err(read_on_host(scope, reference))
    }
}

fn read_on_host(scope: &SecretScope, reference: &SecretReference) -> MmError {
    let environment = scope.environment();
    let location = reference.locator();
    MmError::Secret {
        message: format!(
            "a host never reads a file a project names, and this secret is read from {location} on the machine that runs mm. To give a service of {environment} the value, keep it in a secret with `provider: age`, bind that one for {environment}, and stage it from your machine: `mm -e {environment} secret set NAME --stdin < {location}`"
        ),
    }
}

pub fn file_storage_descriptor() -> Result<StorageCapabilityDescriptor, MmError> {
    Ok(StorageCapabilityDescriptor {
        id: StorageProviderId::new("file").map_err(|message| MmError::Security { message })?,
        operations: vec![
            StorageOperation::InspectMetadata,
            StorageOperation::ResolveMaterial,
        ],
        revision_semantics: RevisionSemantics::Observed,
        max_material_bytes: MAX_SECRET_MATERIAL_BYTES,
        max_location_bytes: 1024,
        environment_scoping: StorageEnvironmentScoping::ProcessGlobal,
        material_kinds: vec![SecretMaterialKind::Text],
        rotation: RotationCapability::None,
        authentication: AuthenticationKind::LocalFile,
        metadata_without_material_resolution: true,
    })
}

fn read_regular(location: &str, path: &Path) -> Result<FileRead, MmError> {
    let unusable = |problem: &str| Ok(FileRead::Unusable(format!("{location} {problem}")));
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => return unusable("is a directory, not a file"),
        Ok(metadata) if !metadata.is_file() => return unusable("is not a regular file"),
        Ok(metadata) if metadata.len() > MAX_SECRET_MATERIAL_BYTES as u64 => {
            return unusable("is larger than 1 MiB");
        }
        Ok(_) => {}
        Err(error) if gone(&error) => return Ok(FileRead::Missing),
        Err(error) => return Ok(cannot_read(location, &error)),
    }
    let file = match open_no_follow(path) {
        Ok(file) => file,
        Err(error) if gone(&error) => return Ok(FileRead::Missing),
        Err(error) => return Ok(cannot_read(location, &error)),
    };
    let metadata = file.metadata().map_err(|source| MmError::Io {
        context: format!("inspecting secret file {location}"),
        source,
    })?;
    if !metadata.is_file() {
        return unusable("is not a regular file");
    }
    let limit = MAX_SECRET_MATERIAL_BYTES as u64 + 1;
    let mut bytes = Zeroizing::new(Vec::with_capacity(
        usize::try_from(metadata.len().saturating_add(1).min(limit))
            .unwrap_or(MAX_SECRET_MATERIAL_BYTES),
    ));
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|source| MmError::Io {
            context: format!("reading secret file {location}"),
            source,
        })?;
    if bytes.len() > MAX_SECRET_MATERIAL_BYTES {
        return unusable("is larger than 1 MiB");
    }
    if bytes.is_empty() {
        return Ok(FileRead::Missing);
    }
    match String::from_utf8(std::mem::take(&mut *bytes)) {
        Ok(text) => Ok(FileRead::Value(Zeroizing::new(text))),
        Err(error) => {
            error.into_bytes().zeroize();
            unusable("is not UTF-8 text, and mm passes secrets as text")
        }
    }
}

fn cannot_read(location: &str, error: &std::io::Error) -> FileRead {
    FileRead::Unusable(format!("mm cannot read {location}: {error}"))
}

fn gone(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

fn user_home() -> Result<PathBuf, MmError> {
    directories::BaseDirs::new()
        .map(|directories| directories.home_dir().to_path_buf())
        .ok_or_else(|| MmError::State {
            message: "could not determine the current user's home directory to expand `~/`"
                .to_owned(),
        })
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> std::io::Result<fs::File> {
    use rustix::fs::{Mode, OFlags};
    rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map(fs::File::from)
    .map_err(std::io::Error::from)
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> std::io::Result<fs::File> {
    fs::File::open(path)
}

fn validate(reference: &SecretReference) -> Result<(), MmError> {
    if reference.scheme() == "file" {
        Ok(())
    } else {
        Err(MmError::Secret {
            message: "file secret store received a non-file reference".to_owned(),
        })
    }
}
