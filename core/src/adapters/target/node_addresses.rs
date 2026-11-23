//! The addresses this device connects to for cluster nodes
//! (`mm target preference set-address`), private to this machine under
//! MM_HOME and never in the repo.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::error::MmError;
use crate::domain::target::NodeAddress;

/// Per project and environment (`<project-id>/<environment>`), each node's
/// address.
pub type NodeAddresses = BTreeMap<String, BTreeMap<String, NodeAddress>>;

#[derive(Debug, Clone)]
pub struct NodeAddressStore {
    path: PathBuf,
}

impl NodeAddressStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn all(&self) -> Result<NodeAddresses, MmError> {
        super::private_json::read(&self.path)
    }

    pub fn environment(&self, key: &str) -> Result<BTreeMap<String, NodeAddress>, MmError> {
        Ok(self.all()?.remove(key).unwrap_or_default())
    }

    pub fn set(&self, key: &str, node: &str, address: NodeAddress) -> Result<(), MmError> {
        let mut all = self.all()?;
        all.entry(key.to_owned())
            .or_default()
            .insert(node.to_owned(), address);
        super::private_json::write(&self.path, &all, "node addresses")
    }

    /// Whether there was an address to remove.
    pub fn unset(&self, key: &str, node: &str) -> Result<bool, MmError> {
        let mut all = self.all()?;
        let Some(nodes) = all.get_mut(key) else {
            return Ok(false);
        };
        if nodes.remove(node).is_none() {
            return Ok(false);
        }
        if nodes.is_empty() {
            all.remove(key);
        }
        super::private_json::write(&self.path, &all, "node addresses")?;
        Ok(true)
    }
}
