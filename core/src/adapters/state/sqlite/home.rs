use std::fs;
use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::adapters::private_fs::{
    ensure_no_symlink_components, ensure_owned_by_current_user, ensure_safe_path_ancestors,
    ensure_user_controlled_directory,
};
use crate::domain::error::MmError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MmHome {
    pub root: PathBuf,
    pub state_database: PathBuf,
    pub identities_directory: PathBuf,
    pub secrets_directory: PathBuf,
    pub run_directory: PathBuf,
    pub target_locks_directory: PathBuf,
    pub compose_directory: PathBuf,
    pub builds_directory: PathBuf,
    pub ca_directory: PathBuf,
    pub targets_file: PathBuf,
    pub ingress_directory: PathBuf,
    pub services_directory: PathBuf,
}

impl MmHome {
    /// Resolve paths without creating directories or changing permissions.
    pub fn inspect(explicit: Option<&Path>) -> Result<Self, MmError> {
        let root = match explicit {
            Some(path) => path.to_path_buf(),
            None => BaseDirs::new()
                .map(|directories| directories.home_dir().join(".mm"))
                .ok_or_else(|| MmError::State {
                    message: "could not determine the current user's home directory".to_owned(),
                })?,
        };
        ensure_safe_path_ancestors(&root)?;
        ensure_no_symlink_components(&root)?;
        if root.exists() {
            ensure_user_controlled_directory(&root)?;
        }
        Ok(Self::at_root(root))
    }

    pub fn resolve(explicit: Option<&Path>) -> Result<Self, MmError> {
        let is_explicit = explicit.is_some();
        let root = if let Some(explicit) = explicit {
            explicit.to_path_buf()
        } else {
            BaseDirs::new()
                .map(|directories| directories.home_dir().join(".mm"))
                .ok_or_else(|| MmError::State {
                    message: "could not determine the current user's home directory".to_owned(),
                })?
        };
        prepare_mm_home(&root, is_explicit)?;
        let root = dunce::canonicalize(&root).map_err(|source| MmError::Io {
            context: format!("resolving mm home {}", root.display()),
            source,
        })?;
        ensure_user_controlled_directory(&root)?;

        Ok(Self::at_root(root))
    }

    fn at_root(root: PathBuf) -> Self {
        Self {
            state_database: root.join("state.sqlite3"),
            identities_directory: root.join("identities"),
            secrets_directory: root.join("secrets"),
            run_directory: root.join("run"),
            target_locks_directory: root.join("run").join("locks"),
            compose_directory: root.join("compose"),
            builds_directory: root.join("builds"),
            ca_directory: root.join("ca"),
            targets_file: root.join("targets.yaml"),
            ingress_directory: root.join("ingress"),
            services_directory: root.join("services"),
            root,
        }
    }
}

pub(super) fn make_private_file(path: &Path) -> Result<(), MmError> {
    make_private_file_platform(path)
}

fn prepare_mm_home(path: &Path, is_explicit: bool) -> Result<(), MmError> {
    ensure_safe_path_ancestors(path)?;
    ensure_no_symlink_components(path)?;
    if !path.exists() {
        create_mm_home(path)?;
    }
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting mm home {}", path.display()),
        source,
    })?;
    if !metadata.is_dir() {
        return Err(MmError::Security {
            message: format!("mm home '{}' is not a directory", path.display()),
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        ensure_owned_by_current_user(path)?;
        if is_explicit && metadata.permissions().mode() & 0o077 != 0 && !is_claimable_mm_home(path)?
        {
            return Err(MmError::Security {
                message: format!(
                    "explicit mm home '{}' contains unrelated files and is accessible by group or other users",
                    path.display()
                ),
            });
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            MmError::Io {
                context: format!("setting private permissions on {}", path.display()),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    {
        _ = is_explicit;
        ensure_user_controlled_directory(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn is_claimable_mm_home(path: &Path) -> Result<bool, MmError> {
    let entries = fs::read_dir(path).map_err(|source| MmError::Io {
        context: format!("reading mm home {}", path.display()),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| MmError::Io {
            context: format!("reading mm home {}", path.display()),
            source,
        })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            return Ok(false);
        };
        let file_type = entry.file_type().map_err(|source| MmError::Io {
            context: format!("inspecting mm home entry {}", entry.path().display()),
            source,
        })?;
        let expected = match name.as_str() {
            "identities" | "secrets" | "run" | "compose" | "ingress" | "services" => {
                file_type.is_dir()
            }
            "targets.yaml" | "targets.yaml.lock" => file_type.is_file(),
            "state.sqlite3"
            | "state.sqlite3-wal"
            | "state.sqlite3-shm"
            | "state.sqlite3-journal" => file_type.is_file(),
            _ => false,
        };
        if !expected || file_type.is_symlink() {
            return Ok(false);
        }
        ensure_owned_by_current_user(&entry.path())?;
    }
    Ok(true)
}

#[cfg(unix)]
fn create_mm_home(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|source| MmError::Io {
            context: format!("creating mm home {}", path.display()),
            source,
        })
}

#[cfg(not(unix))]
fn create_mm_home(path: &Path) -> Result<(), MmError> {
    fs::create_dir_all(path).map_err(|source| MmError::Io {
        context: format!("creating mm home {}", path.display()),
        source,
    })
}

#[cfg(unix)]
fn make_private_file_platform(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| MmError::Io {
        context: format!("setting private permissions on {}", path.display()),
        source,
    })
}

#[cfg(test)]
#[path = "home/tests.rs"]
mod tests;

#[cfg(not(unix))]
fn make_private_file_platform(_path: &Path) -> Result<(), MmError> {
    Ok(())
}
