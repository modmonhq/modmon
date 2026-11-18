//! The networks that span a cluster target, in the words of a plan.

use crate::domain::cluster_network::{ChangeSet, ClusterNetworkPlan, NodeNetworkPlan};

fn change_lines(title: &str, changes: &ChangeSet, lines: &mut Vec<String>) {
    if changes.is_empty() {
        return;
    }
    lines.push(format!("  {title}:"));
    lines.extend(changes.added.iter().map(|line| format!("    + {line}")));
    lines.extend(changes.removed.iter().map(|line| format!("    - {line}")));
}

/// The line under the plan's header that says where the addresses come
/// from.
pub(crate) fn network_header_line(plan: &ClusterNetworkPlan) -> String {
    format!(
        "  networks span the nodes: addresses from {}, between nodes through {} (MTU {})",
        plan.sealed.space, plan.sealed.mesh_interface, plan.sealed.mtu
    )
}

/// The lines of one node's section about its networks: the addresses each
/// network has there, a network that is created again and the services
/// that restart with it, and the routes and firewall rules that change.
pub(crate) fn node_network_lines(node: &NodeNetworkPlan) -> Vec<String> {
    let mut lines = Vec::new();
    if !node.networks.is_empty() {
        lines.push("  networks:".to_owned());
        lines.extend(
            node.networks
                .iter()
                .map(|network| format!("    {}", network.summary())),
        );
    }
    if !node.removed.is_empty() {
        lines.push(format!(
            "  networks no service uses any more, removed when the rollout finished here: {}",
            node.removed.join(", ")
        ));
    }
    change_lines("routes through the mesh", &node.routes, &mut lines);
    change_lines("firewall rules", &node.firewall, &mut lines);
    if !node.repairs.is_empty() {
        lines.push("  repairs what differs on the node from the last deployment:".to_owned());
        lines.extend(node.repairs.iter().map(|repair| format!("    {repair}")));
    }
    lines
}
