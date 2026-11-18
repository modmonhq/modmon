use serde_yaml_ng::Value;

use super::*;

const MAX_COMPOSE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// On a cluster target mm decides which node runs each service, so every
/// Compose service needs an entry under `modules`, and the Compose keys
/// that asked Docker Swarm to place or multiply a service are refused with
/// what to write in mm.yaml instead. Elsewhere there is one host and
/// Compose reads these keys itself.
pub(super) fn validate_cluster_compose(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let environment = context.environment;
    let Some((target_name, target)) = crate::app::cluster_target(document, environment) else {
        return;
    };
    let first = target
        .nodes
        .first()
        .map(|node| node.name.as_str())
        .unwrap_or_default();
    for (stack_name, stack) in &document.config.stacks {
        for file in &stack.files {
            // Missing and malformed files are reported by the stack checks.
            let path = document.root.join(file);
            let Ok(Some(bytes)) = read_regular_file(&path, MAX_COMPOSE_FILE_BYTES) else {
                continue;
            };
            let Ok(mut model) = serde_yaml_ng::from_slice::<Value>(&bytes) else {
                continue;
            };
            if model.apply_merge().is_err() {
                continue;
            }
            let Some(services) = model.get("services").and_then(Value::as_mapping) else {
                continue;
            };
            for (name, service) in services {
                let Some(name) = name.as_str() else {
                    continue;
                };
                let declared = document.config.modules.iter().find_map(|(module, config)| {
                    config
                        .services
                        .get(name)
                        .filter(|service| &service.stack == stack_name)
                        .map(|service| (module.as_str(), service))
                });
                let Some((module, config)) = declared else {
                    report.push(
                        Diagnostic::error(
                            "MM_PLACEMENT",
                            format!(
                                "Compose service '{name}' of stack '{stack_name}' has no entry under modules, so mm cannot tell which node of target '{target_name}' runs it. Add `modules.<module>.services.{name}: {{stack: {stack_name}}}` to mm.yaml; without `placement` it runs on the first node ({first})"
                            ),
                        )
                        .at_path(&path),
                    );
                    continue;
                };
                if !config.is_in_environment(environment) {
                    continue;
                }
                let subject = SwarmSubject {
                    service: name,
                    entry: format!("modules.{module}.services.{name}"),
                    target: target_name,
                };
                for problem in swarm_only_keys(&subject, service) {
                    report.push(Diagnostic::error("MM_COMPOSE_DEPLOY_KEY", problem).at_path(&path));
                }
                for problem in scaled_keys(&subject, config, target, service) {
                    report.push(Diagnostic::error("MM_SCALE", problem).at_path(&path));
                }
                let on_network = !stack.networks.is_empty()
                    || !config.networks.is_empty()
                    || document
                        .config
                        .modules
                        .get(module)
                        .is_some_and(|module| !module.networks.is_empty());
                if target.spans_networks() && on_network {
                    for problem in resolver_keys(&subject, service) {
                        report.push(Diagnostic::error("MM_COMPOSE_DNS", problem).at_path(&path));
                    }
                }
            }
        }
    }
}

struct SwarmSubject<'a> {
    service: &'a str,
    /// The service's entry in mm.yaml, where `placement` goes.
    entry: String,
    target: &'a str,
}

/// What to write instead of each Docker Swarm key a service sets.
fn swarm_only_keys(subject: &SwarmSubject<'_>, service: &Value) -> Vec<String> {
    let SwarmSubject {
        service: name,
        entry,
        target,
    } = subject;
    let Some(deploy) = service.get("deploy").and_then(Value::as_mapping) else {
        return Vec::new();
    };
    let mut problems = Vec::new();
    if let Some(mode) = deploy.get("mode").and_then(Value::as_str) {
        problems.push(if mode == "global" {
            format!(
                "service '{name}' sets `deploy.mode: global`, which only Docker Swarm reads. To run it on every node, write `placement: all` under {entry} in mm.yaml and remove `deploy.mode`"
            )
        } else {
            format!(
                "service '{name}' sets `deploy.mode: {mode}`, which only Docker Swarm reads. Remove it: mm runs one container of a service on each node its `placement` under {entry} names"
            )
        });
    }
    if deploy.get("placement").is_some() {
        problems.push(format!(
            "service '{name}' sets `deploy.placement`, which only Docker Swarm reads. Write `placement:` under {entry} in mm.yaml instead: a node name, a group from targets.{target}.cluster.groups, `all`, or a list of those; then remove `deploy.placement`"
        ));
    }
    if deploy.get("endpoint_mode").is_some() {
        problems.push(format!(
            "service '{name}' sets `deploy.endpoint_mode`, which chooses between Docker Swarm's virtual IP and DNS round robin. mm has no virtual IPs; remove `deploy.endpoint_mode`"
        ));
    }
    // A count that is not a number is left for Compose to refuse.
    let replicas = deploy.get("replicas").and_then(|replicas| match replicas {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => Some(text.trim().parse::<u64>().unwrap_or_default()),
        _ => None,
    });
    if let Some(replicas) = replicas.filter(|replicas| *replicas > 1) {
        problems.push(format!(
            "service '{name}' sets `deploy.replicas: {replicas}`. On a cluster target mm spreads a service's containers over its nodes: write `replicas: {replicas}` under {entry} in mm.yaml (with `placement:` for the nodes, `all` for every node) and remove `deploy.replicas`"
        ));
    }
    problems
}

/// What a service whose containers mm spreads over its nodes cannot set
/// in Compose: a fixed `container_name` (autoscale), and, where a node can
/// run more than one of them, a fixed `container_name`, a host port that it
/// publishes, or a count of its own (`scale`).
fn scaled_keys(
    subject: &SwarmSubject<'_>,
    config: &crate::domain::config::ServiceConfig,
    target: &crate::domain::repo_target::RepoTargetConfig,
    service: &Value,
) -> Vec<String> {
    let SwarmSubject {
        service: name,
        entry,
        ..
    } = subject;
    if config.replicas.is_none() && config.autoscale.is_none() {
        return Vec::new();
    }
    let nodes = target
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    let placed = crate::domain::placement::service_nodes(
        config,
        crate::domain::placement::ClusterNodes {
            nodes: &nodes,
            groups: &target.cluster.groups,
        },
    )
    .map_or(1, |placed| {
        u32::try_from(placed.len()).unwrap_or(u32::MAX).max(1)
    });
    // The most containers the service runs over its nodes.
    let most = config.autoscale.as_ref().map_or_else(
        || config.replicas.unwrap_or(placed),
        |autoscale| autoscale.scale.max,
    );
    let several = most > placed;
    let key = if config.autoscale.is_some() {
        "autoscale.scale.max"
    } else {
        "replicas"
    };
    let mut problems = Vec::new();
    if service.get("container_name").is_some() && (several || config.autoscale.is_some()) {
        problems.push(format!(
            "service '{name}' sets `container_name`, and {entry} in mm.yaml gives it more than one container; the engine names each one itself. Remove `container_name`"
        ));
    }
    if several && publishes_host_port(service) {
        problems.push(format!(
            "service '{name}' publishes a host port, and {entry}.{key} ({most}) is more than the {placed} node(s) it is placed on, so two of its containers would want the same port on one node. Reach it through `ingress:` or by name instead of the host port, or lower `{key}` to {placed}"
        ));
    }
    if service.get("scale").is_some() {
        problems.push(format!(
            "service '{name}' sets `scale`, and {entry} in mm.yaml sets how many containers it runs. Remove `scale`"
        ));
    }
    problems
}

/// Whether a Compose service publishes a port on a fixed host port.
fn publishes_host_port(service: &Value) -> bool {
    let Some(ports) = service.get("ports").and_then(Value::as_sequence) else {
        return false;
    };
    ports.iter().any(|port| match port {
        // `8080:80`, `127.0.0.1:8080:80`; `80` alone takes any host port.
        Value::String(text) => text.contains(':'),
        Value::Number(_) => false,
        Value::Mapping(long) => long.get("published").is_some(),
        _ => false,
    })
}

/// The Compose keys that choose a container's resolver. On a network that
/// spans the nodes the node's own resolver answers the container, so the
/// keys would be ignored without a word.
fn resolver_keys(subject: &SwarmSubject<'_>, service: &Value) -> Vec<String> {
    ["dns", "dns_search", "dns_opt"]
        .into_iter()
        .filter(|key| service.get(*key).is_some())
        .map(|key| {
            format!(
                "service '{}' sets `{key}`, which has no effect on target '{}': the service is on a network that spans the nodes, where each node answers its containers' lookups itself (service names on every node, and every other name through the resolvers and search domains in the host's /etc/resolv.conf). Remove `{key}`; to resolve other names differently, change /etc/resolv.conf on the nodes",
                subject.service, subject.target
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "cluster_compose/tests.rs"]
mod tests;
