//! Small JSON files private to this machine under MM_HOME: read whole,
//! replaced whole, readable by the owner only.

use std::path::Path;

use crate::domain::error::MmError;

/// The file's contents, or the default when there is no file yet.
pub(super) fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, MmError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| MmError::ConfigParse {
            path: path.to_path_buf(),
            message: error.to_string(),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(source) => Err(MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        }),
    }
}

/// Replaces the file with `value` in one rename, mode 0600.
pub(super) fn write<T: serde::Serialize>(
    path: &Path,
    value: &T,
    what: &str,
) -> Result<(), MmError> {
    let mut text = serde_json::to_string_pretty(value).map_err(|error| MmError::State {
        message: format!("serializing {what}: {error}"),
    })?;
    text.push('\n');
    let parent = path.parent().ok_or_else(|| MmError::State {
        message: format!("the {what} file has no directory"),
    })?;
    std::fs::create_dir_all(parent).map_err(|source| MmError::Io {
        context: format!("creating {}", parent.display()),
        source,
    })?;
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let temporary = parent.join(format!(".{stem}.{}", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&temporary)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary, path));
    written.map_err(|source| {
        if let Err(cleanup) = std::fs::remove_file(&temporary)
            && cleanup.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%cleanup, "could not remove a temporary {what} file");
        }
        MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        }
    })
}
