use async_trait::async_trait;

use crate::domain::cluster_network::{NetworkDrift, NodeNetworks};
use crate::domain::error::MmError;
use crate::domain::mesh::MeshApplied;

/// The cluster networks on `mmd`'s own host: the state the last deployment
/// stored, and the routes, firewall rules, and bridge settings that follow
/// from it.
///
/// Storing is the daemon's part. Changing the host's network is done by the
/// root-run `mmd network up`, at boot and whenever the daemon asks for it
/// with [`ClusterNetworkHost::converge`].
#[async_trait]
pub trait ClusterNetworkHost: Send + Sync {
    /// What the node stored at its last deployment. Reads a file only.
    fn stored(&self) -> Result<Option<NodeNetworks>, MmError>;

    /// Stores `desired` for the next `up`; returns whether it changed.
    /// Nothing on the host's network changes. Also writes, for each network
    /// the node holds a part of, the resolver file its containers mount.
    fn store(&self, desired: &NodeNetworks) -> Result<bool, MmError>;

    /// The directory of the node's resolver files, one per cluster network
    /// named `<network's name across the cluster>.conf`.
    fn resolver_files(&self) -> std::path::PathBuf;

    /// Has the host brought to the stored state and waits for it: through
    /// the unit that runs `mmd network up`, or directly on a host without
    /// systemd.
    async fn converge(&self) -> Result<(), MmError>;

    /// Brings the host to the stored state: the routes to the other nodes'
    /// slices, mm's firewall chains, and the settings of each network's
    /// bridge. `before_engine` loads only the firewall chains, which have
    /// to be in place before the engine starts a container.
    async fn up(&self, before_engine: bool) -> Result<MeshApplied, MmError>;

    /// What the kernel has that differs from the stored state.
    async fn drift(&self) -> Result<Vec<NetworkDrift>, MmError>;

    /// Has a network's bridge that the engine created since the last call
    /// brought to the stored state, through [`Self::converge`]. Podman
    /// creates a network's bridge with its first container and removes it
    /// with its last, so a bridge can come back without its settings.
    /// Returns whether it converged; costs a small file read per bridge
    /// otherwise.
    async fn settle_bridges(&self) -> Result<bool, MmError> {
        Ok(false)
    }

    /// Removes the engine's network `network` when no container is on it;
    /// returns whether it is gone. A host without engine networks has none.
    async fn remove_bridge(&self, _network: &str) -> Result<bool, MmError> {
        Ok(true)
    }

    /// Removes mm's firewall chains and the stored state.
    async fn leave(&self) -> Result<MeshApplied, MmError>;
}
