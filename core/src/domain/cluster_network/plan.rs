//! The network part of a cluster deployment plan: which addresses each
//! network gets on each node, and what that changes on the node.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{NetworkNeed, NetworkTable, NodeNetworks};
use crate::domain::repo_target::Ipv4Cidr;

/// A network of the deployed configuration as the node's engine has it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ObservedNetwork {
    /// The addresses the engine's network has, such as `172.18.0.0/16`.
    pub addresses: String,
    /// Whether it is the bridge network the node's stored addresses call
    /// for; `false` for a network an earlier mm created host-local.
    pub current: bool,
}

/// One mm network that services on a node use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NetworkUse {
    /// The network's name in `mm.yaml`.
    pub name: String,
    /// Its name across the cluster.
    pub key: String,
    /// Its name on the node's engine.
    pub network: String,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub internal: bool,
    /// The services of the node that are on it.
    pub services: Vec<String>,
    /// The engine's network of that name, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed: Option<ObservedNetwork>,
}

/// What a deployment plan binds for the cluster's networks. Every node of
/// the plan keeps it with the plan's seal and applies exactly this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SealedNetworks {
    pub space: Ipv4Cidr,
    pub mesh_interface: String,
    pub mtu: u16,
    pub table: NetworkTable,
    /// The networks, by name across the cluster, that the plan takes out
    /// of the table: no service of the environment uses them any more.
    /// Each node keeps them until its part of the rollout finished, then
    /// drops them with their routes and firewall rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retired: Vec<String>,
}

impl SealedNetworks {
    /// The state `node` stores when it applies the plan.
    #[must_use]
    pub fn on_node(&self, node: &str) -> NodeNetworks {
        NodeNetworks {
            node: node.to_owned(),
            space: self.space,
            mesh_interface: self.mesh_interface.clone(),
            mtu: self.mtu,
            table: self.table.clone(),
        }
    }

    /// What node `node` stores when a rollout starts: the plan's table and,
    /// of the networks the plan retires, the entries `stored` holds, which
    /// containers on them use until the rollout finishes.
    #[must_use]
    pub fn on_node_keeping(&self, node: &str, stored: Option<&NodeNetworks>) -> NodeNetworks {
        let mut desired = self.on_node(node);
        for key in &self.retired {
            if let Some(entry) = stored.and_then(|stored| stored.table.networks.get(key)) {
                desired.table.networks.insert(key.clone(), entry.clone());
            }
        }
        desired
    }

    /// Refuses what a node must not apply.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(key) = self
            .retired
            .iter()
            .find(|key| self.table.networks.contains_key(key.as_str()))
        {
            return Err(format!("the network plan both keeps and retires {key}"));
        }
        super::space_problem(self.space)?;
        crate::domain::target::validate_target_name(&self.mesh_interface)
            .map_err(|_| format!("'{}' is not an interface name", self.mesh_interface))?;
        if self.mesh_interface.len() > 15 || !(1280..=9000).contains(&self.mtu) {
            return Err("the mesh interface or MTU of the network plan is out of range".to_owned());
        }
        self.table.validate()
    }
}

/// Lines a plan adds and removes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ChangeSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

impl ChangeSet {
    fn between(before: &[String], after: &[String]) -> Self {
        Self {
            added: after
                .iter()
                .filter(|line| !before.contains(line))
                .cloned()
                .collect(),
            removed: before
                .iter()
                .filter(|line| !after.contains(line))
                .cloned()
                .collect(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// One network on one node in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct PlannedNetwork {
    pub name: String,
    pub network: String,
    pub block: Ipv4Cidr,
    pub slice: Ipv4Cidr,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub internal: bool,
    /// The plan gives the node this slice; `false` when it had it already.
    pub assigned: bool,
    /// The addresses of the engine's network when the plan has to remove
    /// it and create it again on the slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    /// The services whose containers restart because the network is
    /// created again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restarts: Vec<String>,
}

impl PlannedNetwork {
    /// What the plan does to the network on the node, in words.
    #[must_use]
    pub fn summary(&self) -> String {
        let internal = if self.internal { ", internal" } else { "" };
        let addresses = format!("{} of {}{internal}", self.slice, self.block);
        match (&self.replaces, self.assigned) {
            (Some(previous), _) => {
                let restarts = if self.restarts.is_empty() {
                    String::new()
                } else {
                    format!("; restarts {}", self.restarts.join(", "))
                };
                format!(
                    "{}: created again on {addresses} (was {previous}){restarts}",
                    self.name
                )
            }
            (None, true) => format!("{}: gets {addresses}", self.name),
            (None, false) => format!("{}: keeps {addresses}", self.name),
        }
    }
}

/// The network changes on one node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeNetworkPlan {
    pub node: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<PlannedNetwork>,
    /// Slices routed through the mesh to other nodes.
    #[serde(default, skip_serializing_if = "ChangeSet::is_empty")]
    pub routes: ChangeSet,
    /// Rules of mm's firewall chains.
    #[serde(default, skip_serializing_if = "ChangeSet::is_empty")]
    pub firewall: ChangeSet,
    /// What the node has that differs from what it stored, which the apply
    /// puts right.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repairs: Vec<String>,
    /// The networks of `mm.yaml` the node holds a part of that no service
    /// of the environment uses any more: taken away when the rollout
    /// finished on the node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

impl NodeNetworkPlan {
    /// Whether applying changes anything about the node's networks.
    #[must_use]
    pub fn changes(&self) -> bool {
        !self.routes.is_empty()
            || !self.firewall.is_empty()
            || !self.repairs.is_empty()
            || !self.removed.is_empty()
            || self
                .networks
                .iter()
                .any(|network| network.assigned || network.replaces.is_some())
    }
}

/// The network section of a cluster deployment plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ClusterNetworkPlan {
    /// The addresses after the plan; what the plan's hash covers.
    pub sealed: SealedNetworks,
    pub nodes: Vec<NodeNetworkPlan>,
}

impl ClusterNetworkPlan {
    #[must_use]
    pub fn changes(&self) -> bool {
        self.nodes.iter().any(NodeNetworkPlan::changes)
    }
}

/// One node of the plan: what it stored, what its services use, and what
/// its kernel and engine have that differs from the stored state.
#[derive(Debug, Clone, Default)]
pub struct PlannedNode {
    pub node: String,
    pub stored: Option<NodeNetworks>,
    pub uses: Vec<NetworkUse>,
    pub drift: Vec<String>,
}

/// Plans the cluster's networks. `known` is every table read from a node of
/// the target, planned or not, with the node it came from; `nodes` are the
/// nodes of the plan in order. Returns `None` when no node uses or holds a
/// cluster network.
pub fn plan_networks(
    settings: &SealedNetworks,
    known: &[(String, NetworkTable)],
    nodes: &[PlannedNode],
) -> Result<Option<ClusterNetworkPlan>, String> {
    plan_networks_retiring(settings, known, nodes, &|_| None)
}

/// [`plan_networks`], also taking out of the table every network of the
/// environment no planned node uses: `ours` gives the name in `mm.yaml` of
/// a network that is the environment's ([`super::environment_network`]).
/// One that a node outside the plan holds a part of stays.
pub fn plan_networks_retiring(
    settings: &SealedNetworks,
    known: &[(String, NetworkTable)],
    nodes: &[PlannedNode],
    ours: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<ClusterNetworkPlan>, String> {
    let mut table = NetworkTable::merge(
        known
            .iter()
            .map(|(holder, table)| (holder.as_str(), table)),
    )
    .map_err(|message| {
        format!("{message}. Two deployments gave out addresses without seeing each other. On one of the two nodes, remove that network with the engine and its entry in /var/lib/mm/networks/networks.json, then plan again")
    })?;
    if table.is_empty() && nodes.iter().all(|node| node.uses.is_empty()) {
        return Ok(None);
    }
    // A network the plan uses that has addresses in the sizes of an
    // earlier mm gets new ones: on every node of the plan it is created
    // again. Its old block stays out of reach of new networks, because the
    // engines hold it until then.
    let used = nodes
        .iter()
        .flat_map(|node| &node.uses)
        .map(|used| used.key.as_str())
        .collect::<Vec<_>>();
    let mut retired = Vec::new();
    table.networks.retain(|name, network| {
        let outdated = !network.is_current() && used.contains(&name.as_str());
        if outdated {
            retired.push(network.block);
        }
        !outdated
    });
    // A network of the environment that no service of it uses any more
    // leaves the table. Its block stays out of reach of new networks in
    // this plan: the engines hold it until the rollout finished.
    let planned = nodes
        .iter()
        .map(|node| node.node.as_str())
        .collect::<Vec<_>>();
    let mut removed = Vec::new();
    table.networks.retain(|key, network| {
        let unused = !used.contains(&key.as_str())
            && network
                .nodes
                .keys()
                .all(|holder| planned.contains(&holder.as_str()));
        match ours(key).filter(|_| unused) {
            Some(name) => {
                retired.push(network.block);
                removed.push((
                    key.clone(),
                    name,
                    network.nodes.keys().cloned().collect::<Vec<_>>(),
                ));
                false
            }
            None => true,
        }
    });
    let before = table.clone();
    for node in nodes {
        for used in &node.uses {
            table.assign_around(
                settings.space,
                &NetworkNeed {
                    key: &used.key,
                    node: &node.node,
                    network: &used.network,
                    internal: used.internal,
                },
                &retired,
            )?;
        }
    }
    table.validate_sizes(&used)?;
    let sealed = SealedNetworks {
        table,
        retired: removed.iter().map(|(key, ..)| key.clone()).collect(),
        ..settings.clone()
    };
    sealed.validate()?;
    let nodes = nodes
        .iter()
        .map(|node| {
            let mut plan = node_plan(&sealed, &before, node);
            plan.removed = removed
                .iter()
                .filter(|(_, _, holders)| holders.contains(&node.node))
                .map(|(_, name, _)| name.clone())
                .collect();
            plan
        })
        .collect();
    Ok(Some(ClusterNetworkPlan { sealed, nodes }))
}

/// What the plan changes on one node, against what the node stored.
fn node_plan(
    sealed: &SealedNetworks,
    before: &NetworkTable,
    node: &PlannedNode,
) -> NodeNetworkPlan {
    let after = sealed.on_node(&node.node);
    let (firewall, routes) = match &node.stored {
        Some(stored) => (
            stored.firewall().lines().cloned().collect::<Vec<_>>(),
            stored.route_lines(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let networks = node
        .uses
        .iter()
        .filter_map(|used| {
            let local = after.local_network(&used.key)?;
            let assigned = !before
                .networks
                .get(&used.key)
                .is_some_and(|network| network.nodes.contains_key(&node.node));
            // A network that is not the stored bridge network, or whose
            // slice is new, cannot be kept.
            let replaces = used
                .observed
                .as_ref()
                .filter(|observed| assigned || !observed.current)
                .map(|observed| observed.addresses.clone());
            Some(PlannedNetwork {
                name: used.name.clone(),
                network: used.network.clone(),
                block: local.block,
                slice: local.slice,
                internal: local.internal,
                assigned,
                restarts: if replaces.is_some() {
                    used.services.clone()
                } else {
                    Vec::new()
                },
                replaces,
            })
        })
        .collect();
    NodeNetworkPlan {
        node: node.node.clone(),
        networks,
        routes: ChangeSet::between(&routes, &after.route_lines()),
        firewall: ChangeSet::between(
            &firewall,
            &after.firewall().lines().cloned().collect::<Vec<_>>(),
        ),
        repairs: node.drift.clone(),
        removed: Vec::new(),
    }
}
