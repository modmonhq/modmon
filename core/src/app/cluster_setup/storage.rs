//! Shared storage and the first access sync.

use super::ClusterSetupService;
use super::mesh::mmd as node_mmd;
use super::steps::mmd;
use crate::app::ports::{ClusterNodeCommand, ClusterScript};
use crate::domain::cluster_setup::{ClusterNodePlan, ClusterSetupPlan};
use crate::domain::error::MmError;
use crate::domain::shared_storage::{STORAGE_ROOT, migration_steps};

/// Where the storage node keeps shared volumes.
pub const STORAGE_HOST_ROOT: &str = STORAGE_ROOT;

fn script(script: ClusterScript, arguments: Vec<String>) -> ClusterNodeCommand {
    ClusterNodeCommand::Script { script, arguments }
}

impl ClusterSetupService {
    /// The storage node serves shared volumes from its own kernel NFS
    /// server, listening on its mesh address only and exporting only to
    /// the nodes' mesh addresses. Every node then loads the rules that keep
    /// its containers away from the storage port, and every other node
    /// proves that it can mount the export.
    ///
    /// A node that served before and keeps files is never taken out of
    /// service: changing `cluster.storage` is refused with the steps to
    /// move the files by hand. One that keeps no file stops serving.
    pub(super) async fn serve_storage(&self, plan: &ClusterSetupPlan) -> Result<(), MmError> {
        let node = plan
            .nodes
            .iter()
            .find(|node| node.name == plan.storage)
            .ok_or_else(|| MmError::Target {
                message: format!(
                    "the setup plan names storage node {}, which it does not list",
                    plan.storage
                ),
            })?;
        let others = plan
            .nodes
            .iter()
            .filter(|other| other.name != node.name)
            .collect::<Vec<_>>();
        self.hand_over_storage(plan).await?;
        let mut arguments = vec![
            STORAGE_HOST_ROOT.to_owned(),
            node.private_address.to_string(),
        ];
        // Every node may mount, whether or not this setup run touches it.
        arguments.extend(plan.private_addresses().iter().map(ToString::to_string));
        let shell = &self.dependencies.shell;
        shell
            .run(
                &self.connection(plan, node),
                &script(ClusterScript::Storage, arguments),
                None,
            )
            .await?;
        for node in &plan.nodes {
            shell
                .run(
                    &self.connection(plan, node),
                    &node_mmd(&["storage", "up"]),
                    None,
                )
                .await?;
        }
        for other in others {
            shell
                .run(
                    &self.connection(plan, other),
                    &script(
                        ClusterScript::StorageReach,
                        vec![node.private_address.to_string()],
                    ),
                    None,
                )
                .await
                .map_err(|error| MmError::Target {
                    message: format!(
                        "node {} cannot mount shared storage from {} ({}): {error}. `mm target mesh status` shows whether the two nodes reach each other; the storage server listens on TCP port {} of {}'s mesh address",
                        other.name,
                        node.name,
                        node.private_address,
                        crate::domain::shared_storage::NFS_PORT,
                        node.name
                    ),
                })?;
        }
        Ok(())
    }

    /// The nodes among `nodes` that serve shared storage now, each with
    /// whether it keeps files there. Changes nothing.
    async fn serving_now(
        &self,
        user: &str,
        nodes: &[&ClusterNodePlan],
    ) -> Result<Vec<(String, bool)>, MmError> {
        let answers = futures_util::future::join_all(nodes.iter().map(|node| async move {
            self.dependencies
                .shell
                .run(
                    &self.connection_as(user, node),
                    &script(
                        ClusterScript::StorageFacts,
                        vec![STORAGE_HOST_ROOT.to_owned()],
                    ),
                    None,
                )
                .await
        }))
        .await;
        let mut serving = Vec::new();
        for (node, facts) in nodes.iter().zip(answers) {
            let facts = facts?;
            let said = |fact: &str| facts.split_whitespace().any(|word| word == fact);
            if said("serves=yes") {
                serving.push((node.name.clone(), said("files=yes")));
            }
        }
        Ok(serving)
    }

    /// Refuses a plan that takes shared storage away from a node that
    /// keeps files, with the steps to move them by hand; mm never copies
    /// or deletes them. Returns a notice for each node that keeps no file
    /// and will stop serving. Asks the nodes among `nodes` whose host key
    /// `asked` accepts, so a plan asks the ones the repo already records
    /// and an apply asks them all.
    pub(super) async fn storage_handover(
        &self,
        (target, user, storage): (&str, &str, &str),
        nodes: &[ClusterNodePlan],
        asked: impl Fn(&ClusterNodePlan) -> bool,
    ) -> Result<Vec<String>, MmError> {
        let others = nodes
            .iter()
            .filter(|node| node.name != storage && asked(node))
            .collect::<Vec<_>>();
        let mut leaving = Vec::new();
        for (node, keeps_files) in self.serving_now(user, &others).await? {
            if keeps_files {
                return Err(MmError::Target {
                    message: format!(
                        "targets.{target}.cluster.storage now names {storage}, but {}. Nothing was changed",
                        migration_steps(target, &node, storage)
                    ),
                });
            }
            leaving.push(node);
        }
        Ok(leaving)
    }

    /// At plan time: the nodes the repo already records are asked, so a
    /// storage node that changed is refused before anything is applied,
    /// and a node that will stop serving is named in the plan.
    pub(super) async fn planned_handover(
        &self,
        steps: &[crate::domain::cluster_setup::ClusterSetupStep],
        (target, user, storage): (&str, &str, &str),
        nodes: &[ClusterNodePlan],
    ) -> Result<Vec<String>, MmError> {
        if !steps.contains(&crate::domain::cluster_setup::ClusterSetupStep::ServeStorage) {
            return Ok(Vec::new());
        }
        let leaving = self
            .storage_handover((target, user, storage), nodes, |node| {
                node.host_key_origin == crate::domain::cluster_setup::HostKeyOrigin::Recorded
            })
            .await?;
        Ok(leaving
            .into_iter()
            .map(|node| {
                format!(
                    "storage: moves from {node} to {storage}. {node} keeps no file of a shared volume and stops serving"
                )
            })
            .collect())
    }

    /// Before the storage node serves: a node that served until now and
    /// keeps files refuses the change; one that keeps none stops serving.
    /// Nothing is changed before every node of the plan has answered.
    async fn hand_over_storage(&self, plan: &ClusterSetupPlan) -> Result<(), MmError> {
        let leaving = self
            .storage_handover(
                (&plan.target, &plan.setup.user, &plan.storage),
                &plan.nodes,
                |_| true,
            )
            .await?;
        for node in plan
            .nodes
            .iter()
            .filter(|node| leaving.contains(&node.name))
        {
            self.dependencies
                .shell
                .run(
                    &self.connection(plan, node),
                    &script(ClusterScript::StorageWithdraw, Vec::new()),
                    None,
                )
                .await?;
        }
        Ok(())
    }

    /// The first users-file sync, as root on every node. Each node keeps its
    /// own access record, so each one reads the users file itself.
    pub(super) async fn sync_access(&self, plan: &ClusterSetupPlan) -> Result<(), MmError> {
        let ClusterNodeCommand::Program(mut argv) = mmd(plan, &["access", "sync"]) else {
            return Err(MmError::State {
                message: "unexpected mmd command shape".to_owned(),
            });
        };
        argv.extend(["--commit".to_owned(), plan.access_commit.clone()]);
        // Nodes that fetch nothing get the commit's files with the sync,
        // on the connection setup already has, as text on standard input.
        let bundle = if plan.pushes() {
            argv.push("--bundle-stdin".to_owned());
            Some(self.access_bundle(&plan.access_commit).await?)
        } else {
            None
        };
        let command = ClusterNodeCommand::Program(argv);
        for node in &plan.nodes {
            self.dependencies
                .shell
                .run(&self.connection(plan, node), &command, bundle.clone())
                .await?;
        }
        Ok(())
    }

    /// The bundle of the access commit as `mmd access sync --bundle-stdin`
    /// reads it: the offer as one line of JSON, then lines of base64.
    async fn access_bundle(&self, commit: &str) -> Result<zeroize::Zeroizing<String>, MmError> {
        use base64::Engine;

        let bundles = self.bundles.as_ref().ok_or_else(|| MmError::State {
            message: "cluster setup for a push target was started without a bundle source"
                .to_owned(),
        })?;
        let prepared = bundles.bundle(commit).await?;
        let path = prepared.path.as_ref().ok_or_else(|| MmError::Environment {
            message: format!(
                "commit {commit} is not in this checkout, so setup cannot send the nodes its users file; fetch it (`git fetch origin {commit}`) or pass --commit"
            ),
        })?;
        let mut text = serde_json::to_string(&prepared.offer).map_err(|error| MmError::State {
            message: format!("could not write the bundle offer: {error}"),
        })?;
        text.push('\n');
        let bytes = std::fs::read(path).map_err(|source| MmError::Io {
            context: format!("reading the bundle {}", path.display()),
            source,
        })?;
        // 57 KiB is a multiple of three: every line is base64 of its own.
        for piece in bytes.chunks(57 * 1024) {
            text.push_str(&base64::engine::general_purpose::STANDARD.encode(piece));
            text.push('\n');
        }
        Ok(zeroize::Zeroizing::new(text))
    }
}
