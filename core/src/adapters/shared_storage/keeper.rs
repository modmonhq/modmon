//! Starts the containers of this node that could not start because shared
//! storage was away, once the storage node answers again.
//!
//! This is what makes a shared volume come back when the storage node
//! boots after the others: the engine gives up on a container whose volume
//! it cannot mount and never tries again. A container that someone
//! stopped, or that failed for another reason, is never touched (see
//! `domain::shared_storage::WaitingContainer`).

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use crate::app::ports::{MeshHost, RuntimeDriver, SharedStorageHost};
use crate::domain::error::MmError;
use crate::domain::shared_storage::STORAGE_LABEL;

/// How often the node looks for containers that wait for storage.
const LOOK_EVERY: Duration = Duration::from_secs(20);

pub struct StorageKeeper {
    runtime: Arc<dyn RuntimeDriver>,
    storage: Arc<dyn SharedStorageHost>,
    mesh: Arc<dyn MeshHost>,
}

impl StorageKeeper {
    #[must_use]
    pub fn new(
        runtime: Arc<dyn RuntimeDriver>,
        storage: Arc<dyn SharedStorageHost>,
        mesh: Arc<dyn MeshHost>,
    ) -> Arc<Self> {
        Arc::new(Self {
            runtime,
            storage,
            mesh,
        })
    }

    /// Looks every few seconds for as long as the process lives. On a host
    /// that holds no mesh it asks the engine nothing.
    pub async fn run(self: Arc<Self>) {
        loop {
            tokio::time::sleep(LOOK_EVERY).await;
            if let Err(error) = self.look().await {
                tracing::debug!(%error, "looking for containers that wait for shared storage failed");
            }
        }
    }

    /// One look: every container that waits for a storage node that
    /// answers is started. Returns the names of the ones it started.
    pub async fn look(&self) -> Result<Vec<String>, MmError> {
        if self.mesh.stored()?.is_none() {
            return Ok(Vec::new());
        }
        let mut waiting = BTreeMap::<Ipv4Addr, Vec<(String, String)>>::new();
        for container in self.runtime.waiting_containers(STORAGE_LABEL).await? {
            if let Some(address) = container.waits_for() {
                waiting
                    .entry(address)
                    .or_default()
                    .push((container.id, container.name));
            }
        }
        let mut started = Vec::new();
        for (address, containers) in waiting {
            if self.storage.reach(address).await.is_err() {
                continue;
            }
            for (id, name) in containers {
                match self.runtime.start_container(&id).await {
                    Ok(()) => {
                        tracing::warn!(
                            container = %name,
                            storage = %address,
                            "started a container that waited for shared storage"
                        );
                        started.push(name);
                    }
                    Err(error) => tracing::warn!(
                        container = %name,
                        storage = %address,
                        %error,
                        "a container that waits for shared storage did not start"
                    ),
                }
            }
        }
        Ok(started)
    }
}
