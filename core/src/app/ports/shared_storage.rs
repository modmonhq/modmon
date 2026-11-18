use std::net::Ipv4Addr;

use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::mesh::MeshApplied;
use crate::domain::shared_storage::{StoredVolume, VolumeName};
use crate::domain::workspace::ProjectId;

/// The volume directories of one environment on a storage node, and the
/// disk they are on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredVolumes {
    pub volumes: Vec<StoredVolume>,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

/// Shared storage on `mmd`'s own host: the directories the storage node
/// keeps, what the host has mounted, and the packet filter rules that keep
/// storage traffic between the hosts.
///
/// The daemon creates, reads, and (on the one explicit command) deletes
/// directories. Loading the rules is done by the root-run `mmd storage up`,
/// at boot and during setup.
#[async_trait]
pub trait SharedStorageHost: Send + Sync {
    /// Whether setup exported the storage root on this host.
    fn exported(&self) -> bool;

    /// Creates the directories, given below the storage root, that are not
    /// there, and returns those. Never changes one that exists.
    fn create_directories(&self, directories: &[String]) -> Result<Vec<String>, MmError>;

    /// Whether the storage port of `address` answers from this host: the
    /// milliseconds it took, or why it did not.
    async fn reach(&self, address: Ipv4Addr) -> Result<u64, String>;

    /// The shared volumes of `project` in `environment` this host has
    /// mounted.
    fn mounted(&self, project: ProjectId, environment: &str) -> Result<Vec<VolumeName>, MmError>;

    /// The volume directories of `project` in `environment` on this host.
    /// With `sizes` each one's files are counted, within a time limit.
    async fn stored(
        &self,
        project: ProjectId,
        environment: &str,
        sizes: bool,
    ) -> Result<StoredVolumes, MmError>;

    /// Removes one volume's directory, given below the storage root, with
    /// everything in it. The only thing that ever deletes shared data.
    async fn delete(&self, directory: &str) -> Result<(), MmError>;

    /// Loads the packet filter rules that follow from the host's mesh.
    /// Nothing to do on a host that holds no mesh.
    async fn up(&self) -> Result<MeshApplied, MmError>;

    /// Removes the rules: for a host that is no longer a node of a cluster.
    async fn leave(&self) -> Result<MeshApplied, MmError>;
}
