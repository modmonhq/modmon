//! The mesh earlier mm versions wrote from their setup scripts: a wg-quick
//! configuration for `wg0` under `/etc/wireguard`, with the key next to it.
//! `mmd` reads it once to take the node's key and peers over, then removes it.

use std::fs;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use zeroize::Zeroizing;

use crate::domain::error::MmError;
use crate::domain::mesh::{LEGACY_MESH_INTERFACE, MeshPeer, MeshState};
use crate::domain::repo_target::Ipv4Cidr;

/// The first line the setup scripts wrote; nothing else is ever touched.
const MARKER: &str = "# Managed by mm target setup";

#[derive(Debug, Clone)]
pub(super) struct LegacyMesh {
    directory: PathBuf,
}

impl LegacyMesh {
    pub(super) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn configuration_path(&self) -> PathBuf {
        self.directory.join(format!("{LEGACY_MESH_INTERFACE}.conf"))
    }

    fn key_path(&self) -> PathBuf {
        self.directory.join(format!("{LEGACY_MESH_INTERFACE}.key"))
    }

    fn configuration(&self) -> Option<String> {
        match fs::read_to_string(self.configuration_path()) {
            Ok(text) if text.starts_with(MARKER) => Some(text),
            Ok(_) => None,
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(%error, "could not read the earlier WireGuard configuration");
                }
                None
            }
        }
    }

    /// Whether an earlier mm version's mesh configuration is on this host.
    pub(super) fn managed(&self) -> bool {
        self.configuration().is_some()
    }

    /// The node's key as the earlier setup made it.
    pub(super) fn private_key(&self) -> Result<Option<Zeroizing<String>>, MmError> {
        if !self.managed() {
            return Ok(None);
        }
        match fs::read_to_string(self.key_path()) {
            Ok(key) => Ok(Some(Zeroizing::new(key))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(MmError::Io {
                context: format!("reading {}", self.key_path().display()),
                source,
            }),
        }
    }

    /// The interface as the earlier configuration describes it. Its peers
    /// carry no names; the client names them by their mesh address.
    pub(super) fn state(&self) -> Option<MeshState> {
        parse(&self.configuration()?)
    }

    /// Removes the earlier configuration and key once `mm0` has taken over.
    pub(super) fn remove(&self) -> Result<(), MmError> {
        for path in [self.configuration_path(), self.key_path()] {
            match fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(MmError::Io {
                        context: format!("removing {}", path.display()),
                        source: error,
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Reads the wg-quick file the setup scripts rendered. Anything it cannot
/// read is left out: the plan then shows that peer as added.
pub(super) fn parse(configuration: &str) -> Option<MeshState> {
    let mut state = MeshState {
        interface: LEGACY_MESH_INTERFACE.to_owned(),
        address: Ipv4Addr::UNSPECIFIED,
        prefix: 0,
        port: 0,
        mtu: 0,
        peers: Vec::new(),
    };
    let mut in_peer = false;
    for line in configuration.lines() {
        let line = line.trim();
        if line == "[Peer]" {
            in_peer = true;
            state.peers.push(MeshPeer {
                name: String::new(),
                public_key: String::new(),
                endpoint: String::new(),
                allowed_ips: Vec::new(),
            });
            continue;
        }
        if line.starts_with('[') {
            in_peer = false;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match (in_peer, key, state.peers.last_mut()) {
            (false, "Address", _) => {
                let (address, prefix) = value.split_once('/')?;
                state.address = address.parse().ok()?;
                state.prefix = prefix.parse().ok()?;
            }
            (false, "ListenPort", _) => state.port = value.parse().ok()?,
            (false, "MTU", _) => state.mtu = value.parse().ok()?,
            (true, "PublicKey", Some(peer)) => peer.public_key = value.to_owned(),
            (true, "Endpoint", Some(peer)) => peer.endpoint = value.to_owned(),
            (true, "AllowedIPs", Some(peer)) => {
                peer.allowed_ips = value
                    .split(',')
                    .filter_map(|prefix| prefix.trim().parse::<Ipv4Cidr>().ok())
                    .collect();
            }
            _ => {}
        }
    }
    state.peers.retain(|peer| {
        !peer.public_key.is_empty() && !peer.endpoint.is_empty() && !peer.allowed_ips.is_empty()
    });
    (state.port != 0 && state.prefix != 0).then_some(state)
}
