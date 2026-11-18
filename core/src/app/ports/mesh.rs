use async_trait::async_trait;

use crate::domain::cluster_network::HostRange;
use crate::domain::error::MmError;
use crate::domain::mesh::{MeshApplied, MeshFacts, MeshNodeStatus, MeshState};

/// The WireGuard interface `mmd` owns on its own host. The private key is
/// made here and stays here: no method returns it.
#[async_trait]
pub trait MeshHost: Send + Sync {
    /// The node's public key, the interface state `mmd` last applied,
    /// whether the wg-quick mesh of an earlier mm version is still there,
    /// and [`MeshHost::host_ranges`]. Changes nothing.
    async fn facts(&self) -> Result<MeshFacts, MmError>;

    /// The interface state `mmd` last applied, read from its file; `None`
    /// on a host that holds no mesh.
    fn stored(&self) -> Result<Option<MeshState>, MmError> {
        Ok(None)
    }

    /// The addresses and routes the host has outside mm: everything but
    /// the mesh interface and the bridges of cluster networks. A mesh that
    /// is not on a real host has none.
    async fn host_ranges(&self) -> Result<Vec<HostRange>, MmError> {
        Ok(Vec::new())
    }

    /// Makes the node's key pair when it has none, taking over the key of an
    /// earlier mm version's interface when there is one, and returns the
    /// public key.
    async fn ensure_key(&self) -> Result<String, MmError>;

    /// Stores `desired` and brings the interface to it, touching only what
    /// differs: a peer that does not change keeps its session.
    async fn apply(&self, desired: &MeshState) -> Result<MeshApplied, MmError>;

    /// Brings the interface to the stored state, as at boot.
    async fn up(&self) -> Result<MeshApplied, MmError>;

    /// The interface and each peer's endpoint, last handshake, and traffic.
    async fn status(&self) -> Result<MeshNodeStatus, MmError>;

    /// Makes a new key pair next to the one in use and returns its public
    /// key. Repeating it returns the same pending key.
    async fn stage_key(&self) -> Result<String, MmError>;

    /// Switches the interface to the pending key and returns its public key.
    async fn activate_key(&self) -> Result<String, MmError>;

    /// Removes the interface, the stored state, and the key pair.
    async fn leave(&self) -> Result<MeshApplied, MmError>;
}
