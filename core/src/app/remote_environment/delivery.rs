//! Push-mode delivery across the nodes of a remote environment.
//!
//! The client sends the commit's bundle to each node itself, over the
//! session it already has with that node's `mmd`. No node passes it on to
//! another: a node only ever takes project files from a principal it
//! authorized, and the mesh carries no such trust. Every selected node
//! plans from the commit (also one that runs nothing, for the users file),
//! so every selected node gets the bundle, all at once. A node that holds
//! the commit already answers without an upload.

use std::sync::Arc;

use super::{RemoteEnvironment, RemoteNode, reporting};
use crate::app::ports::{CommitBundles, ProgressReporter, progress_step};
use crate::domain::delivery::{Delivered, PlanBundle};
use crate::domain::error::MmError;
use crate::domain::remote::RemotePlanBinding;

/// What builds the bundles of a target whose hosts fetch nothing.
#[derive(Clone)]
pub(super) struct PushedCommits(Arc<dyn CommitBundles>);

impl std::fmt::Debug for PushedCommits {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PushedCommits")
    }
}

/// What each selected node holds for the commit after a delivery.
pub(super) struct Deliveries {
    nodes: Vec<Delivered>,
    pub(super) plan: PlanBundle,
}

impl Deliveries {
    /// Every node has to have planned from the bundle it holds.
    pub(super) fn check(&self, bindings: &[&RemotePlanBinding]) -> Result<(), MmError> {
        for (delivered, binding) in self.nodes.iter().zip(bindings) {
            if binding.bundle_hash.as_deref() != Some(delivered.bundle_hash.as_str()) {
                let node = binding.node.as_deref().unwrap_or(&binding.target);
                return Err(MmError::Security {
                    message: format!(
                        "'{node}' did not plan commit {} from the bundle mm delivered ({}); nothing was planned. Its host policy may have stopped taking pushed files: compare `delivery` in /etc/mm/host.yaml with the target in mm.yaml",
                        binding.commit, delivered.bundle_hash
                    ),
                });
            }
        }
        Ok(())
    }
}

impl RemoteEnvironment {
    /// For a target with `delivery: push`: what builds a commit's bundle
    /// from the checkout mm runs in.
    #[must_use]
    pub fn with_pushed_commits(mut self, bundles: Option<Arc<dyn CommitBundles>>) -> Self {
        self.pushed = bundles.map(PushedCommits);
        self
    }

    /// Whether mm sends this target's hosts the project's files.
    #[must_use]
    pub const fn pushes(&self) -> bool {
        self.pushed.is_some()
    }

    /// On a push target: has every selected node hold `commit`, sending
    /// the bundle to the ones that do not. `None` on a target whose hosts
    /// fetch.
    pub(super) async fn deliver(
        &self,
        commit: &str,
        selected: &[&RemoteNode],
        progress: &dyn ProgressReporter,
    ) -> Result<Option<Deliveries>, MmError> {
        if self.pushed.is_none() {
            return Ok(None);
        }
        let short = crate::domain::rollout::short_id(commit);
        progress_step(
            progress,
            (
                reporting::DELIVER,
                &format!(
                    "Sending commit {short} to {}",
                    reporting::nodes(selected.len())
                ),
            ),
            self.deliver_unreported(commit, selected, progress),
            |delivered| {
                delivered.as_ref().map(|delivered| {
                    let plan = &delivered.plan;
                    if plan.sent_to.is_empty() {
                        format!("Every node holds commit {short}")
                    } else {
                        format!(
                            "Sent commit {short} ({} files, {}) to {}",
                            plan.files,
                            crate::domain::delivery::human_size(plan.bytes),
                            plan.sent_to.join(", ")
                        )
                    }
                })
            },
        )
        .await
    }

    async fn deliver_unreported(
        &self,
        commit: &str,
        selected: &[&RemoteNode],
        progress: &dyn ProgressReporter,
    ) -> Result<Option<Deliveries>, MmError> {
        let Some(PushedCommits(bundles)) = &self.pushed else {
            return Ok(None);
        };
        let prepared = bundles.bundle(commit).await?;
        let answers = self
            .ask_all(
                (reporting::DELIVER, "sending the commit's files"),
                selected,
                progress,
                |node| {
                    let (offer, path) = (prepared.offer.clone(), prepared.path.clone());
                    async move {
                        node.connect()?
                            .deliver(node.scope.clone(), offer, path)
                            .await
                    }
                },
            )
            .await;
        let nodes = self.all_or_explain(selected, answers)?;
        let first = nodes.first().ok_or_else(|| MmError::State {
            message: "no host answered the delivery".to_owned(),
        })?;
        let plan = PlanBundle {
            bundle_hash: first.bundle_hash.clone(),
            bytes: first.bytes,
            files: first.files,
            sent_to: selected
                .iter()
                .zip(&nodes)
                .filter(|(_, delivered)| delivered.received)
                .map(|(node, _)| self.name_of(node).to_owned())
                .collect(),
        };
        Ok(Some(Deliveries { nodes, plan }))
    }
}
