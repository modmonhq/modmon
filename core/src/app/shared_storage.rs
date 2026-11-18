//! What a node's `mmd` does for the cluster's shared volumes: makes them
//! ready before a rollout, reports them, and deletes one on request.
//!
//! Nothing here removes data except [`SharedStorageNode::delete`], which is
//! `mm target storage delete` after its plan was reviewed.

use std::net::Ipv4Addr;
use std::sync::Arc;

use crate::app::ports::{MeshHost, SharedStorageHost};
use crate::domain::error::MmError;
use crate::domain::shared_storage::{
    SealedStorage, StorageNodeStatus, VolumeDeletePlan, VolumeDeleted, VolumeName, volume_directory,
};
use crate::domain::workspace::ProjectId;

/// One environment of one project, on the node that was asked.
#[derive(Debug, Clone, Copy)]
pub struct StorageScope<'a> {
    /// The node's own name; `None` on a host that is no cluster node.
    pub node: Option<&'a str>,
    pub project: ProjectId,
    pub environment: &'a str,
}

#[derive(Clone)]
pub struct SharedStorageNode {
    host: Arc<dyn SharedStorageHost>,
    mesh: Arc<dyn MeshHost>,
}

fn refusal(code: &'static str, message: String) -> MmError {
    MmError::Remote { code, message }
}

impl SharedStorageNode {
    #[must_use]
    pub fn new(host: Arc<dyn SharedStorageHost>, mesh: Arc<dyn MeshHost>) -> Self {
        Self { host, mesh }
    }

    /// The mesh address this node reaches `storage_node` at: its own when
    /// it is that node.
    fn address_of(&self, node: Option<&str>, storage_node: &str) -> Result<Ipv4Addr, String> {
        let no_mesh =
            || "this node holds no mesh; run `mm target setup plan` and apply it".to_owned();
        let state = self
            .mesh
            .stored()
            .map_err(|error| error.to_string())?
            .ok_or_else(no_mesh)?;
        if node == Some(storage_node) {
            return Ok(state.address);
        }
        state
            .peer_addresses()
            .get(storage_node)
            .copied()
            .ok_or_else(|| {
                format!(
                    "this node's mesh has no peer named '{storage_node}'; run `mm target setup plan` and apply it so every node holds the mesh"
                )
            })
    }

    /// Makes a plan's shared volumes ready on node `node`: the storage node
    /// creates the directories that are missing and returns them; a node
    /// that mounts one checks that it reaches the storage node. Changes
    /// nothing else and never removes anything.
    pub async fn prepare(
        &self,
        node: &str,
        storage: &SealedStorage,
    ) -> Result<Vec<String>, MmError> {
        if storage.node == node {
            if !self.host.exported() {
                return Err(refusal(
                    "MM_STORAGE_NOT_SERVED",
                    "this node keeps the plan's shared volumes and does not serve shared storage yet. Run `mm target setup plan` and apply it, then plan the deployment again; nothing was changed"
                        .to_owned(),
                ));
            }
            return self.host.create_directories(&storage.directories);
        }
        if !storage.mounting.iter().any(|mounting| mounting == node) {
            return Ok(Vec::new());
        }
        let unreachable = |problem: String| {
            refusal(
                "MM_STORAGE_UNREACHABLE",
                format!(
                    "this node mounts shared volumes from '{}' and cannot reach it: {problem}. Check that {} is up and that `mm target mesh status` shows a handshake between the two nodes (`mm target storage status` shows every node), then plan the deployment again; nothing was changed",
                    storage.node, storage.node
                ),
            )
        };
        let address = self
            .address_of(Some(node), &storage.node)
            .map_err(unreachable)?;
        self.host.reach(address).await.map_err(unreachable)?;
        Ok(Vec::new())
    }

    /// What this node says about shared storage kept on `storage_node`.
    pub async fn status(
        &self,
        scope: StorageScope<'_>,
        storage_node: &str,
        sizes: bool,
    ) -> Result<StorageNodeStatus, MmError> {
        let serves = scope.node == Some(storage_node);
        let mut status = StorageNodeStatus {
            node: scope.node.map(str::to_owned),
            serves,
            mounted: self.host.mounted(scope.project, scope.environment)?,
            ..StorageNodeStatus::default()
        };
        if serves {
            status.exported = Some(self.host.exported());
            let stored = self
                .host
                .stored(scope.project, scope.environment, sizes)
                .await?;
            status.volumes = stored.volumes;
            status.free_bytes = stored.free_bytes;
            status.total_bytes = stored.total_bytes;
        }
        match self.address_of(scope.node, storage_node) {
            Ok(address) => {
                status.address = Some(address);
                match self.host.reach(address).await {
                    Ok(took) => {
                        status.reachable = true;
                        status.reach_ms = Some(took);
                    }
                    Err(problem) => status.problem = Some(problem),
                }
            }
            Err(problem) => status.problem = Some(problem),
        }
        Ok(status)
    }

    /// What deleting `name` would remove. Refused anywhere but on the
    /// storage node, for a volume that is not there, and while a container
    /// of this node uses it.
    pub async fn plan_delete(
        &self,
        scope: StorageScope<'_>,
        name: &VolumeName,
    ) -> Result<VolumeDeletePlan, MmError> {
        let node = scope.node.unwrap_or_default();
        if !self.host.exported() {
            return Err(refusal(
                "MM_STORAGE_NOT_SERVED",
                format!(
                    "node '{node}' does not serve shared storage, so it keeps no shared volume to delete; `mm target storage status` names the storage node"
                ),
            ));
        }
        volume_directory(scope.project, scope.environment, name)
            .map_err(|message| MmError::Environment { message })?;
        let stored = self
            .host
            .stored(scope.project, scope.environment, true)
            .await?;
        let Some(volume) = stored
            .volumes
            .into_iter()
            .find(|stored| stored.stack == name.stack && stored.volume == name.volume)
        else {
            return Err(MmError::Environment {
                message: format!(
                    "node '{node}' keeps no shared volume {name} for environment '{}'; `mm target storage status` lists the ones it keeps",
                    scope.environment
                ),
            });
        };
        if self
            .host
            .mounted(scope.project, scope.environment)?
            .contains(name)
        {
            return Err(refusal(
                "MM_STORAGE_IN_USE",
                format!(
                    "a container on '{node}' uses shared volume {name}. Take the volume out of `shared-volumes` (or remove the services that mount it) and deploy, then delete it"
                ),
            ));
        }
        Ok(VolumeDeletePlan {
            plan_hash: VolumeDeletePlan::hash_of(
                scope.project,
                scope.environment,
                node,
                &volume.path,
            ),
            node: node.to_owned(),
            stack: volume.stack,
            volume: volume.volume,
            path: volume.path,
            bytes: volume.bytes.unwrap_or_default(),
            files: volume.files.unwrap_or_default(),
            counted: volume.counted,
            declared: false,
        })
    }

    /// Deletes the directory of `name` with everything in it, when
    /// `plan_hash` is the hash its plan printed.
    pub async fn delete(
        &self,
        scope: StorageScope<'_>,
        name: &VolumeName,
        plan_hash: &str,
    ) -> Result<VolumeDeleted, MmError> {
        let plan = self.plan_delete(scope, name).await?;
        if plan.plan_hash != plan_hash {
            return Err(MmError::Security {
                message: format!(
                    "plan {plan_hash} does not name deleting shared volume {name} on '{}'; run `mm target storage delete {}` again and use the hash it prints",
                    plan.node, name.volume
                ),
            });
        }
        let directory = volume_directory(scope.project, scope.environment, name)
            .map_err(|message| MmError::Environment { message })?;
        self.host.delete(&directory).await?;
        Ok(VolumeDeleted {
            node: plan.node,
            stack: plan.stack,
            volume: plan.volume,
            path: plan.path,
        })
    }
}
