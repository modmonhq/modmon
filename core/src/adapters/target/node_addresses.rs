//! The addresses this device connects to for cluster nodes
//! (`mm target preference set-address`), private to this machine under
//! MM_HOME and never in the repo.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::error::MmError;
use crate::domain::target::DeviceNodeAddress;

/// Per project and environment (`<project-id>/<environment>`), each node's
/// address.
pub type NodeAddresses = BTreeMap<String, BTreeMap<String, DeviceNodeAddress>>;

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

    pub fn environment(&self, key: &str) -> Result<BTreeMap<String, DeviceNodeAddress>, MmError> {
        Ok(self.all()?.remove(key).unwrap_or_default())
    }

    pub fn set(&self, key: &str, node: &str, address: DeviceNodeAddress) -> Result<(), MmError> {
        self.change(|all| {
            all.entry(key.to_owned())
                .or_default()
                .insert(node.to_owned(), address);
            true
        })
        .map(|_| ())
    }

    /// Whether there was an address to remove.
    pub fn unset(&self, key: &str, node: &str) -> Result<bool, MmError> {
        self.change(|all| {
            let Some(nodes) = all.get_mut(key) else {
                return false;
            };
            if nodes.remove(node).is_none() {
                return false;
            }
            if nodes.is_empty() {
                all.remove(key);
            }
            true
        })
    }

    fn change(&self, apply: impl FnOnce(&mut NodeAddresses) -> bool) -> Result<bool, MmError> {
        let lock_path = self.path.with_extension("lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|source| MmError::Io {
                context: format!("opening {}", lock_path.display()),
                source,
            })?;
        crate::adapters::file_lock::exclusive(&lock, &lock_path)?;
        let mut all = self.all()?;
        let changed = apply(&mut all);
        if changed {
            super::private_json::write(&self.path, &all, "node addresses")?;
        }
        Ok(changed)
    }
}
