use std::fs;
use std::io::Read;
use std::path::Path;

use crate::domain::error::MmError;

pub fn ensure_no_symlink_components(path: &Path) -> Result<(), MmError> {
    for component in path.ancestors() {
        match fs::symlink_metadata(component) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(MmError::Security {
                    message: format!(
                        "refusing to use path with symlink component '{}'",
                        component.display()
                    ),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("inspecting private path component {}", component.display()),
                    source,
                });
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
pub fn ensure_safe_path_ancestors(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|source| MmError::Io {
                context: "resolving the current directory for path validation".to_owned(),
                source,
            })?
            .join(path)
    };
    let current_user = rustix::process::geteuid().as_raw();
    for ancestor in absolute.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("inspecting path ancestor {}", ancestor.display()),
                    source,
                });
            }
        };
        let mode = metadata.permissions().mode();
        if !metadata.is_dir()
            || (metadata.uid() != 0 && metadata.uid() != current_user)
            || (mode & 0o022 != 0 && mode & 0o1000 == 0)
        {
            return Err(MmError::Security {
                message: format!(
                    "path ancestor '{}' must be owned by root or the current user and not be writable by other users unless protected by the sticky bit",
                    ancestor.display()
                ),
            });
        }
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_safe_path_ancestors(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)
}

#[cfg(unix)]
pub fn ensure_private_file(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting private file {}", path.display()),
        source,
    })?;
    if !metadata.file_type().is_file() {
        return Err(MmError::Security {
            message: format!("private path '{}' is not a regular file", path.display()),
        });
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(MmError::Security {
            message: format!(
                "private file '{}' must not be accessible by group or other users",
                path.display()
            ),
        });
    }
    if metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err(MmError::Security {
            message: format!(
                "private file '{}' is not owned by the current user",
                path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(unix)]
pub fn ensure_owned_by_current_user(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::MetadataExt;

    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting ownership of {}", path.display()),
        source,
    })?;
    if metadata.uid() == rustix::process::geteuid().as_raw() {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("path '{}' is not owned by the current user", path.display()),
        })
    }
}

#[cfg(unix)]
pub fn ensure_safe_identity_parent(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting identity directory {}", path.display()),
        source,
    })?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(MmError::Security {
            message: format!(
                "identity directory '{}' must be owned by the current user and not writable by other users",
                path.display()
            ),
        });
    }
    Ok(())
}

#[cfg(unix)]
pub fn ensure_user_controlled_directory(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting project directory {}", path.display()),
        source,
    })?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(MmError::Security {
            message: format!(
                "project directory '{}' must be owned by the current user and not writable by other users",
                path.display()
            ),
        });
    }
    ensure_safe_path_ancestors(path)?;
    Ok(())
}

#[cfg(unix)]
pub fn ensure_user_controlled_file(path: &Path) -> Result<(), MmError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path).map_err(|source| MmError::Io {
        context: format!("inspecting project file {}", path.display()),
        source,
    })?;
    if !metadata.file_type().is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.permissions().mode() & 0o022 != 0
    {
        return Err(MmError::Security {
            message: format!(
                "project file '{}' must be owned by the current user and not writable by other users",
                path.display()
            ),
        });
    }
    ensure_safe_path_ancestors(&path.with_file_name(""))?;
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_private_file(_path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()
}

#[cfg(not(unix))]
pub fn ensure_owned_by_current_user(_path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()
}

#[cfg(not(unix))]
pub fn ensure_safe_identity_parent(_path: &Path) -> Result<(), MmError> {
    ensure_private_permissions_supported()
}

#[cfg(not(unix))]
pub fn ensure_user_controlled_directory(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    if path.is_dir() {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("project path '{}' is not a directory", path.display()),
        })
    }
}

#[cfg(not(unix))]
pub fn ensure_user_controlled_file(path: &Path) -> Result<(), MmError> {
    ensure_no_symlink_components(path)?;
    if path.is_file() {
        Ok(())
    } else {
        Err(MmError::Security {
            message: format!("project path '{}' is not a regular file", path.display()),
        })
    }
}

pub fn read_regular_file(path: &Path, maximum_bytes: usize) -> Result<Option<Vec<u8>>, MmError> {
    let file = match open_read_only_no_follow(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("opening regular file {}", path.display()),
                source,
            });
        }
    };
    let metadata = file.metadata().map_err(|source| MmError::Io {
        context: format!("inspecting opened file {}", path.display()),
        source,
    })?;
    if !metadata.is_file() {
        return Err(MmError::Security {
            message: format!("refusing to read non-regular file '{}'", path.display()),
        });
    }
    if metadata.len() > u64::try_from(maximum_bytes).unwrap_or(u64::MAX) {
        return Err(MmError::Security {
            message: format!(
                "refusing to read '{}' because it exceeds {maximum_bytes} bytes",
                path.display()
            ),
        });
    }
    let limit = u64::try_from(maximum_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut contents = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(maximum_bytes)
            .min(maximum_bytes),
    );
    file.take(limit)
        .read_to_end(&mut contents)
        .map_err(|source| MmError::Io {
            context: format!("reading regular file {}", path.display()),
            source,
        })?;
    if contents.len() > maximum_bytes {
        return Err(MmError::Security {
            message: format!(
                "refusing to read '{}' because it exceeds {maximum_bytes} bytes",
                path.display()
            ),
        });
    }
    Ok(Some(contents))
}

#[cfg(unix)]
fn open_read_only_no_follow(path: &Path) -> Result<fs::File, std::io::Error> {
    use rustix::fs::{Mode, OFlags};

    rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map(fs::File::from)
    .map_err(std::io::Error::from)
}

#[cfg(not(unix))]
fn open_read_only_no_follow(path: &Path) -> Result<fs::File, std::io::Error> {
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err(std::io::Error::other("refusing to follow a symlink"));
    }
    fs::File::open(path)
}

#[cfg(unix)]
pub fn ensure_private_permissions_supported() -> Result<(), MmError> {
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_private_permissions_supported() -> Result<(), MmError> {
    Err(MmError::Security {
        message: "private file ACL enforcement is not implemented on this platform".to_owned(),
    })
}

#[cfg(all(test, unix))]
mod tests;
