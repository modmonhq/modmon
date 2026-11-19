//! Release binaries for hosts, kept under the mm home once checked.

use std::fs;
use std::path::{Path, PathBuf};

use crate::app::ports::ReleaseCache;
use crate::domain::error::MmError;

const DIGEST_FILE: &str = "archive.sha256";

#[derive(Debug, Clone)]
pub struct FilesystemReleaseCache {
    root: PathBuf,
}

impl FilesystemReleaseCache {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn release(&self, tag: &str) -> Result<PathBuf, MmError> {
        component(tag)?;
        Ok(self.root.join(tag))
    }
}

impl ReleaseCache for FilesystemReleaseCache {
    fn entry(&self, tag: &str, target: &str) -> Result<(PathBuf, Option<String>), MmError> {
        component(target)?;
        let directory = self.release(tag)?.join(target);
        let complete = ["mm", "mmd"]
            .iter()
            .all(|name| directory.join(name).is_file());
        let digest = match fs::read_to_string(directory.join(DIGEST_FILE)) {
            Ok(digest) if complete => Some(digest.trim().to_owned()),
            Ok(_) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(source) => return Err(io(&directory, source)),
        };
        Ok((directory, digest))
    }

    fn staging(&self, tag: &str) -> Result<PathBuf, MmError> {
        let release = self.release(tag)?;
        private_directory(&release)?;
        tempfile::Builder::new()
            .prefix(".staging-")
            .tempdir_in(&release)
            .map(tempfile::TempDir::keep)
            .map_err(|source| io(&release, source))
    }

    fn keep(
        &self,
        tag: &str,
        target: &str,
        unpacked: &Path,
        sha256: &str,
    ) -> Result<PathBuf, MmError> {
        component(target)?;
        let directory = self.release(tag)?.join(target);
        fs::write(unpacked.join(DIGEST_FILE), format!("{sha256}\n"))
            .map_err(|source| io(unpacked, source))?;
        let displaced = directory.with_file_name(format!(".old-{target}-{}", std::process::id()));
        if directory.exists() {
            fs::rename(&directory, &displaced).map_err(|source| io(&directory, source))?;
        }
        if let Err(source) = fs::rename(unpacked, &directory) {
            let (_, kept) = self.entry(tag, target)?;
            if kept.as_deref() != Some(sha256) {
                return Err(io(&directory, source));
            }
        }
        self.remove(&displaced)?;
        Ok(directory)
    }

    fn remove(&self, path: &Path) -> Result<(), MmError> {
        let removed = match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
            Ok(_) => fs::remove_file(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
        removed.map_err(|source| io(path, source))
    }
}

fn component(value: &str) -> Result<(), MmError> {
    if value.is_empty()
        || value.starts_with('.')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(MmError::State {
            message: format!("'{value}' cannot name a release cache entry"),
        });
    }
    Ok(())
}

fn private_directory(path: &Path) -> Result<(), MmError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path).map_err(|source| io(path, source))
}

fn io(path: &Path, source: std::io::Error) -> MmError {
    MmError::Io {
        context: format!("keeping release binaries in {}", path.display()),
        source,
    }
}
