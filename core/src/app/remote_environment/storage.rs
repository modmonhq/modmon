//! Shared volumes across the nodes of a cluster target: what a deployment
//! plan says about them, `mm target storage status`, and deleting one.

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode};
use crate::domain::error::MmError;
use crate::domain::remote::NodeDeploymentPlan;
use crate::domain::shared_storage::{
    ClusterStoragePlan, StorageNodeReport, StorageReport, VolumeDeletePlan, VolumeDeleted,
    VolumeName,
};

impl RemoteEnvironment {
    /// Names the node that keeps the target's shared volumes:
    /// `cluster.storage`, or the first node.
    #[must_use]
    pub fn with_storage_node(mut self, node: Option<String>) -> Self {
        self.storage_node = node;
        self
    }

    /// The node that keeps shared volumes, or why this target has none.
    fn storage_node(&self) -> Result<&str, MmError> {
        if let Some(interface) = &self.existing_interface {
            return Err(MmError::Environment {
                message: crate::domain::shared_storage::needs_mesh(&self.target, interface),
            });
        }
        self.storage_node
            .as_deref()
            .filter(|_| self.is_cluster())
            .ok_or_else(|| MmError::Environment {
                message: format!(
                    "target '{}' names one host, so it has no shared storage; shared volumes are served between the hosts a target lists under nodes:",
                    self.target
                ),
            })
    }

    /// The shared volumes of a deployment plan over `nodes`. Refused when
    /// the plan leaves out the node that keeps them: its directories have
    /// to exist before another node mounts them.
    pub(super) fn plan_storage(
        &self,
        nodes: &[NodeDeploymentPlan],
        skipped: &[String],
    ) -> Result<Option<ClusterStoragePlan>, MmError> {
        let uses = nodes
            .iter()
            .filter_map(|node| {
                let name = node.node.as_deref()?;
                let project = node.plan.plan.as_ref()?.node.as_ref()?;
                Some((name, project.shared_volumes.as_slice()))
            })
            .collect::<Vec<_>>();
        let Some(plan) =
            ClusterStoragePlan::of(&uses).map_err(|message| MmError::Environment { message })?
        else {
            return Ok(None);
        };
        if skipped.contains(&plan.node) {
            let volumes = plan
                .volumes
                .iter()
                .map(|volume| format!("{}/{}", volume.stack, volume.volume))
                .collect::<Vec<_>>();
            return Err(MmError::Environment {
                message: format!(
                    "this plan uses shared volume {} and leaves out '{}', the node that keeps shared volumes. Add {} to `--nodes`: it creates each volume's directory before another node mounts it",
                    volumes.join(", "),
                    plan.node,
                    plan.node
                ),
            });
        }
        Ok(Some(plan))
    }

    async fn storage_node_report(
        &self,
        node: &RemoteNode,
        storage_node: &str,
        sizes: bool,
    ) -> StorageNodeReport {
        let answer = match &node.transport {
            Ok(transport) => {
                transport
                    .storage_status(node.scope.clone(), storage_node.to_owned(), sizes)
                    .await
            }
            Err(unavailable) => Err(unavailable.error()),
        };
        let name = self.name_of(node).to_owned();
        match answer {
            Ok(status) => StorageNodeReport {
                name,
                answered: true,
                problem: None,
                status: Some(status),
            },
            Err(MmError::Unreachable { problem, help, .. }) => StorageNodeReport {
                name,
                answered: false,
                problem: Some(format!("{problem}. {help}")),
                status: None,
            },
            Err(error) => StorageNodeReport {
                name,
                answered: false,
                problem: Some(error.to_string()),
                status: None,
            },
        }
    }

    /// The target's shared storage as every node sees it, asked of all
    /// nodes at once. `declared` are the volumes the checkout lists under
    /// `shared-volumes`; `sizes` has the storage node count each volume's
    /// files. A node that does not answer is a row that says why.
    pub async fn storage_report(
        &self,
        declared: &[VolumeName],
        sizes: bool,
    ) -> Result<StorageReport, MmError> {
        let storage_node = self.storage_node()?;
        let nodes = join_all(
            self.nodes
                .iter()
                .map(|node| self.storage_node_report(node, storage_node, sizes)),
        )
        .await;
        Ok(StorageReport::new(
            (
                self.target.clone(),
                self.first().scope.environment.clone(),
                storage_node.to_owned(),
            ),
            declared,
            nodes,
        ))
    }

    /// Refuses to delete `name` while a node still has it mounted, or
    /// while a node cannot say.
    async fn unused_everywhere(
        &self,
        name: &VolumeName,
        declared: &[VolumeName],
    ) -> Result<(), MmError> {
        let report = self.storage_report(declared, false).await?;
        let silent = report
            .nodes
            .iter()
            .filter(|node| !node.answered)
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        if !silent.is_empty() {
            return Err(MmError::Environment {
                message: format!(
                    "{} did not answer, so mm cannot tell whether a container there still uses shared volume {name}; nothing was deleted. `mm target nodes` shows why a node does not answer",
                    silent.join(", ")
                ),
            });
        }
        let users = report
            .volumes
            .iter()
            .find(|volume| volume.stack == name.stack && volume.volume == name.volume)
            .map(|volume| volume.mounted_on.clone())
            .unwrap_or_default();
        if !users.is_empty() {
            return Err(MmError::Remote {
                code: "MM_STORAGE_IN_USE",
                message: format!(
                    "shared volume {name} is mounted on {}; nothing was deleted. Take '{}' out of stacks.{}.shared-volumes (or remove the services that mount it), deploy, and delete it then",
                    users.join(", "),
                    name.volume,
                    name.stack
                ),
            });
        }
        Ok(())
    }

    fn storage_transport(
        &self,
    ) -> Result<
        (
            &RemoteNode,
            &std::sync::Arc<dyn crate::app::ports::RemoteTransport>,
        ),
        MmError,
    > {
        let storage_node = self.storage_node()?;
        let node = self
            .nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(storage_node))
            .ok_or_else(|| MmError::Target {
                message: format!(
                    "target '{}' names '{storage_node}' as its storage node and lists no such node",
                    self.target
                ),
            })?;
        Ok((node, node.connect()?))
    }

    /// What `mm target storage delete` would remove: the volume's
    /// directory on the storage node, with its size. Refused while any
    /// node has the volume mounted.
    pub async fn plan_volume_delete(
        &self,
        name: &VolumeName,
        declared: &[VolumeName],
    ) -> Result<VolumeDeletePlan, MmError> {
        self.unused_everywhere(name, declared).await?;
        let (node, transport) = self.storage_transport()?;
        let mut plan = transport
            .plan_volume_delete(node.scope.clone(), name.clone())
            .await?;
        plan.declared = declared.contains(name);
        Ok(plan)
    }

    /// Deletes the volume on the storage node, once `plan_hash` is the
    /// hash its plan printed and no node has it mounted.
    pub async fn delete_volume(
        &self,
        name: &VolumeName,
        declared: &[VolumeName],
        plan_hash: &str,
    ) -> Result<VolumeDeleted, MmError> {
        self.unused_everywhere(name, declared).await?;
        let (node, transport) = self.storage_transport()?;
        transport
            .delete_volume(node.scope.clone(), name.clone(), plan_hash.to_owned())
            .await
    }
}
