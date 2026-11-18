use super::*;
use crate::domain::autoscale::{MOST_PER_NODE, ScalePolicy, replicas};

/// `replicas` and `autoscale` on a service. They spread containers over
/// the nodes of a cluster target, so on a target of one host (`targets.<t>`
/// with `host:`) they are refused with what to write instead; a local
/// target (`mm dev`) ignores them and runs one container of each service.
/// Every other problem names the key to change.
pub(super) fn validate_scaling(context: &ValidationContext<'_>, report: &mut ValidationReport) {
    let document = context.document;
    let environment = context.environment;
    let cluster = crate::app::cluster_target(document, environment);
    let one_host = one_host_target(document, environment);
    for (module_name, module) in &document.config.modules {
        for (name, service) in &module.services {
            if (service.replicas.is_none() && service.autoscale.is_none())
                || !service.is_in_environment(environment)
                || service.is_job_only()
            {
                continue;
            }
            let entry = format!("modules.{module_name}.services.{name}");
            let key = if service.autoscale.is_some() {
                "autoscale"
            } else {
                "replicas"
            };
            let mut problems = Vec::new();
            match (cluster, one_host) {
                (Some((target_name, target)), _) => {
                    let nodes = target
                        .nodes
                        .iter()
                        .map(|node| node.name.as_str())
                        .collect::<Vec<_>>();
                    let cluster_nodes = crate::domain::placement::ClusterNodes {
                        nodes: &nodes,
                        groups: &target.cluster.groups,
                    };
                    // A placement that names no node is `validate_placement`'s.
                    let Ok(placed) = crate::domain::placement::service_nodes(service, cluster_nodes)
                    else {
                        continue;
                    };
                    problems.extend(range_problems(service, placed.len(), &entry));
                    if service.autoscale.is_some() {
                        problems.extend(exchange_problem(
                            document,
                            (target_name, target),
                            (module_name, name, service),
                        ));
                    }
                }
                (None, Some(target_name)) => problems.push(format!(
                    "{entry} sets `{key}`, which spreads containers over the nodes of a cluster target, and environment '{environment}' deploys to target '{target_name}', one host. Write targets.{target_name} as a cluster: `nodes: [{{name: <node>, host: <address>}}]` (one node is a cluster too), or remove `{key}`"
                )),
                (None, None) => {}
            }
            for problem in problems {
                report.push(
                    Diagnostic::error("MM_SCALE", problem).at_path(document.source("modules")),
                );
            }
        }
    }
}

/// The name of the environment's target when it is one remote host.
pub(super) fn one_host_target<'a>(
    document: &'a ConfigDocument,
    environment: &str,
) -> Option<&'a str> {
    let name = document
        .config
        .environments
        .get(environment)
        .and_then(|overlay| overlay.target.as_deref())
        .unwrap_or(document.config.target.as_str());
    document
        .config
        .targets
        .get_key_value(name)
        .filter(|(_, target)| !target.is_cluster())
        .map(|(name, _)| name.as_str())
}

/// `replicas` and `autoscale.scale` against each other and the nodes.
fn range_problems(
    service: &crate::domain::config::ServiceConfig,
    nodes: usize,
    entry: &str,
) -> Vec<String> {
    let mut problems = Vec::new();
    match replicas(service.replicas, nodes) {
        Ok(count) => {
            let per_node = count.div_ceil(u32::try_from(nodes).unwrap_or(u32::MAX).max(1));
            if per_node > MOST_PER_NODE {
                problems.push(format!(
                    "{entry}.replicas is {count}, which puts {per_node} containers on one of the service's {nodes} node(s); a node holds at most {MOST_PER_NODE} of one service. Lower `replicas` or place the service on more nodes"
                ));
            }
        }
        Err(problem) => problems.push(format!("{entry}.{} {}", problem.key, problem.message)),
    }
    if let Some(config) = &service.autoscale
        && let Err(found) = ScalePolicy::resolve(config, service.replicas, nodes)
    {
        problems.extend(
            found
                .into_iter()
                // A `replicas` of 0 is reported once, above.
                .filter(|problem| problem.key != "replicas" || service.replicas != Some(0))
                .map(|problem| format!("{entry}.{} {}", problem.key, problem.message)),
        );
    }
    problems
}

/// The nodes of an autoscaled service tell each other its load with the
/// records they exchange about a network that spans them, so the service
/// has to be on one.
fn exchange_problem(
    document: &ConfigDocument,
    (target_name, target): (&str, &crate::domain::repo_target::RepoTargetConfig),
    (module_name, name, service): (&str, &str, &crate::domain::config::ServiceConfig),
) -> Option<String> {
    let entry = format!("modules.{module_name}.services.{name}");
    if !target.spans_networks() {
        return Some(format!(
            "{entry} sets `autoscale`, and target '{target_name}' uses an existing interface (`cluster.mesh.interface`), over which mm's networks do not span the nodes, so the nodes cannot tell each other the service's load. Use mm's own mesh (remove `cluster.mesh.interface`), or remove `autoscale`"
        ));
    }
    let stack = document.config.stacks.get(&service.stack);
    let on_network = !service.networks.is_empty()
        || document
            .config
            .modules
            .get(module_name)
            .is_some_and(|module| !module.networks.is_empty())
        || stack.is_some_and(|stack| !stack.networks.is_empty())
        || stack.is_some_and(|stack| {
            stack
                .ingress
                .routes()
                .iter()
                .any(|route| route.service.as_deref().is_none_or(|routed| routed == name))
        });
    (!on_network).then(|| {
        format!(
            "{entry} sets `autoscale` and is on no network of mm.yaml, over which its nodes tell each other the service's load. Put it on one: `networks: [<name>]` on the service, with the network declared under `networks:`"
        )
    })
}
