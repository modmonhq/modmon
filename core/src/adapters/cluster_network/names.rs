//! The resolver files of a node: one per cluster network, under
//! `<state root>/networks/resolv`. The containers of a network's services
//! get the network's file as their `/etc/resolv.conf`; it names the node's
//! own resolver on the network's gateway address.

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::cluster_network::NodeNetworks;
use crate::domain::discovery::{resolver_file, resolver_file_name};
use crate::domain::error::MmError;

#[derive(Debug, Clone)]
pub struct ResolverFiles {
    directory: PathBuf,
    /// The host's own resolver configuration, whose search domains the
    /// containers keep.
    host: PathBuf,
}

impl ResolverFiles {
    #[must_use]
    pub fn new(directory: PathBuf, host: PathBuf) -> Self {
        Self { directory, host }
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Writes the file of every network `state` gives this node a part of.
    /// A file that already says the same is left alone, and one that
    /// changes is rewritten in place: containers hold the file itself, not
    /// its name, so replacing it would leave them with the old one.
    pub fn write(&self, state: &NodeNetworks) -> Result<(), MmError> {
        let local = state.local();
        if local.is_empty() {
            return Ok(());
        }
        let host = match fs::read_to_string(&self.host) {
            Ok(host) => host,
            Err(error) => {
                tracing::debug!(%error, "cannot read the host's resolver configuration");
                String::new()
            }
        };
        let io = |context: String| move |source| MmError::Io { context, source };
        fs::create_dir_all(&self.directory)
            .map_err(io(format!("creating {}", self.directory.display())))?;
        readable(&self.directory, 0o755)?;
        for network in local {
            let path = self.directory.join(resolver_file_name(&network.key));
            let contents = resolver_file(network.gateway, &host);
            if fs::read_to_string(&path).is_ok_and(|current| current == contents) {
                continue;
            }
            fs::write(&path, contents).map_err(io(format!("writing {}", path.display())))?;
            // Read by whatever user a container runs as.
            readable(&path, 0o644)?;
        }
        Ok(())
    }
}

#[cfg(unix)]
fn readable(path: &Path, mode: u32) -> Result<(), MmError> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| MmError::Io {
        context: format!("setting the mode of {}", path.display()),
        source,
    })
}

#[cfg(not(unix))]
fn readable(_path: &Path, _mode: u32) -> Result<(), MmError> {
    Ok(())
}
