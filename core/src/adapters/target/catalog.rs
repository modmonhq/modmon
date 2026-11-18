use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use atomic_write_file::AtomicWriteFile;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_private_file, ensure_private_permissions_supported,
    ensure_user_controlled_directory, read_regular_file,
};
use crate::app::ports::TargetStore;
use crate::domain::error::MmError;
use crate::domain::target::{
    NamedTarget, TargetCatalog, TargetDefinition, TargetIngress, TargetTransport,
    validate_target_catalog, validate_target_definition, validate_target_name,
};

const MAX_TARGET_CATALOG_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct FilesystemTargetStore {
    path: PathBuf,
    lock_path: PathBuf,
}

impl FilesystemTargetStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let lock_path = path.with_extension("yaml.lock");
        Self { path, lock_path }
    }

    fn read_catalog(&self) -> Result<TargetCatalog, MmError> {
        self.read_catalog_snapshot().map(|(catalog, _)| catalog)
    }

    fn read_catalog_snapshot(&self) -> Result<(TargetCatalog, Option<Vec<u8>>), MmError> {
        if let Some(parent) = self.path.parent() {
            ensure_user_controlled_directory(parent)?;
        }
        ensure_no_symlink_components(&self.path)?;
        if self.path.exists() {
            ensure_private_file(&self.path)?;
        }
        let bytes = read_regular_file(&self.path, MAX_TARGET_CATALOG_BYTES)?;
        let Some(bytes) = bytes else {
            return Ok((TargetCatalog::default(), None));
        };
        let catalog = serde_yaml_ng::from_slice::<TargetCatalog>(&bytes).map_err(|error| {
            MmError::Target {
                message: format!(
                    "could not parse target catalog '{}': {error}",
                    self.path.display()
                ),
            }
        })?;
        validate_target_catalog(&catalog).map_err(|message| MmError::Target { message })?;
        Ok((catalog, Some(bytes)))
    }

    fn exclusive_lock(&self) -> Result<File, MmError> {
        ensure_private_permissions_supported()?;
        if let Some(parent) = self.lock_path.parent() {
            ensure_user_controlled_directory(parent)?;
        }
        ensure_no_symlink_components(&self.lock_path)?;
        let mut options = OpenOptions::new();
        configure_private_open_options(&mut options);
        let file = options
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .map_err(|source| MmError::Io {
                context: format!("opening target catalog lock {}", self.lock_path.display()),
                source,
            })?;
        set_private_file(&self.lock_path)?;
        crate::adapters::file_lock::exclusive(&file, &self.lock_path)?;
        Ok(file)
    }

    fn write_catalog(
        &self,
        catalog: &TargetCatalog,
        expected_source: Option<&[u8]>,
    ) -> Result<(), MmError> {
        validate_target_catalog(catalog).map_err(|message| MmError::Target { message })?;
        let bytes = serde_yaml_ng::to_string(catalog)
            .map(String::into_bytes)
            .map_err(|error| MmError::Target {
                message: format!("could not serialize target catalog: {error}"),
            })?;
        if bytes.len() > MAX_TARGET_CATALOG_BYTES {
            return Err(MmError::Target {
                message: format!(
                    "target catalog exceeds the {MAX_TARGET_CATALOG_BYTES}-byte safety limit"
                ),
            });
        }
        ensure_private_permissions_supported()?;
        ensure_no_symlink_components(&self.path)?;
        let mut options = AtomicWriteFile::options();
        configure_private_atomic_options(&mut options);
        let mut file = options.open(&self.path).map_err(|source| MmError::Io {
            context: format!("opening target catalog {}", self.path.display()),
            source,
        })?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| MmError::Io {
                context: format!("writing target catalog {}", self.path.display()),
                source,
            })?;
        ensure_no_symlink_components(&self.path)?;
        if self.path.exists() {
            ensure_private_file(&self.path)?;
        }
        let current = read_regular_file(&self.path, MAX_TARGET_CATALOG_BYTES)?;
        if current.as_deref() != expected_source {
            return Err(MmError::Target {
                message: "target catalog changed while it was being updated; retry the command"
                    .to_owned(),
            });
        }
        file.commit().map_err(|source| MmError::Io {
            context: format!("publishing target catalog {}", self.path.display()),
            source,
        })?;
        set_private_file(&self.path)
    }
}

impl TargetStore for FilesystemTargetStore {
    fn list_targets(&self) -> Result<Vec<NamedTarget>, MmError> {
        Ok(self
            .read_catalog()?
            .targets
            .into_iter()
            .map(|(name, definition)| NamedTarget::new(name, definition))
            .collect())
    }

    fn resolve_target(&self, name: &str) -> Result<NamedTarget, MmError> {
        validate_target_name(name).map_err(|message| MmError::Target { message })?;
        let definition =
            self.read_catalog()?
                .targets
                .remove(name)
                .ok_or_else(|| MmError::Setup {
                    issue: crate::domain::readiness::SetupIssue::MissingTarget {
                        name: name.to_owned(),
                        path: self.path.clone(),
                    },
                })?;
        Ok(NamedTarget::new(name.to_owned(), definition))
    }

    fn insert_target(
        &self,
        name: &str,
        definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        validate_target_name(name).map_err(|message| MmError::Target { message })?;
        validate_target_definition(&definition).map_err(|message| MmError::Target { message })?;
        let _lock = self.exclusive_lock()?;
        let (mut catalog, source) = self.read_catalog_snapshot()?;
        if catalog.targets.contains_key(name) {
            return Err(MmError::Target {
                message: format!("target '{name}' already exists"),
            });
        }
        catalog.targets.insert(name.to_owned(), definition.clone());
        self.write_catalog(&catalog, source.as_deref())?;
        Ok(NamedTarget::new(name.to_owned(), definition))
    }

    fn ensure_target(
        &self,
        name: &str,
        definition: TargetDefinition,
    ) -> Result<NamedTarget, MmError> {
        validate_target_name(name).map_err(|message| MmError::Target { message })?;
        validate_target_definition(&definition).map_err(|message| MmError::Target { message })?;
        let _lock = self.exclusive_lock()?;
        let (mut catalog, source) = self.read_catalog_snapshot()?;
        if let Some(existing) = catalog.targets.get(name) {
            return Ok(NamedTarget::new(name.to_owned(), existing.clone()));
        }
        catalog.targets.insert(name.to_owned(), definition.clone());
        self.write_catalog(&catalog, source.as_deref())?;
        Ok(NamedTarget::new(name.to_owned(), definition))
    }

    fn remove_target(
        &self,
        name: &str,
        expected_fingerprint: &str,
    ) -> Result<NamedTarget, MmError> {
        validate_target_name(name).map_err(|message| MmError::Target { message })?;
        let _lock = self.exclusive_lock()?;
        let (mut catalog, source) = self.read_catalog_snapshot()?;
        let existing = catalog
            .targets
            .get(name)
            .cloned()
            .ok_or_else(|| MmError::Target {
                message: format!("target '{name}' does not exist"),
            })?;
        let current_fingerprint = existing.canonical_fingerprint();
        if current_fingerprint != expected_fingerprint {
            return Err(MmError::Target {
                message: format!(
                    "target '{name}' changed since it was reviewed; retry with fingerprint '{current_fingerprint}'"
                ),
            });
        }
        catalog.targets.remove(name);
        self.write_catalog(&catalog, source.as_deref())?;
        Ok(NamedTarget::new(name.to_owned(), existing))
    }

    fn update_target_transport(
        &self,
        name: &str,
        expected_fingerprint: &str,
        transport: TargetTransport,
    ) -> Result<NamedTarget, MmError> {
        validate_target_name(name).map_err(|message| MmError::Target { message })?;
        let _lock = self.exclusive_lock()?;
        let (mut catalog, source) = self.read_catalog_snapshot()?;
        let existing = catalog
            .targets
            .get(name)
            .cloned()
            .ok_or_else(|| MmError::Target {
                message: format!("target '{name}' does not exist"),
            })?;
        let current_fingerprint = existing.canonical_fingerprint();
        if current_fingerprint != expected_fingerprint {
            return Err(MmError::Target {
                message: format!(
                    "target '{name}' changed since it was reviewed; retry with fingerprint '{current_fingerprint}'"
                ),
            });
        }
        let updated = TargetDefinition {
            transport,
            ..existing
        };
        validate_target_definition(&updated).map_err(|message| MmError::Target { message })?;
        catalog.targets.insert(name.to_owned(), updated.clone());
        self.write_catalog(&catalog, source.as_deref())?;
        Ok(NamedTarget::new(name.to_owned(), updated))
    }

    fn update_target_ingress(
        &self,
        name: &str,
        expected_fingerprint: &str,
        ingress: TargetIngress,
    ) -> Result<NamedTarget, MmError> {
        validate_target_name(name).map_err(|message| MmError::Target { message })?;
        let _lock = self.exclusive_lock()?;
        let (mut catalog, source) = self.read_catalog_snapshot()?;
        let existing = catalog
            .targets
            .get(name)
            .cloned()
            .ok_or_else(|| MmError::Target {
                message: format!("target '{name}' does not exist"),
            })?;
        let current_fingerprint = existing.canonical_fingerprint();
        if current_fingerprint != expected_fingerprint {
            return Err(MmError::Target {
                message: format!(
                    "target '{name}' changed since it was reviewed; retry with fingerprint '{current_fingerprint}'"
                ),
            });
        }
        let updated = TargetDefinition {
            ingress: Some(ingress),
            ..existing
        };
        validate_target_definition(&updated).map_err(|message| MmError::Target { message })?;
        catalog.targets.insert(name.to_owned(), updated.clone());
        self.write_catalog(&catalog, source.as_deref())?;
        Ok(NamedTarget::new(name.to_owned(), updated))
    }
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
fn set_private_file(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| MmError::Io {
        context: format!("setting private permissions on {}", path.display()),
        source,
    })?;
    ensure_private_file(path)
}

#[cfg(not(unix))]
fn set_private_file(path: &Path) -> Result<(), MmError> {
    Err(MmError::Security {
        message: format!(
            "cannot enforce private target catalog permissions for '{}' on this platform",
            path.display()
        ),
    })
}

#[cfg(test)]
#[path = "catalog/tests.rs"]
mod tests;
