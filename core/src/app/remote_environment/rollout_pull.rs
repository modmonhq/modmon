//! Every node of a rollout pulls its images before any node is replaced,
//! all at the same time (`RolloutStep::Pull`).

use super::*;

impl RemoteEnvironment {
    /// Has every node of the plan pull the images it runs. No container
    /// changes; when a node fails, the lock is given up everywhere and
    /// nothing else was done.
    pub(super) async fn pull_all(
        &self,
        plan_hash: &str,
        nodes: &[&RemoteNode],
        rollout: &ClusterRollout,
        progress: NodeProgress<'_>,
    ) -> Result<(), MmError> {
        let patience = rollout.pull_timeout().saturating_add(SETUP_PATIENCE);
        let pulled = join_all(nodes.iter().map(|node| {
            self.with_heartbeat(
                Watched {
                    node,
                    step: "pulling images",
                    operation: plan_hash,
                },
                progress,
                step(node, plan_hash, RolloutStep::Pull, patience),
            )
        }))
        .await;
        let refused = nodes
            .iter()
            .zip(pulled)
            .find_map(|(node, pulled)| pulled.err().map(|error| (node, error)));
        let Some((node, error)) = refused else {
            return Ok(());
        };
        join_all(
            nodes
                .iter()
                .map(|node| step(node, plan_hash, RolloutStep::Release, LOCK_PATIENCE)),
        )
        .await;
        Err(in_node(node.name.as_deref(), error))
    }
}
