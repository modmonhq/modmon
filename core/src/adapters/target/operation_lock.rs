use std::fs::File;
use std::path::{Path, PathBuf};

use crate::app::ports::{BuildOperationLock, TargetOperationLock, TargetOperationLockGuard};
use crate::domain::error::MmError;
use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

#[derive(Debug, Clone)]
pub struct FilesystemTargetOperationLock {
    root: PathBuf,
}

impl FilesystemTargetOperationLock {
    /// Creates a target operation lock adapter rooted at `MM_HOME/run/locks`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl TargetOperationLock for FilesystemTargetOperationLock {
    fn acquire(&self, target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        let file = acquire_platform(&self.root, target_id)?;
        Ok(Box::new(FilesystemTargetOperationLockGuard { _file: file }))
    }
}

impl BuildOperationLock for FilesystemTargetOperationLock {
    fn acquire(&self, project_id: ProjectId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        TargetOperationLock::acquire(self, TargetId(project_id.0))
    }
}

struct FilesystemTargetOperationLockGuard {
    // Closing this descriptor releases the process-lifetime fs4 lock.
    _file: File,
}

impl TargetOperationLockGuard for FilesystemTargetOperationLockGuard {}

#[cfg(unix)]
fn acquire_platform(root: &Path, target_id: TargetId) -> Result<File, MmError> {
    unix::acquire(root, target_id)
}

#[cfg(not(unix))]
fn acquire_platform(root: &Path, target_id: TargetId) -> Result<File, MmError> {
    use std::fs::{self, OpenOptions};

    use fs4::{FileExt, TryLockError};

    use crate::adapters::private_fs::ensure_no_symlink_components;

    validate_root_path(root)?;
    ensure_no_symlink_components(root)?;
    fs::create_dir_all(root).map_err(|source| MmError::Io {
        context: format!(
            "creating target operation lock directory {}",
            root.display()
        ),
        source,
    })?;
    ensure_no_symlink_components(root)?;
    if !fs::symlink_metadata(root)
        .map_err(|source| MmError::Io {
            context: format!(
                "inspecting target operation lock directory {}",
                root.display()
            ),
            source,
        })?
        .is_dir()
    {
        return Err(unsafe_lock_path(root));
    }

    let path = lock_path(root, target_id);
    ensure_no_symlink_components(&path)?;
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(&path).map_err(|source| MmError::Io {
                context: format!("inspecting target operation lock {}", path.display()),
                source,
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(unsafe_lock_path(&path));
            }
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|source| MmError::Io {
                    context: format!("opening target operation lock {}", path.display()),
                    source,
                })?
        }
        Err(source) => {
            return Err(MmError::Io {
                context: format!("creating target operation lock {}", path.display()),
                source,
            });
        }
    };
    ensure_no_symlink_components(&path)?;
    if !file
        .metadata()
        .map_err(|source| MmError::Io {
            context: format!("inspecting target operation lock {}", path.display()),
            source,
        })?
        .is_file()
    {
        return Err(unsafe_lock_path(&path));
    }
    match FileExt::try_lock(&file) {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(MmError::TargetOperationInProgress {
            target_id: target_id.0,
        }),
        Err(TryLockError::Error(source)) => Err(MmError::Io {
            context: format!("acquiring target operation lock {}", path.display()),
            source,
        }),
    }
}

fn lock_path(root: &Path, target_id: TargetId) -> PathBuf {
    root.join(format!("{}.lock", target_id.0))
}

fn validate_root_path(path: &Path) -> Result<(), MmError> {
    use std::path::Component;

    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(MmError::Security {
            message: format!(
                "target operation lock root '{}' must not be empty or contain parent components",
                path.display()
            ),
        });
    }
    Ok(())
}

fn unsafe_lock_path(path: &Path) -> MmError {
    MmError::Security {
        message: format!(
            "target operation lock path '{}' must be symlink-free, private, and owned by the current user",
            path.display()
        ),
    }
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsStr;
    use std::fs::File;
    use std::path::Path;

    use fs4::{FileExt, TryLockError};
    use rustix::fs::{AtFlags, FileType, Mode, OFlags};

    use super::{lock_path, unsafe_lock_path, validate_root_path};
    use crate::adapters::private_fs::{ensure_no_symlink_components, ensure_safe_path_ancestors};
    use crate::domain::error::MmError;
    use crate::domain::target::TargetId;

    const DIRECTORY_FLAGS: OFlags = OFlags::RDONLY
        .union(OFlags::DIRECTORY)
        .union(OFlags::NOFOLLOW)
        .union(OFlags::CLOEXEC);

    pub(super) fn acquire(root: &Path, target_id: TargetId) -> Result<File, MmError> {
        let (parent_path, root_name) = root_parent_and_name(root)?;
        let parent = open_private_root(parent_path)?;
        ensure_no_symlink_components(root)?;
        let root_directory = open_or_create_private_directory(&parent, root_name, root)?;
        let path = lock_path(root, target_id);
        let name = path.file_name().ok_or_else(|| unsafe_lock_path(&path))?;
        let lock = open_lock(&root_directory, name, &path)?;
        verify_lock(&root_directory, name, &lock, &path)?;

        match FileExt::try_lock(&lock) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(MmError::TargetOperationInProgress {
                    target_id: target_id.0,
                });
            }
            Err(TryLockError::Error(source)) => {
                return Err(MmError::Io {
                    context: format!("acquiring target operation lock {}", path.display()),
                    source,
                });
            }
        }

        verify_lock(&root_directory, name, &lock, &path)?;
        verify_directory_entry(&parent, root_name, &root_directory, root)?;
        verify_root_identity(parent_path, &parent)?;
        Ok(lock)
    }

    fn root_parent_and_name(path: &Path) -> Result<(&Path, &OsStr), MmError> {
        validate_root_path(path)?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| unsafe_lock_path(path))?;
        let name = path
            .file_name()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| unsafe_lock_path(path))?;
        Ok((parent, name))
    }

    fn open_private_root(path: &Path) -> Result<File, MmError> {
        validate_root_path(path)?;
        ensure_safe_path_ancestors(path)?;
        ensure_no_symlink_components(path)?;
        let directory = walk_path(path, true)?;
        validate_private_directory(&directory, path)?;
        verify_root_identity(path, &directory)?;
        Ok(directory)
    }

    fn verify_root_identity(path: &Path, directory: &File) -> Result<(), MmError> {
        ensure_no_symlink_components(path)?;
        let reopened = walk_path(path, false)?;
        if same_file(directory, &reopened)? {
            Ok(())
        } else {
            Err(filesystem_changed(path))
        }
    }

    fn walk_path(path: &Path, create: bool) -> Result<File, MmError> {
        let start = if path.is_absolute() { "/" } else { "." };
        let descriptor = rustix::fs::openat(rustix::fs::CWD, start, DIRECTORY_FLAGS, Mode::empty())
            .map_err(|error| MmError::Io {
                context: format!(
                    "opening start of target operation lock path {}",
                    path.display()
                ),
                source: error.into(),
            })?;
        let mut directory = File::from(descriptor);
        for component in path.components() {
            let std::path::Component::Normal(name) = component else {
                continue;
            };
            match try_open_directory(&directory, name, path)? {
                Some(child) => directory = child,
                None if create => {
                    create_private_directory(&directory, name, path)?;
                    directory = try_open_directory(&directory, name, path)?
                        .ok_or_else(|| filesystem_changed(path))?;
                }
                None => return Err(filesystem_changed(path)),
            }
        }
        Ok(directory)
    }

    fn open_or_create_private_directory(
        parent: &File,
        name: &OsStr,
        path: &Path,
    ) -> Result<File, MmError> {
        let directory = match try_open_directory(parent, name, path)? {
            Some(directory) => directory,
            None => {
                create_private_directory(parent, name, path)?;
                try_open_directory(parent, name, path)?.ok_or_else(|| filesystem_changed(path))?
            }
        };
        validate_private_directory(&directory, path)?;
        verify_directory_entry(parent, name, &directory, path)?;
        Ok(directory)
    }

    fn try_open_directory(
        parent: &File,
        name: &OsStr,
        path: &Path,
    ) -> Result<Option<File>, MmError> {
        match rustix::fs::openat(parent, name, DIRECTORY_FLAGS, Mode::empty()) {
            Ok(descriptor) => Ok(Some(File::from(descriptor))),
            Err(error) if error == rustix::io::Errno::NOENT => Ok(None),
            Err(error)
                if error == rustix::io::Errno::LOOP || error == rustix::io::Errno::NOTDIR =>
            {
                Err(unsafe_lock_path(path))
            }
            Err(error) => Err(MmError::Io {
                context: format!("opening target operation lock directory {}", path.display()),
                source: error.into(),
            }),
        }
    }

    fn create_private_directory(parent: &File, name: &OsStr, path: &Path) -> Result<(), MmError> {
        match rustix::fs::mkdirat(parent, name, Mode::RWXU) {
            Ok(()) => Ok(()),
            Err(error) if error == rustix::io::Errno::EXIST => Ok(()),
            Err(error) => Err(MmError::Io {
                context: format!(
                    "creating private target operation lock directory {}",
                    path.display()
                ),
                source: error.into(),
            }),
        }
    }

    fn validate_private_directory(directory: &File, path: &Path) -> Result<(), MmError> {
        let metadata = rustix::fs::fstat(directory).map_err(|error| MmError::Io {
            context: format!(
                "inspecting target operation lock directory {}",
                path.display()
            ),
            source: error.into(),
        })?;
        let mode = Mode::from_raw_mode(metadata.st_mode);
        if !FileType::from_raw_mode(metadata.st_mode).is_dir()
            || metadata.st_uid != rustix::process::geteuid().as_raw()
            || mode.intersects(Mode::RWXG | Mode::RWXO)
        {
            return Err(unsafe_lock_path(path));
        }
        Ok(())
    }

    fn open_lock(parent: &File, name: &OsStr, path: &Path) -> Result<File, MmError> {
        inspect_existing_lock(parent, name, path)?;
        let descriptor = match rustix::fs::openat(
            parent,
            name,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(descriptor) => descriptor,
            Err(error)
                if error == rustix::io::Errno::LOOP
                    || error == rustix::io::Errno::ISDIR
                    || error == rustix::io::Errno::NOTDIR =>
            {
                return Err(unsafe_lock_path(path));
            }
            Err(error) => {
                return Err(MmError::Io {
                    context: format!("opening target operation lock {}", path.display()),
                    source: error.into(),
                });
            }
        };
        Ok(File::from(descriptor))
    }

    fn inspect_existing_lock(parent: &File, name: &OsStr, path: &Path) -> Result<(), MmError> {
        match rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(metadata) => validate_lock_metadata(&metadata, path),
            Err(error) if error == rustix::io::Errno::NOENT => Ok(()),
            Err(error) => Err(MmError::Io {
                context: format!("inspecting target operation lock {}", path.display()),
                source: error.into(),
            }),
        }
    }

    fn verify_lock(parent: &File, name: &OsStr, lock: &File, path: &Path) -> Result<(), MmError> {
        let opened = rustix::fs::fstat(lock).map_err(|error| MmError::Io {
            context: format!("inspecting target operation lock {}", path.display()),
            source: error.into(),
        })?;
        validate_lock_metadata(&opened, path)?;
        let entry = entry_metadata(parent, name, path)?;
        validate_lock_metadata(&entry, path)?;
        if opened.st_dev != entry.st_dev || opened.st_ino != entry.st_ino {
            return Err(filesystem_changed(path));
        }
        Ok(())
    }

    fn validate_lock_metadata(metadata: &rustix::fs::Stat, path: &Path) -> Result<(), MmError> {
        let mode = Mode::from_raw_mode(metadata.st_mode);
        if !FileType::from_raw_mode(metadata.st_mode).is_file()
            || metadata.st_uid != rustix::process::geteuid().as_raw()
            || metadata.st_nlink != 1
            || mode.intersects(Mode::RWXG | Mode::RWXO)
        {
            return Err(unsafe_lock_path(path));
        }
        Ok(())
    }

    fn verify_directory_entry(
        parent: &File,
        name: &OsStr,
        directory: &File,
        path: &Path,
    ) -> Result<(), MmError> {
        let opened = rustix::fs::fstat(directory).map_err(|error| MmError::Io {
            context: format!(
                "inspecting target operation lock directory {}",
                path.display()
            ),
            source: error.into(),
        })?;
        let entry = entry_metadata(parent, name, path)?;
        if !FileType::from_raw_mode(opened.st_mode).is_dir()
            || !FileType::from_raw_mode(entry.st_mode).is_dir()
            || opened.st_dev != entry.st_dev
            || opened.st_ino != entry.st_ino
        {
            return Err(filesystem_changed(path));
        }
        Ok(())
    }

    fn entry_metadata(
        parent: &File,
        name: &OsStr,
        path: &Path,
    ) -> Result<rustix::fs::Stat, MmError> {
        rustix::fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|error| {
            if error == rustix::io::Errno::NOENT {
                filesystem_changed(path)
            } else {
                MmError::Io {
                    context: format!("inspecting target operation lock entry {}", path.display()),
                    source: error.into(),
                }
            }
        })
    }

    fn same_file(left: &File, right: &File) -> Result<bool, MmError> {
        let left = rustix::fs::fstat(left).map_err(|error| MmError::Io {
            context: "inspecting an opened target operation lock directory".to_owned(),
            source: error.into(),
        })?;
        let right = rustix::fs::fstat(right).map_err(|error| MmError::Io {
            context: "inspecting a reopened target operation lock directory".to_owned(),
            source: error.into(),
        })?;
        Ok(left.st_dev == right.st_dev && left.st_ino == right.st_ino)
    }

    fn filesystem_changed(path: &Path) -> MmError {
        MmError::Security {
            message: format!(
                "target operation lock path changed while it was being opened: {}",
                path.display()
            ),
        }
    }
}

#[cfg(all(test, unix))]
#[path = "operation_lock_tests.rs"]
mod tests;
