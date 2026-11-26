use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::node_metrics::procfs::{FilesystemSample, HostSample};
use crate::domain::node_metrics::{DeployedEnvironment, LabelledContainer, PeerSample};

/// The host's own numbers. Reading them reads files and can block on a
/// filesystem, so it belongs on a thread that serves nothing else.
pub trait HostStats: Send + Sync {
    fn read(&self) -> HostSample;

    /// The host's local filesystems with their usage. `statvfs` waits for
    /// a filesystem that hangs, so this is read apart from the rest.
    fn filesystems(&self) -> Vec<FilesystemSample>;
}

/// The containers of every environment on this node, as the engine shows
/// them.
#[async_trait]
pub trait NodeContainers: Send + Sync {
    async fn containers(&self) -> Result<Vec<LabelledContainer>, MmError>;
}

/// The node's mesh peers as its interface shows them; `None` on a host
/// that holds no mesh.
#[async_trait]
pub trait MeshPeers: Send + Sync {
    async fn peers(&self) -> Result<Option<Vec<PeerSample>>, MmError>;
}

/// What each environment's last deployment runs on this node. Reads the
/// node's records and the deployed checkouts, so it blocks.
pub trait DeployedEnvironments: Send + Sync {
    fn deployed(&self) -> Result<Vec<DeployedEnvironment>, MmError>;
}
