//! The files of this machine's `mm` installation, for `mm update`.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use crate::app::ports::InstallationFiles;
use crate::domain::error::MmError;
use crate::domain::release::RELEASE_BINARIES;
use crate::domain::self_update::{
    Installation, MAX_ARCHIVE_BYTES, PRIVILEGED_HELPER, RECEIPT_FILE, archive_member,
    receipt_checkout,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalInstallation;

impl InstallationFiles for LocalInstallation {
    fn inspect(&self, executable: &Path) -> Result<Installation, MmError> {
        let executable = fs::canonicalize(executable).map_err(|source| MmError::Io {
            context: format!("resolving the mm executable {}", executable.display()),
            source,
        })?;
        let directory =
            executable
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| MmError::Update {
                    message: format!("{} has no directory", executable.display()),
                })?;
        let mut beside = Vec::new();
        for name in RELEASE_BINARIES {
            let path = if name == "mm" {
                executable.clone()
            } else {
                directory.join(name)
            };
            match fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_file() => beside.push(name.to_owned()),
                Ok(_) => {
                    return Err(MmError::Update {
                        message: format!(
                            "{} is not a regular file, so mm update will not replace it; move it away, or install with the installer",
                            path.display()
                        ),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(MmError::Io {
                        context: format!("reading {}", path.display()),
                        source,
                    });
                }
            }
        }
        let receipt = directory.join(RECEIPT_FILE);
        let checkout = match fs::read_to_string(&receipt) {
            Ok(text) => Some(receipt_checkout(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", receipt.display()),
                    source,
                });
            }
        };
        let privileged = Path::new(PRIVILEGED_HELPER);
        Ok(Installation {
            writable: writable(&directory)?,
            privileged_helper: (cfg!(target_os = "linux") && privileged.is_file())
                .then(|| privileged.to_path_buf()),
            directory,
            executable,
            beside,
            checkout,
        })
    }

    fn staging(&self, directory: &Path) -> Result<PathBuf, MmError> {
        tempfile::Builder::new()
            .prefix(".mm-update-")
            .tempdir_in(directory)
            .map(tempfile::TempDir::keep)
            .map_err(|source| MmError::Io {
                context: format!("creating a staging directory in {}", directory.display()),
                source,
            })
    }

    fn unpack(&self, archive: &Path, into: &Path) -> Result<Vec<String>, MmError> {
        fs::create_dir(into).map_err(|source| MmError::Io {
            context: format!("creating {}", into.display()),
            source,
        })?;
        let file = fs::File::open(archive).map_err(|source| MmError::Io {
            context: format!("opening {}", archive.display()),
            source,
        })?;
        let mut reader = tar::Archive::new(flate2::read::GzDecoder::new(BufReader::new(file)));
        let mut top: Option<String> = None;
        let mut found = BTreeSet::new();
        for entry in reader
            .entries()
            .map_err(|source| damaged(archive, &source))?
        {
            let mut entry = entry.map_err(|source| damaged(archive, &source))?;
            let path = entry.path().map_err(|source| damaged(archive, &source))?;
            let path = path
                .to_str()
                .ok_or_else(|| unsafe_archive("a path that is not UTF-8"))?
                .to_owned();
            let kind = entry.header().entry_type();
            if kind.is_symlink() || kind.is_hard_link() {
                return Err(unsafe_archive(&format!("a link: {path}")));
            }
            if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
                continue;
            }
            let (directory, binary) =
                archive_member(&path).map_err(|message| MmError::Security {
                    message: format!("{message}; nothing was changed"),
                })?;
            if top.get_or_insert_with(|| directory.to_owned()).as_str() != directory {
                return Err(unsafe_archive("more than one top directory"));
            }
            if !kind.is_file() && !kind.is_dir() {
                return Err(unsafe_archive(&format!("a special file: {path}")));
            }
            if let Some(name) = binary.filter(|_| kind.is_file()) {
                if !found.insert(name.to_owned()) {
                    return Err(unsafe_archive(&format!("{name} twice")));
                }
                write_executable(&mut entry, &into.join(name))?;
            }
        }
        Ok(found.into_iter().collect())
    }

    fn replace(&self, staged: &Path, destination: &Path) -> Result<PathBuf, MmError> {
        let mut kept = destination.as_os_str().to_owned();
        kept.push(format!(".mm-old-{}", std::process::id()));
        let kept = PathBuf::from(kept);
        if fs::hard_link(destination, &kept).is_err() {
            fs::copy(destination, &kept).map_err(|source| MmError::Io {
                context: format!("keeping a copy of {}", destination.display()),
                source,
            })?;
        }
        fs::rename(staged, destination).map_err(|source| MmError::Io {
            context: format!("moving the new {} into place", destination.display()),
            source,
        })?;
        Ok(kept)
    }

    fn restore(&self, kept: &Path, destination: &Path) -> Result<(), MmError> {
        fs::rename(kept, destination).map_err(|source| MmError::Io {
            context: format!("putting {} back", destination.display()),
            source,
        })
    }

    fn remove(&self, path: &Path) -> Result<(), MmError> {
        let removed = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
            Ok(_) => fs::remove_file(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
        removed.map_err(|source| MmError::Io {
            context: format!("removing {}", path.display()),
            source,
        })
    }

    fn same_file_contents(&self, left: &Path, right: &Path) -> Result<bool, MmError> {
        let read = |path: &Path| {
            fs::read(path).map_err(|source| MmError::Io {
                context: format!("reading {}", path.display()),
                source,
            })
        };
        Ok(read(left)? == read(right)?)
    }
}

fn writable(directory: &Path) -> Result<bool, MmError> {
    match tempfile::Builder::new()
        .prefix(".mm-update-check-")
        .tempfile_in(directory)
    {
        Ok(probe) => probe.close().map(|()| true).map_err(|source| MmError::Io {
            context: format!("removing a probe file in {}", directory.display()),
            source,
        }),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::ReadOnlyFilesystem
            ) =>
        {
            Ok(false)
        }
        Err(source) => Err(MmError::Io {
            context: format!("checking that {} is writable", directory.display()),
            source,
        }),
    }
}

fn write_executable(entry: &mut impl Read, destination: &Path) -> Result<(), MmError> {
    let io = |source| MmError::Io {
        context: format!("unpacking {}", destination.display()),
        source,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(io)?;
    let copied = std::io::copy(&mut entry.take(MAX_ARCHIVE_BYTES + 1), &mut file).map_err(io)?;
    if copied > MAX_ARCHIVE_BYTES {
        return Err(unsafe_archive("a binary larger than any release holds"));
    }
    file.flush().map_err(io)?;
    file.sync_all().map_err(io)?;
    set_executable(destination).map_err(io)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn damaged(archive: &Path, source: &std::io::Error) -> MmError {
    MmError::Security {
        message: format!(
            "{} is not a readable release archive ({source}); nothing was changed",
            archive.display()
        ),
    }
}

fn unsafe_archive(what: &str) -> MmError {
    MmError::Security {
        message: format!("the release archive holds {what}; nothing was changed"),
    }
}
