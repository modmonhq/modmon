//! The files `mmd` keeps for its mesh under `<state root>/mesh`: the applied
//! interface state, the node's key pair, and a pending key during a rotation.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::domain::error::MmError;
use crate::domain::mesh::MeshState;

const STATE: &str = "mesh.json";
const PRIVATE_KEY: &str = "private.key";
const PUBLIC_KEY: &str = "public.key";
const PENDING_PRIVATE_KEY: &str = "next.key";
const PENDING_PUBLIC_KEY: &str = "next.pub";

#[derive(Debug, Clone)]
pub(super) struct MeshStore {
    directory: PathBuf,
}

fn io(context: String) -> impl FnOnce(std::io::Error) -> MmError {
    move |source| MmError::Io { context, source }
}

fn read_optional(path: &Path) -> Result<Option<String>, MmError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        }),
    }
}

/// Writes through a temporary file in the same directory, readable by root
/// only, so a reader never sees half a file.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), MmError> {
    let temporary = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options
        .open(&temporary)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&temporary, path));
    written.map_err(|source| {
        if let Err(error) = fs::remove_file(&temporary)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "could not remove a temporary mesh file");
        }
        MmError::Io {
            context: format!("writing {}", path.display()),
            source,
        }
    })
}

fn remove(path: &Path) -> Result<(), MmError> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(io(format!("removing {}", path.display()))(error))
        }
        _ => Ok(()),
    }
}

impl MeshStore {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join(name)
    }

    fn ensure_directory(&self) -> Result<(), MmError> {
        fs::create_dir_all(&self.directory)
            .map_err(io(format!("creating {}", self.directory.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700))
                .map_err(io(format!("securing {}", self.directory.display())))?;
        }
        Ok(())
    }

    /// The interface state last applied.
    pub(super) fn state(&self) -> Result<Option<MeshState>, MmError> {
        let path = self.path(STATE);
        let Some(text) = read_optional(&path)? else {
            return Ok(None);
        };
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| MmError::State {
                message: format!(
                    "{} is not a mesh state mmd wrote ({error}); run `mm target setup plan` and apply it to write it again",
                    path.display()
                ),
            })
    }

    /// Stores `state` unless the file already holds exactly it. Returns
    /// whether the file changed.
    pub(super) fn write_state(&self, state: &MeshState) -> Result<bool, MmError> {
        let mut bytes = serde_json::to_vec_pretty(state).map_err(|error| MmError::State {
            message: format!("serializing the mesh state: {error}"),
        })?;
        bytes.push(b'\n');
        let path = self.path(STATE);
        if read_optional(&path)?.is_some_and(|current| current.as_bytes() == bytes) {
            return Ok(false);
        }
        self.ensure_directory()?;
        write_private(&path, &bytes)?;
        Ok(true)
    }

    /// The file WireGuard reads the node's private key from.
    pub(super) fn private_key_path(&self) -> PathBuf {
        self.path(PRIVATE_KEY)
    }

    pub(super) fn has_key(&self) -> bool {
        self.private_key_path().is_file()
    }

    /// The private key, for deriving a public key that was not stored.
    pub(super) fn private_key(&self) -> Result<Option<Zeroizing<String>>, MmError> {
        Ok(read_optional(&self.private_key_path())?.map(Zeroizing::new))
    }

    pub(super) fn public_key(&self) -> Result<Option<String>, MmError> {
        Ok(read_optional(&self.path(PUBLIC_KEY))?.map(|key| key.trim().to_owned()))
    }

    pub(super) fn pending_public_key(&self) -> Result<Option<String>, MmError> {
        if !self.path(PENDING_PRIVATE_KEY).is_file() {
            return Ok(None);
        }
        Ok(read_optional(&self.path(PENDING_PUBLIC_KEY))?.map(|key| key.trim().to_owned()))
    }

    /// Stores a key pair: the one in use, or the pending one of a rotation.
    pub(super) fn write_key_pair(
        &self,
        private_key: &Zeroizing<String>,
        public_key: &str,
        pending: bool,
    ) -> Result<(), MmError> {
        self.ensure_directory()?;
        let (private_name, public_name) = if pending {
            (PENDING_PRIVATE_KEY, PENDING_PUBLIC_KEY)
        } else {
            (PRIVATE_KEY, PUBLIC_KEY)
        };
        // The private half goes last: a pair is there once its key file is.
        write_private(
            &self.path(public_name),
            format!("{}\n", public_key.trim()).as_bytes(),
        )?;
        write_private(
            &self.path(private_name),
            format!("{}\n", private_key.trim()).as_bytes(),
        )
    }

    /// Makes the pending key pair the one in use.
    pub(super) fn activate_pending(&self) -> Result<(), MmError> {
        for (from, to) in [
            (PENDING_PUBLIC_KEY, PUBLIC_KEY),
            (PENDING_PRIVATE_KEY, PRIVATE_KEY),
        ] {
            fs::rename(self.path(from), self.path(to))
                .map_err(io(format!("activating the pending mesh key ({to})")))?;
        }
        Ok(())
    }

    /// Removes the state and both key pairs.
    pub(super) fn remove_all(&self) -> Result<(), MmError> {
        for name in [
            STATE,
            PRIVATE_KEY,
            PUBLIC_KEY,
            PENDING_PRIVATE_KEY,
            PENDING_PUBLIC_KEY,
        ] {
            remove(&self.path(name))?;
        }
        match fs::remove_dir(&self.directory) {
            Err(error)
                if !matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) =>
            {
                Err(io(format!("removing {}", self.directory.display()))(error))
            }
            _ => Ok(()),
        }
    }
}
