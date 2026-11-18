//! The versions of an environment's config files on this host.
//!
//! One file per version, named after the version, in a directory next to
//! the environment's stack artifacts. A version is written once and never
//! changed, so a container that mounts it keeps what it was started with;
//! a version no deployment mounts any more is removed after the next
//! deployment has converged.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::adapters::private_fs::{ensure_no_symlink_components, read_regular_file};
use crate::domain::config_file::{
    CONFIG_FILE_DIRECTORY, MAX_CONFIG_FILE_BYTES, is_version, version,
};
use crate::domain::error::MmError;
use crate::domain::stack::CompiledConfigFile;

pub(super) fn directory(environment_root: &Path) -> PathBuf {
    environment_root.join(CONFIG_FILE_DIRECTORY)
}

/// Puts one version at its path. A version that is already there is left
/// alone when it holds the same bytes, and refused when it does not.
pub(super) fn publish(file: &CompiledConfigFile) -> Result<(), MmError> {
    let name = file.path.file_name().and_then(|name| name.to_str());
    if !is_version(&file.version)
        || name != Some(file.version.as_str())
        || version(&file.bytes, file.mode, file.uid, file.gid) != file.version
    {
        return Err(MmError::Security {
            message: "a config file's version does not match its content, owner and mode"
                .to_owned(),
        });
    }
    let Some(parent) = file.path.parent() else {
        return Err(MmError::Security {
            message: "a config file version has no directory".to_owned(),
        });
    };
    ensure_no_symlink_components(parent)?;
    fs::create_dir_all(parent).map_err(|source| MmError::Io {
        context: format!("creating config file directory {}", parent.display()),
        source,
    })?;
    if let Some(existing) = read_regular_file(&file.path, MAX_CONFIG_FILE_BYTES)? {
        if existing == file.bytes {
            return Ok(());
        }
        return Err(MmError::Security {
            message: format!(
                "config file version {} has unexpected contents; remove the file and run the deployment again",
                file.path.display()
            ),
        });
    }
    let staging = parent.join(format!(".{}.{}.tmp", file.version, uuid::Uuid::new_v4()));
    let written = write_version(&staging, file).and_then(|()| {
        fs::rename(&staging, &file.path).map_err(|source| MmError::Io {
            context: format!("publishing config file version {}", file.path.display()),
            source,
        })
    });
    if written.is_err()
        && let Err(source) = fs::remove_file(&staging)
        && source.kind() != std::io::ErrorKind::NotFound
    {
        return Err(MmError::Io {
            context: format!("removing config file staging {}", staging.display()),
            source,
        });
    }
    written
}

fn write_version(staging: &Path, file: &CompiledConfigFile) -> Result<(), MmError> {
    let mut handle = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .map_err(|source| MmError::Io {
            context: format!("creating config file version {}", staging.display()),
            source,
        })?;
    handle
        .write_all(&file.bytes)
        .and_then(|()| handle.sync_all())
        .map_err(|source| MmError::Io {
            context: format!("writing config file version {}", staging.display()),
            source,
        })?;
    set_owner_and_mode(staging, file)
}

#[cfg(unix)]
fn set_owner_and_mode(staging: &Path, file: &CompiledConfigFile) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;
    if file.uid.is_some() || file.gid.is_some() {
        std::os::unix::fs::chown(staging, file.uid, file.gid).map_err(|error| {
            MmError::Environment {
                message: format!(
                    "a config file asks for owner {}:{}, and mm cannot give a file to another user here ({error}). mmd on a remote host runs as root and sets the owner; to deploy on this machine as an ordinary user, leave `uid` and `gid` out of the service's `config-files` entry",
                    id(file.uid),
                    id(file.gid)
                ),
            }
        })?;
    }
    fs::set_permissions(staging, fs::Permissions::from_mode(file.mode)).map_err(|source| {
        MmError::Io {
            context: format!(
                "setting the mode of config file version {}",
                staging.display()
            ),
            source,
        }
    })
}

#[cfg(unix)]
fn id(id: Option<u32>) -> String {
    id.map_or_else(|| "unchanged".to_owned(), |id| id.to_string())
}

#[cfg(not(unix))]
fn set_owner_and_mode(_staging: &Path, file: &CompiledConfigFile) -> Result<(), MmError> {
    if file.uid.is_some() || file.gid.is_some() {
        return Err(MmError::Environment {
            message: "a config file's `uid` and `gid` need a Unix host".to_owned(),
        });
    }
    Ok(())
}

/// Removes every version in `directory` that is not in `keep`.
pub(super) fn prune(directory: &Path, keep: &BTreeSet<String>) -> Result<Vec<String>, MmError> {
    ensure_no_symlink_components(directory)?;
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("reading config file directory {}", directory.display()),
                source,
            });
        }
    };
    let mut removed = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| MmError::Io {
            context: format!("reading config file directory {}", directory.display()),
            source,
        })?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !is_version(&name) || keep.contains(&name) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => removed.push(name),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("removing config file version {}", entry.path().display()),
                    source,
                });
            }
        }
    }
    removed.sort();
    Ok(removed)
}
