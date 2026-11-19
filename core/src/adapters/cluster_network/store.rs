//! The file `mmd` keeps for the cluster's networks under
//! `<state root>/networks`. It holds addresses only, nothing secret, and is
//! all the root-run `mmd network up` needs to restore routes and rules.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::domain::cluster_network::NodeNetworks;
use crate::domain::error::MmError;
use crate::domain::stored_state::{StoredOn, unknown_in, written_by_newer_mm};

const STATE: &str = "networks.json";

#[derive(Debug, Clone)]
pub struct NetworkStore {
    directory: PathBuf,
}

impl NetworkStore {
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn path(&self) -> PathBuf {
        self.directory.join(STATE)
    }

    /// What the last deployment stored.
    pub fn state(&self) -> Result<Option<NodeNetworks>, MmError> {
        let path = self.path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(MmError::Io {
                    context: format!("reading {}", path.display()),
                    source,
                });
            }
        };
        let state: NodeNetworks = serde_json::from_str(&text).map_err(|error| {
            if let Some(unknown) = unknown_in(&error) {
                return written_by_newer_mm(&path, &unknown, StoredOn::Host);
            }
            MmError::State {
                message: format!(
                    "{} is not a network state mmd wrote ({error}); `mm deploy plan` and its apply write it again",
                    path.display()
                ),
            }
        })?;
        state.table.validate().map_err(|message| MmError::State {
            message: format!("{}: {message}", path.display()),
        })?;
        Ok(Some(state))
    }

    /// Writes through a temporary file in the same directory, so a reader
    /// never sees half a file. Returns whether the stored state changed.
    pub fn write(&self, state: &NodeNetworks) -> Result<bool, MmError> {
        let mut bytes = serde_json::to_vec_pretty(state).map_err(|error| MmError::State {
            message: format!("serializing the network state: {error}"),
        })?;
        bytes.push(b'\n');
        let path = self.path();
        if fs::read(&path).is_ok_and(|current| current == bytes) {
            return Ok(false);
        }
        let io = |context: String| move |source| MmError::Io { context, source };
        fs::create_dir_all(&self.directory)
            .map_err(io(format!("creating {}", self.directory.display())))?;
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
            .and_then(|mut file| file.write_all(&bytes).and_then(|()| file.sync_all()))
            .and_then(|()| fs::rename(&temporary, &path));
        written.map_err(|source| {
            if let Err(error) = fs::remove_file(&temporary)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%error, "could not remove a temporary network state file");
            }
            MmError::Io {
                context: format!("writing {}", path.display()),
                source,
            }
        })?;
        Ok(true)
    }

    /// Removes the stored state; returns whether there was one.
    pub fn remove(&self) -> Result<bool, MmError> {
        let path = self.path();
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(MmError::Io {
                context: format!("removing {}", path.display()),
                source,
            }),
        }
    }
}
