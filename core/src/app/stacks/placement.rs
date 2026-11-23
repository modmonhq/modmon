//! A stack's Compose files as one node of a cluster target runs them.
//!
//! A node's project holds only the services placed on it. Taking a service
//! out of a Compose file leaves two kinds of loose ends, and both are tied
//! off here rather than left to the engine, because Docker Compose and
//! Podman Compose treat them differently:
//!
//! - a `depends_on` that names a service on another node (both refuse the
//!   project) is dropped and reported;
//! - a top-level network, volume, secret, or config that no remaining
//!   service uses (Podman Compose refuses an unused external network that
//!   does not exist) is taken out, so it exists only on the nodes that
//!   need it.

use serde_yaml_ng::{Mapping, Value};

use super::*;
use crate::domain::deployment::DroppedDependency;

/// What rendering a node's projects left out, and what the plan says about
/// the result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PlacementNotes {
    /// The Compose project name of every stack in the configuration,
    /// rendered on this node or not.
    pub projects: BTreeMap<String, String>,
    /// The services placed on other nodes, by stack and name, with the
    /// nodes that run them.
    pub elsewhere: BTreeMap<(String, String), Vec<String>>,
    pub dropped_dependencies: Vec<DroppedDependency>,
    /// The services, by stack and name, that get mm's default restart
    /// policy because their Compose files set none.
    pub restart_defaults: Vec<(String, String)>,
    /// The shared volumes the node's services mount.
    pub shared_volumes: Vec<crate::domain::shared_storage::SharedVolumeUse>,
    /// The config files the host's services mount. Filled on every host,
    /// not only on a cluster node.
    pub config_files: Vec<crate::domain::config_file::ConfigFileUse>,
    /// The user ids of the host's services that have ids of their own, by
    /// stack and name.
    pub user_ids: BTreeMap<(String, String), crate::domain::user_namespace::ServiceIds>,
    /// The node's services that set `replicas` or `autoscale`.
    pub scaled: Vec<ScaleNote>,
    /// The node's services that set `health:`, by stack and name.
    pub health: Vec<(String, String, crate::domain::healing::HealthConfig)>,
    /// The owned secret files and config files of the node's services
    /// that do not run in the host's user namespace, by key.
    pub outside_host_namespace: Vec<String>,
}

/// A service of the node that does not run one container on each of its
/// nodes, as its configuration says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScaleNote {
    pub stack: String,
    pub service: String,
    /// Its placement, in the target's order.
    pub nodes: Vec<String>,
    pub replicas: Option<u32>,
    pub autoscale: Option<crate::domain::autoscale::AutoscaleConfig>,
}

/// The restart policy a service of a cluster node gets when its Compose
/// files set none: it comes back when the node reboots, and stays stopped
/// when someone stopped it.
pub(super) const DEFAULT_RESTART: &str = "unless-stopped";

/// The services of `stack` that this node runs and whose Compose files set
/// no `restart:` policy. Empty anywhere but on a cluster node.
pub(super) fn services_without_restart(
    document: &ConfigDocument,
    stack: &str,
    sources: &[CompiledComposeSource],
) -> Result<BTreeSet<String>, MmError> {
    let mut candidates = document
        .config
        .modules
        .values()
        .flat_map(|module| &module.services)
        .filter(|(_, service)| service.stack == stack && service.restarts_with_node)
        .map(|(name, _)| name.clone())
        .collect::<BTreeSet<_>>();
    if candidates.is_empty() {
        return Ok(candidates);
    }
    let mut defined = BTreeSet::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in services {
            let Some(name) = name.as_str() else {
                continue;
            };
            defined.insert(name.to_owned());
            if service.get("restart").is_some() {
                candidates.remove(name);
            }
        }
    }
    candidates.retain(|name| defined.contains(name));
    Ok(candidates)
}

/// How one stack's services are placed, as the node's configuration says.
pub(super) struct StackPlacement<'a> {
    pub stack: &'a str,
    /// Services of the stack that run on other nodes, with those nodes.
    pub elsewhere: BTreeMap<String, Vec<String>>,
    pub notes: &'a mut PlacementNotes,
}

impl<'a> StackPlacement<'a> {
    pub(super) fn of(
        document: &ConfigDocument,
        stack: &'a str,
        notes: &'a mut PlacementNotes,
    ) -> Self {
        let mut elsewhere = BTreeMap::new();
        for (name, service) in document
            .config
            .modules
            .values()
            .flat_map(|module| &module.services)
            .filter(|(_, service)| service.stack == stack)
        {
            if let Some(nodes) = &service.placed_elsewhere {
                elsewhere.insert(name.clone(), nodes.clone());
                notes
                    .elsewhere
                    .insert((stack.to_owned(), name.clone()), nodes.clone());
            }
            if let (Some(_), Some(health)) = (&service.placed_on, &service.health) {
                notes
                    .health
                    .push((stack.to_owned(), name.clone(), health.clone()));
            }
            if let Some(nodes) = &service.placed_on
                && (service.replicas.is_some() || service.autoscale.is_some())
            {
                notes.scaled.push(ScaleNote {
                    stack: stack.to_owned(),
                    service: name.clone(),
                    nodes: nodes.clone(),
                    replicas: service.replicas,
                    autoscale: service.autoscale.clone(),
                });
            }
        }
        Self {
            stack,
            elsewhere,
            notes,
        }
    }

    /// The file without the services that run on other nodes. A file that
    /// loses nothing keeps its bytes.
    pub(super) fn place(&mut self, path: &Path, bytes: Vec<u8>) -> Result<Vec<u8>, MmError> {
        if self.elsewhere.is_empty() {
            return Ok(bytes);
        }
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        let Some(services) = model.get_mut("services").and_then(Value::as_mapping_mut) else {
            return Ok(bytes);
        };
        let mut changed = false;
        for name in self.elsewhere.keys() {
            changed |= services.remove(Value::String(name.clone())).is_some();
        }
        for (name, service) in services.iter_mut() {
            let (Some(name), Some(config)) = (name.as_str(), service.as_mapping_mut()) else {
                continue;
            };
            self.require_same_node(path, name, config)?;
            changed |= self.drop_dependencies(name, config);
        }
        if !changed {
            return Ok(bytes);
        }
        serde_yaml_ng::to_string(&model)
            .map(String::into_bytes)
            .map_err(invalid)
    }

    /// Takes the dependencies on services of other nodes out of
    /// `depends_on` and reports each one.
    fn drop_dependencies(&mut self, service: &str, config: &mut Mapping) -> bool {
        let key = Value::String("depends_on".to_owned());
        let mut dropped = Vec::new();
        let empty = match config.get_mut(&key) {
            Some(Value::Mapping(dependencies)) => {
                dependencies.retain(|name, _| {
                    let away = name
                        .as_str()
                        .filter(|name| self.elsewhere.contains_key(*name));
                    dropped.extend(away.map(str::to_owned));
                    away.is_none()
                });
                dependencies.is_empty()
            }
            Some(Value::Sequence(dependencies)) => {
                dependencies.retain(|name| {
                    let away = name
                        .as_str()
                        .filter(|name| self.elsewhere.contains_key(*name));
                    dropped.extend(away.map(str::to_owned));
                    away.is_none()
                });
                dependencies.is_empty()
            }
            _ => false,
        };
        if dropped.is_empty() {
            return false;
        }
        if empty {
            config.remove(&key);
        }
        for dependency in dropped {
            let note = DroppedDependency {
                stack: self.stack.to_owned(),
                service: service.to_owned(),
                nodes: self.elsewhere.get(&dependency).cloned().unwrap_or_default(),
                depends_on: dependency,
            };
            if !self.notes.dropped_dependencies.contains(&note) {
                self.notes.dropped_dependencies.push(note);
            }
        }
        true
    }

    /// Two services that share a namespace or a link live in one engine:
    /// refuse the plan when their placements differ.
    fn require_same_node(
        &self,
        path: &Path,
        service: &str,
        config: &Mapping,
    ) -> Result<(), MmError> {
        for field in ["links", "network_mode", "ipc", "pid"] {
            let Some(value) = config.get(Value::String(field.to_owned())) else {
                continue;
            };
            let references: Vec<&str> = match value {
                Value::Sequence(items) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.split(':').next().unwrap_or(item))
                    .collect(),
                Value::String(value) => value.strip_prefix("service:").into_iter().collect(),
                _ => Vec::new(),
            };
            if let Some((other, nodes)) = references
                .into_iter()
                .find_map(|name| self.elsewhere.get_key_value(name))
            {
                return Err(MmError::Placement {
                    message: format!(
                        "service '{service}' of stack '{}' uses `{field}` with service '{other}', which runs on {} and not on this node ({}). Services joined by `{field}` share one engine: give '{service}' and '{other}' the same `placement` in mm.yaml, or remove `{field}`",
                        self.stack,
                        nodes.join(", "),
                        path.display()
                    ),
                });
            }
        }
        Ok(())
    }
}

/// The top-level resource kinds a Compose file declares once for all its
/// services.
const RESOURCES: [&str; 4] = ["networks", "volumes", "secrets", "configs"];

/// Takes the top-level networks, volumes, secrets, and configs that none of
/// the stack's remaining services uses out of every file. `files` are the
/// stack's sources after services placed elsewhere were removed.
pub(super) fn prune_unused_resources(files: &mut [(PathBuf, Vec<u8>)]) -> Result<(), MmError> {
    let mut models = Vec::with_capacity(files.len());
    for (path, bytes) in files.iter() {
        let model: Value =
            serde_yaml_ng::from_slice(bytes).map_err(|cause| MmError::Environment {
                message: format!("Compose file '{}': {cause}", path.display()),
            })?;
        models.push(model);
    }
    let mut used = BTreeMap::<&str, BTreeSet<String>>::new();
    // A reference mm cannot read (a variable) keeps every resource of its kind.
    let mut unknown = BTreeSet::new();
    for model in &models {
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for service in services.values() {
            for kind in RESOURCES {
                for name in service_resources(service, kind) {
                    if name.contains('$') {
                        unknown.insert(kind);
                    }
                    used.entry(kind).or_default().insert(name);
                }
            }
        }
    }
    for ((path, bytes), mut model) in files.iter_mut().zip(models) {
        let mut pruned = false;
        for kind in RESOURCES {
            if unknown.contains(kind) {
                continue;
            }
            let Some(declared) = model.get_mut(kind).and_then(Value::as_mapping_mut) else {
                continue;
            };
            let before = declared.len();
            declared.retain(|name, _| {
                name.as_str().is_none_or(|name| {
                    (kind == "networks" && name == "default")
                        || used.get(kind).is_some_and(|used| used.contains(name))
                })
            });
            pruned |= declared.len() != before;
            if declared.is_empty()
                && let Some(root) = model.as_mapping_mut()
            {
                root.remove(Value::String(kind.to_owned()));
            }
        }
        if pruned {
            *bytes = serde_yaml_ng::to_string(&model)
                .map(String::into_bytes)
                .map_err(|cause| MmError::Environment {
                    message: format!("Compose file '{}': {cause}", path.display()),
                })?;
        }
    }
    Ok(())
}

/// The top-level resources of `kind` one service refers to.
pub(super) fn service_resources(service: &Value, kind: &str) -> Vec<String> {
    let Some(value) = service.get(kind) else {
        return Vec::new();
    };
    let entries: Vec<&Value> = match value {
        Value::Sequence(items) => items.iter().collect(),
        // `networks:` may map each network to its options.
        Value::Mapping(map) => {
            return map
                .keys()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
        }
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            Value::String(short) if kind == "volumes" => {
                let source = short
                    .split_once(':')
                    .map_or(short.as_str(), |(source, _)| source);
                let is_path = source.starts_with(['/', '.', '~']);
                (!is_path).then(|| source.to_owned())
            }
            Value::String(name) => Some(name.clone()),
            Value::Mapping(long) if kind == "volumes" => {
                let is_volume = long
                    .get("type")
                    .and_then(Value::as_str)
                    .is_none_or(|kind| kind == "volume");
                long.get("source")
                    .and_then(Value::as_str)
                    .filter(|_| is_volume)
                    .map(str::to_owned)
            }
            Value::Mapping(long) => long
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        })
        .collect()
}

/// On a cluster node the routes come in through the cluster's ingress
/// nodes, which reach the routed services on a cluster network; nothing
/// joins this host's own ingress network and no route is claimed here.
pub(super) fn served_by_cluster(document: &ConfigDocument) -> bool {
    document
        .config
        .networks
        .values()
        .any(|network| network.cluster_ingress.is_some())
}

/// On a node that runs the ingress proxy, the proxy is a member of the
/// network it shares with the routed services, whether or not one of them
/// runs there: the node gets its addresses and creates it.
pub(super) fn ingress_proxy_member(
    network: &crate::domain::config::NetworkConfig,
) -> Option<crate::domain::stack::CanonicalNetworkMember> {
    use crate::domain::cluster_ingress::PROXY_MEMBER;
    (network.cluster_ingress == Some(true)).then(|| crate::domain::stack::CanonicalNetworkMember {
        stack: PROXY_MEMBER.to_owned(),
        service: PROXY_MEMBER.to_owned(),
    })
}

#[cfg(test)]
mod tests;
