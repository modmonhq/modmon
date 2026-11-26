//! The order a cluster deployment rolls out in, from the nodes' plans and
//! from what each node's `mmd` says it knows.

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode};
use crate::domain::remote::NodeDeploymentPlan;
use crate::domain::rollout::{ClusterRollout, NodeRolloutInput, NodeSteps};

impl RemoteEnvironment {
    /// The rollout steps each node of `selected` knows, as its `mmd` says.
    /// A node that does not answer knows none of the newer ones.
    pub(super) async fn rollout_steps(&self, selected: &[&RemoteNode]) -> Vec<NodeSteps> {
        if !self.is_cluster() {
            return Vec::new();
        }
        join_all(selected.iter().map(|node| async move {
            let facts = match node.connect() {
                Ok(transport) => transport.node(node.scope.clone()).await.ok(),
                Err(_) => None,
            };
            NodeSteps {
                node: self.name_of(node).to_owned(),
                daemon_version: facts
                    .as_ref()
                    .map(|facts| facts.daemon_version.clone())
                    .unwrap_or_default(),
                steps: facts.map(|facts| facts.rollout_steps).unwrap_or_default(),
            }
        }))
        .await
    }
}

/// The stages of a deployment plan over `nodes`: what each node renders,
/// with its own policy, and the newer steps when every node knows them.
pub(super) fn cluster_rollout(nodes: &[NodeDeploymentPlan], steps: &[NodeSteps]) -> ClusterRollout {
    let rendered = nodes
        .iter()
        .filter_map(|node| {
            let plan = node.plan.plan.as_ref()?;
            let stacks = plan
                .stacks
                .iter()
                .map(|stack| stack.stack.clone())
                .collect::<Vec<_>>();
            let services = plan
                .node
                .as_ref()
                .map_or(&[][..], |node| &node.services[..]);
            Some((node.node.as_deref()?, stacks, &plan.rollout, services))
        })
        .collect::<Vec<_>>();
    let planned = steps
        .iter()
        .filter(|known| {
            nodes
                .iter()
                .any(|node| node.node.as_deref() == Some(known.node.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    ClusterRollout::of(
        &rendered
            .iter()
            .map(|(node, stacks, policy, services)| NodeRolloutInput {
                node,
                stacks,
                policy,
                services,
            })
            .collect::<Vec<_>>(),
    )
    .knowing_ran_once(nodes.iter().flat_map(|node| node.plan.once.clone()))
    .for_daemons(
        &planned,
        &nodes
            .iter()
            .filter_map(|node| node.plan.plan.as_ref())
            .flat_map(|plan| plan.stack_after.clone())
            .collect(),
    )
}
