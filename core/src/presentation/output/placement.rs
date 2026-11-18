//! Where services run on a cluster target, in the words of a plan.

use crate::domain::deployment::NodeProjectPlan;
use crate::domain::placement::PlacedService;
use crate::domain::remote::ServiceMove;

fn names(services: &[PlacedService]) -> String {
    if services.is_empty() {
        return "none".to_owned();
    }
    services
        .iter()
        .map(|service| format!("{}/{}", service.stack, service.service))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The lines of one node's section that say what it runs after the plan,
/// what starts and stops there, and where its project differs from the
/// Compose files because a service lives on another node.
pub(crate) fn node_project_lines(node: &NodeProjectPlan) -> Vec<String> {
    let mut lines = vec![format!("  services: {}", names(&node.services))];
    if !node.starting.is_empty() {
        lines.push(format!("  starts here: {}", names(&node.starting)));
    }
    for leaving in &node.leaving {
        let name = format!("{}/{}", leaving.stack, leaving.service);
        lines.push(if leaving.moved() {
            format!(
                "  stops here: {name}, after it runs on {} (last step of the apply)",
                leaving.moves_to.join(", ")
            )
        } else {
            format!("  removed here: {name} (no node runs it any more)")
        });
    }
    for dropped in &node.dropped_dependencies {
        lines.push(format!(
            "  depends_on not kept: {}/{} depends on {}, which runs on {}. An engine cannot wait for a service on another node, so {} starts without waiting for it",
            dropped.stack,
            dropped.service,
            dropped.depends_on,
            dropped.nodes.join(", "),
            dropped.service
        ));
    }
    for shared in &node.shared_volumes {
        let name = format!("{}/{}", shared.stack, shared.volume);
        let services = shared.services.join(", ");
        lines.push(if shared.local {
            format!(
                "  shared volume {name}: this node keeps it; {services} use{} the directory itself",
                if shared.services.len() == 1 { "s" } else { "" }
            )
        } else {
            format!(
                "  shared volume {name}: mounted from {} over the mesh for {services}",
                shared.storage_node
            )
        });
    }
    lines.extend(
        node.scaled
            .iter()
            .map(|scaled| scaled_line(&node.node, scaled)),
    );
    lines.extend(node.health.iter().map(|health| {
        format!(
            "  health {}/{}: {}",
            health.stack,
            health.service,
            health.restart.map_or_else(
                || "not restarted for its health check (on-failure: none)".to_owned(),
                |policy| policy.describe()
            )
        )
    }));
    if !node.restart_defaults.is_empty() {
        lines.push(format!(
            "  restart: unless-stopped added to {} (no `restart:` in the Compose file), so {} back when the node reboots; set `restart: \"no\"` on a service to leave it stopped",
            names(&node.restart_defaults),
            if node.restart_defaults.len() == 1 {
                "it comes"
            } else {
                "they come"
            }
        ));
    }
    lines
}

/// How many containers a scaled service runs, over its nodes and here.
fn scaled_line(node: &str, scaled: &crate::domain::autoscale::ScaledService) -> String {
    let name = format!("{}/{}", scaled.stack, scaled.service);
    let nodes = scaled.nodes.len();
    match scaled.policy() {
        Some(policy) => {
            let now = scaled.now.unwrap_or(policy.initial);
            format!(
                "  autoscale {name}: {}..{} containers over {nodes} node(s), {now} now ({} here); {} above {} adds one, below {} removes one",
                policy.min,
                policy.max,
                scaled.share(node, Some(now)),
                policy.metric.as_str(),
                policy.up.threshold,
                policy.down.threshold,
            )
        }
        None => format!(
            "  replicas {name}: {} containers over {nodes} node(s), {} here",
            scaled.replicas,
            scaled.share(node, None)
        ),
    }
}

/// The plan's moves, one line each, in the order they happen.
pub(crate) fn move_lines(moves: &[ServiceMove]) -> Vec<String> {
    moves
        .iter()
        .map(|moved| format!("    {}", moved.describe()))
        .collect()
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
