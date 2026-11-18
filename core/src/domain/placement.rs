//! Where a service runs on a cluster target.
//!
//! A placement is one word or a list of words. Each word is a node name, a
//! group from the target's `cluster.groups`, or `all` for every node. The
//! service runs one container on every node its words select; with no
//! placement it runs on the target's first node.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The placement word that selects every node of the target.
pub const ALL_NODES: &str = "all";

/// One placement word, or a list of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PlacementSpec {
    One(String),
    Several(Vec<String>),
}

impl PlacementSpec {
    #[must_use]
    pub fn words(&self) -> Vec<&str> {
        match self {
            Self::One(word) => vec![word.as_str()],
            Self::Several(words) => words.iter().map(String::as_str).collect(),
        }
    }

    /// The nodes the words select, in the target's declaration order.
    /// `nodes` are the target's node names in that order.
    pub fn resolve(
        &self,
        nodes: &[&str],
        groups: &BTreeMap<String, Vec<String>>,
    ) -> Result<Vec<String>, String> {
        let words = self.words();
        if words.is_empty() {
            return Err("lists no nodes; remove placement to run on the first node".to_owned());
        }
        let mut selected = BTreeSet::new();
        for word in words {
            if word == ALL_NODES {
                selected.extend(nodes.iter().copied());
            } else if let Some(members) = groups.get(word) {
                selected.extend(members.iter().map(String::as_str));
            } else if nodes.contains(&word) {
                selected.insert(word);
            } else {
                return Err(format!(
                    "'{word}' is not a node or group of the target; nodes: {}; groups: {}; or use `{ALL_NODES}`",
                    list(nodes.iter().copied()),
                    list(groups.keys().map(String::as_str)),
                ));
            }
        }
        Ok(nodes
            .iter()
            .filter(|node| selected.contains(**node))
            .map(|node| (*node).to_owned())
            .collect())
    }
}

/// A service that runs on a node, by stack and name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct PlacedService {
    pub stack: String,
    pub service: String,
}

/// The services of one environment that run on one node of a cluster target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeServices(BTreeSet<PlacedService>);

impl NodeServices {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn contains(&self, stack: &str, service: &str) -> bool {
        self.0
            .iter()
            .any(|placed| placed.stack == stack && placed.service == service)
    }

    /// Whether a service of this name runs here, in any stack.
    #[must_use]
    pub fn has_service(&self, service: &str) -> bool {
        self.0.iter().any(|placed| placed.service == service)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlacedService> {
        self.0.iter()
    }
}

/// The nodes and groups of a cluster target that placement resolves against.
#[derive(Debug, Clone, Copy)]
pub struct ClusterNodes<'a> {
    /// Node names in the target's declaration order.
    pub nodes: &'a [&'a str],
    pub groups: &'a BTreeMap<String, Vec<String>>,
}

/// Which of the environment's services run on `node`. This is the one place
/// that decides it: the daemon plans, reports, and serves only these on a
/// node, and the client sends logs and commands to a node that has the
/// service.
pub fn services_on_node(
    modules: &BTreeMap<String, crate::domain::config::ModuleConfig>,
    environment: &str,
    cluster: ClusterNodes<'_>,
    node: &str,
) -> Result<NodeServices, String> {
    if !cluster.nodes.contains(&node) {
        return Err(format!(
            "node '{node}' is not one of the target's nodes ({})",
            list(cluster.nodes.iter().copied())
        ));
    }
    let mut placed = BTreeSet::new();
    for (module_name, module) in modules {
        for (name, service) in &module.services {
            if !service.is_active(environment) {
                continue;
            }
            let nodes = service_nodes(service, cluster)
                .map_err(|problem| format!("service '{module_name}.{name}' placement {problem}"))?;
            if nodes.iter().any(|placed| placed == node) {
                placed.insert(PlacedService {
                    stack: service.stack.clone(),
                    service: name.clone(),
                });
            }
        }
    }
    Ok(NodeServices(placed))
}

/// The nodes one service runs on, in the target's declaration order: what
/// its `placement` selects, or the first node when it has none.
pub fn service_nodes(
    service: &crate::domain::config::ServiceConfig,
    cluster: ClusterNodes<'_>,
) -> Result<Vec<String>, String> {
    match &service.placement {
        Some(placement) => placement.resolve(cluster.nodes, cluster.groups),
        None => Ok(cluster
            .nodes
            .first()
            .map(|first| vec![(*first).to_owned()])
            .unwrap_or_default()),
    }
}

/// The node a job runs on, on a cluster target: the first node its
/// `placement` selects that runs the job's service, or else the first node
/// that runs the service. A job joins its service's networks, so it reaches
/// services on other nodes by name; it cannot run where its service does
/// not, because that node's Compose files do not have the service.
pub fn job_node(
    name: &str,
    job: &crate::domain::config::JobConfig,
    modules: &BTreeMap<String, crate::domain::config::ModuleConfig>,
    cluster: ClusterNodes<'_>,
) -> Result<String, String> {
    let service = modules
        .get(&job.module)
        .and_then(|module| module.services.get(&job.service))
        .ok_or_else(|| {
            format!(
                "job '{name}' runs as service '{}/{}', which is not declared",
                job.module, job.service
            )
        })?;
    let runs = service_nodes(service, cluster)
        .map_err(|problem| format!("service '{}' placement {problem}", job.service))?;
    let wanted = match &job.placement {
        None => runs.clone(),
        Some(word) => PlacementSpec::One(word.clone())
            .resolve(cluster.nodes, cluster.groups)
            .map_err(|problem| format!("jobs.{name}.placement {problem}"))?,
    };
    wanted
        .iter()
        .find(|node| runs.contains(node))
        .cloned()
        .ok_or_else(|| {
            format!(
                "jobs.{name}.placement selects {}, where service '{}' does not run; it runs on {}. Name one of those nodes, or leave placement out to take the first",
                wanted.join(", "),
                job.service,
                runs.join(", ")
            )
        })
}

/// The node job `name` runs on when `target` is a cluster target; `None`
/// for a target that names one host.
pub fn job_node_on(
    config: &crate::domain::config::MmConfig,
    target: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let Some(target) = config
        .targets
        .get(target)
        .filter(|target| target.is_cluster())
    else {
        return Ok(None);
    };
    let job = config
        .jobs
        .get(name)
        .ok_or_else(|| format!("job '{name}' is not declared"))?;
    let names = target
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    let cluster = ClusterNodes {
        nodes: &names,
        groups: &target.cluster.groups,
    };
    job_node(name, job, &config.modules, cluster).map(Some)
}

/// One node of a cluster target, for one environment: what a request to
/// that node's `mmd` is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeScope {
    pub target: String,
    pub node: String,
    pub environment: String,
    /// The directory on the node where its `mmd` keeps one resolver file
    /// per cluster network; `None` where no `mmd` answers names.
    pub resolver_files: Option<std::path::PathBuf>,
    /// The mesh address of every node the node's own mesh knows, by node
    /// name: where it mounts shared volumes from.
    pub mesh_addresses: BTreeMap<String, std::net::Ipv4Addr>,
}

/// Where the node of `scope` finds the shared volumes of `target`.
fn storage_access(
    target: &crate::domain::repo_target::RepoTargetConfig,
    scope: &NodeScope,
) -> crate::domain::shared_storage::StorageAccess {
    use crate::domain::shared_storage::{STORAGE_ROOT, StorageAccess, needs_mesh};
    let node = target.storage_node().unwrap_or_default().to_owned();
    if let crate::domain::repo_target::MeshConfig::Interface(interface) = &target.cluster.mesh {
        return StorageAccess::Unavailable {
            node,
            reason: needs_mesh(&scope.target, interface),
        };
    }
    if node == scope.node {
        return StorageAccess::Directory {
            node,
            root: std::path::PathBuf::from(STORAGE_ROOT),
        };
    }
    match scope.mesh_addresses.get(&node) {
        Some(address) => StorageAccess::Mesh {
            node,
            address: *address,
        },
        None => StorageAccess::Unavailable {
            reason: format!(
                "node '{}' does not know the mesh address of '{node}', which keeps the shared volumes of target '{}'; run `mm target setup plan` and apply it so every node holds the mesh",
                scope.node, scope.target
            ),
            node,
        },
    }
}

/// Turns a configuration into what one node runs of it.
///
/// Every service of the environment that is placed on other nodes is marked
/// with those nodes, which makes it inactive here: this node renders,
/// starts, and reports only the rest. A route follows its service: it stays
/// on the nodes that run the service and leaves the others.
pub fn view_from_node(
    config: &mut crate::domain::config::MmConfig,
    scope: &NodeScope,
) -> Result<(), String> {
    let target = config
        .targets
        .get(&scope.target)
        .filter(|target| target.is_cluster())
        .ok_or_else(|| {
            format!(
                "targets.{} does not list nodes, so nothing can be placed on node '{}'",
                scope.target, scope.node
            )
        })?;
    let names = target
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    if !names.contains(&scope.node.as_str()) {
        return Err(format!(
            "node '{}' is not one of the target's nodes ({})",
            scope.node,
            list(names.iter().copied())
        ));
    }
    let cluster = ClusterNodes {
        nodes: &names,
        groups: &target.cluster.groups,
    };
    let environment = scope.environment.as_str();
    let spans = target.spans_networks();
    let storage = storage_access(target, scope);
    let (by_stack, away) = mark_placement(&mut config.modules, cluster, environment, &scope.node)?;
    place_jobs(&mut config.jobs, &config.modules, cluster, &scope.node)?;
    // What stays on this node comes back with it after a reboot.
    for service in config
        .modules
        .values_mut()
        .flat_map(|module| module.services.values_mut())
    {
        service.restarts_with_node = service.is_active(environment);
    }
    // A stack with shared volumes learns where this node finds them.
    for stack in config.stacks.values_mut() {
        if !stack.shared_volumes.is_empty() {
            stack.storage = Some(storage.clone());
        }
    }
    if spans {
        let ingress_node = target
            .ingress_nodes()
            .map_err(|problem| format!("targets.{}.{problem}", scope.target))?
            .contains(&scope.node);
        span_networks(config, scope, &by_stack, ingress_node);
    }
    leave_routes_to_their_nodes(config, environment, &by_stack, &away)
}

/// The services each stack runs in an environment, on any node.
type StackServices = BTreeMap<String, Vec<String>>;

/// Marks every active service with where it runs: `placed_on` (its nodes)
/// when it runs on `node`, `placed_elsewhere` when it does not. Returns
/// what each stack runs on any node, and the services placed elsewhere.
fn mark_placement(
    modules: &mut BTreeMap<String, crate::domain::config::ModuleConfig>,
    cluster: ClusterNodes<'_>,
    environment: &str,
    node: &str,
) -> Result<(StackServices, BTreeSet<(String, String)>), String> {
    let mut placements = BTreeMap::new();
    // What each stack runs in this environment, on any node: a route that
    // names no service means the stack's only one.
    let mut by_stack = StackServices::new();
    let mut away = BTreeSet::new();
    for (module_name, module) in modules.iter() {
        for (name, service) in &module.services {
            if !service.is_active(environment) {
                continue;
            }
            by_stack
                .entry(service.stack.clone())
                .or_default()
                .push(name.clone());
            let nodes = service_nodes(service, cluster)
                .map_err(|problem| format!("service '{module_name}.{name}' placement {problem}"))?;
            if !nodes.iter().any(|placed| placed == node) {
                away.insert((service.stack.clone(), name.clone()));
            }
            placements.insert((module_name.clone(), name.clone()), nodes);
        }
    }
    for ((module, name), nodes) in placements {
        if let Some(service) = modules
            .get_mut(&module)
            .and_then(|module| module.services.get_mut(&name))
        {
            if nodes.iter().any(|placed| placed == node) {
                service.placed_on = Some(nodes);
            } else {
                service.placed_elsewhere = Some(nodes);
            }
        }
    }
    Ok((by_stack, away))
}

/// Where the networks span the nodes: the routes of every node come in
/// through the ingress nodes, which reach the routed services on a network
/// they share, and the node's own resolver answers the names of every
/// container on the networks.
fn span_networks(
    config: &mut crate::domain::config::MmConfig,
    scope: &NodeScope,
    by_stack: &BTreeMap<String, Vec<String>>,
    ingress_node: bool,
) {
    let environment = scope.environment.as_str();
    join_ingress_network(config, environment, by_stack, ingress_node);
    if let Some(directory) = scope.resolver_files.as_ref() {
        for (name, network) in &mut config.networks {
            let key =
                crate::domain::cluster_network::network_key(config.project.id, environment, name);
            network.resolver =
                Some(directory.join(crate::domain::discovery::resolver_file_name(&key)));
        }
    }
}

/// Whether `node` of the cluster target `target` runs the ingress proxy for
/// `environment`: it is one of the target's ingress nodes, and a route of
/// the environment is active.
#[must_use]
pub fn runs_ingress(
    config: &crate::domain::config::MmConfig,
    target: &str,
    node: &str,
    environment: &str,
) -> bool {
    let serves = config
        .targets
        .get(target)
        .and_then(|target| target.ingress_nodes().ok())
        .is_some_and(|nodes| nodes.iter().any(|name| name == node));
    serves
        && config.stacks.values().any(|stack| {
            stack
                .ingress
                .routes()
                .into_iter()
                .any(|route| crate::domain::config_resolve::route_is_active(route, environment))
        })
}

/// Puts every service an active route names on the network the ingress
/// proxy shares with them (declared or not), and makes the proxy a member
/// of it on an ingress node. Changes nothing when no route is active.
fn join_ingress_network(
    config: &mut crate::domain::config::MmConfig,
    environment: &str,
    by_stack: &BTreeMap<String, Vec<String>>,
    ingress_node: bool,
) {
    use crate::domain::cluster_ingress::INGRESS_NETWORK;
    let mut routed = BTreeSet::new();
    for (stack_name, stack) in &config.stacks {
        let services = by_stack.get(stack_name).map_or(&[][..], Vec::as_slice);
        for route in stack.ingress.routes() {
            if !crate::domain::config_resolve::route_is_active(route, environment) {
                continue;
            }
            match (&route.service, services) {
                (Some(service), _) | (None, [service]) => {
                    routed.insert((stack_name.clone(), service.clone()));
                }
                _ => {}
            }
        }
    }
    if routed.is_empty() {
        return;
    }
    config
        .networks
        .entry(INGRESS_NETWORK.to_owned())
        .or_default()
        .cluster_ingress = Some(ingress_node);
    for (name, service) in config
        .modules
        .values_mut()
        .flat_map(|module| module.services.iter_mut())
    {
        if routed.contains(&(service.stack.clone(), name.clone()))
            && !service
                .networks
                .iter()
                .any(|network| network == INGRESS_NETWORK)
        {
            service.networks.push(INGRESS_NETWORK.to_owned());
        }
    }
}

/// A job runs on one node, which needs its service; the other nodes leave
/// the job out, so its runs and schedules happen there only.
fn place_jobs(
    jobs: &mut BTreeMap<String, crate::domain::config::JobConfig>,
    modules: &BTreeMap<String, crate::domain::config::ModuleConfig>,
    cluster: ClusterNodes<'_>,
    here: &str,
) -> Result<(), String> {
    for (name, job) in jobs {
        let node = job_node(name, job, modules, cluster)?;
        job.placed_elsewhere = (node != here).then_some(node);
    }
    Ok(())
}

/// Drops the routes whose service runs on other nodes. A route that names
/// no service means the stack's only service; that is decided here, from
/// the whole stack, so that what is left of the stack on this node cannot
/// change which service the route means.
fn leave_routes_to_their_nodes(
    config: &mut crate::domain::config::MmConfig,
    environment: &str,
    by_stack: &BTreeMap<String, Vec<String>>,
    away: &BTreeSet<(String, String)>,
) -> Result<(), String> {
    for (stack_name, stack) in &mut config.stacks {
        let services = by_stack.get(stack_name).map_or(&[][..], Vec::as_slice);
        let before = stack
            .ingress
            .routes()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut routes = before.clone();
        for route in &mut routes {
            if route.service.is_some()
                || !crate::domain::config_resolve::route_is_active(route, environment)
            {
                continue;
            }
            match services {
                [only] => route.service = Some(only.clone()),
                [] => {}
                several => {
                    return Err(format!(
                        "the route for domain '{}' in stack '{stack_name}' names no service, and the stack has several ({}); set `service:` on the route",
                        route.domain,
                        several.join(", ")
                    ));
                }
            }
        }
        routes.retain(|route| {
            !crate::domain::config_resolve::route_is_active(route, environment)
                || route
                    .service
                    .as_ref()
                    .is_none_or(|service| !away.contains(&(stack_name.clone(), service.clone())))
        });
        if routes != before {
            stack.ingress = crate::domain::config::IngressConfig::Routes(routes);
        }
    }
    Ok(())
}

fn list<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let names = names.collect::<Vec<_>>();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

#[cfg(test)]
#[path = "placement/tests.rs"]
mod tests;
