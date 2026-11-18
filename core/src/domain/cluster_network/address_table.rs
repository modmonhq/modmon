//! The cluster's addresses as a person reads them: the block each network
//! of an environment has, each node's slice of it, and the addresses and
//! routes every node already uses, which the space of the cluster's
//! networks and the mesh subnet must stay clear of.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{HostRange, NetworkAddresses, NetworkTable, NodeNetworkReport};
use crate::domain::repo_target::Ipv4Cidr;

/// One node's slice of a network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeSlice {
    pub node: String,
    /// The range the node's containers on the network get addresses from.
    pub slice: Ipv4Cidr,
    /// The network's name on that node's engine.
    pub network: String,
}

/// One network of the environment and who holds which part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AddressTableNetwork {
    /// The network's name in `mm.yaml`.
    pub name: String,
    /// Its name across the cluster.
    pub key: String,
    /// The network's block of the cluster's address space.
    pub block: Ipv4Cidr,
    /// Containers on the network reach its members and nothing else.
    pub internal: bool,
    /// The nodes that hold a slice: the target's nodes in declared order,
    /// then nodes that are no longer nodes of the target.
    pub nodes: Vec<NodeSlice>,
}

impl AddressTableNetwork {
    /// The slice `node` holds of the network, when it holds one.
    #[must_use]
    pub fn slice_of(&self, node: &str) -> Option<Ipv4Cidr> {
        self.nodes
            .iter()
            .find(|held| held.node == node)
            .map(|held| held.slice)
    }
}

/// The addresses and routes one node has outside mm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeHostRanges {
    pub node: String,
    /// Whether the node's `mmd` answered. When it did not, `ranges` is
    /// empty because they are not known.
    pub answered: bool,
    /// Why the node did not answer, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub ranges: Vec<HostRange>,
}

/// The addresses of a cluster target for one environment, from what every
/// node stored at its last deployment. Empty for a target whose networks
/// stay local to each host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AddressTable {
    /// The address space new networks get their block from:
    /// `cluster.networks` of the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<Ipv4Cidr>,
    /// The environment's networks that have addresses, by name.
    pub networks: Vec<AddressTableNetwork>,
    /// The blocks that networks of other projects and environments hold on
    /// the same nodes. A new network never gets one of them.
    pub held_by_others: Vec<Ipv4Cidr>,
    /// One entry per node of the target, in declared order.
    pub host_ranges: Vec<NodeHostRanges>,
    /// Where two nodes hold different addresses for the same network, or
    /// the same addresses for two networks, in words. A deployment plan is
    /// refused until the nodes agree. A network the nodes disagree about is
    /// listed as the first node that holds it has it.
    pub differing: Vec<String>,
}

impl AddressTable {
    /// The table for the networks `ours` gives a name in `mm.yaml` to (the
    /// environment's, `super::environment_network`), from every node's
    /// answer in declared order. The nodes' tables are merged the way a
    /// plan merges them; where that merge refuses, the table says so
    /// instead.
    #[must_use]
    pub fn from_reports(
        space: Option<Ipv4Cidr>,
        ours: &dyn Fn(&str) -> Option<String>,
        nodes: &[NodeNetworkReport],
    ) -> Self {
        let held = nodes
            .iter()
            .filter_map(|report| {
                let stored = report.status.as_ref()?.stored.as_ref()?;
                Some((report.name.as_str(), &stored.table))
            })
            .collect::<Vec<_>>();
        let keys = held
            .iter()
            .flat_map(|&(_, table)| table.networks.keys())
            .collect::<BTreeSet<_>>();
        let mut merged = NetworkTable::default();
        let mut differing = Vec::new();
        for key in keys {
            if let Some((network, disagreement)) = merged_network(key, &held) {
                merged.networks.insert(key.clone(), network);
                differing.extend(disagreement);
            }
        }
        // Two networks on the same addresses: each node's table is fine
        // on its own and no single network differs.
        if let Err(message) = merged.validate() {
            let message = format!("the nodes hold network addresses that collide: {message}");
            if !differing.contains(&message) {
                differing.push(message);
            }
        }
        let declared = nodes
            .iter()
            .map(|report| report.name.as_str())
            .collect::<Vec<_>>();
        let mut networks = Vec::new();
        let mut held_by_others = Vec::new();
        for (key, network) in &merged.networks {
            match ours(key) {
                Some(name) => networks.push(AddressTableNetwork {
                    name,
                    key: key.clone(),
                    block: network.block,
                    internal: network.internal,
                    nodes: slices(network, &declared),
                }),
                None => held_by_others.push(network.block),
            }
        }
        held_by_others.sort();
        Self {
            space,
            networks,
            held_by_others,
            host_ranges: nodes.iter().map(host_ranges).collect(),
            differing,
        }
    }

    /// Whether there is nothing to show: the target's networks stay local
    /// to each host.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.space.is_none()
            && self.networks.is_empty()
            && self.held_by_others.is_empty()
            && self.host_ranges.is_empty()
            && self.differing.is_empty()
    }

    /// The nodes of the table, one column each: the target's nodes in
    /// declared order, then nodes that hold a slice and are no longer
    /// nodes of the target.
    #[must_use]
    pub fn node_names(&self) -> Vec<&str> {
        let mut names = self
            .host_ranges
            .iter()
            .map(|held| held.node.as_str())
            .collect::<Vec<_>>();
        let others = self
            .networks
            .iter()
            .flat_map(|network| &network.nodes)
            .map(|held| held.node.as_str())
            .collect::<BTreeSet<_>>();
        for node in others {
            if !names.contains(&node) {
                names.push(node);
            }
        }
        names
    }
}

/// One network out of what every node holds for it, merged as a plan
/// merges whole tables. When that merge refuses, the network as the first
/// node that holds it has it, with the refusal.
fn merged_network(
    key: &str,
    held: &[(&str, &NetworkTable)],
) -> Option<(NetworkAddresses, Option<String>)> {
    let alone = held
        .iter()
        .filter_map(|(holder, table)| {
            let network = table.networks.get(key)?;
            Some((
                *holder,
                NetworkTable {
                    networks: BTreeMap::from([(key.to_owned(), network.clone())]),
                },
            ))
        })
        .collect::<Vec<_>>();
    let first = alone.first()?.1.networks.get(key)?.clone();
    match NetworkTable::merge(alone.iter().map(|(holder, table)| (*holder, table))) {
        Ok(mut table) => Some((table.networks.remove(key)?, None)),
        Err(message) => Some((first, Some(message))),
    }
}

/// The slices of `network`, the nodes in `declared` first and in that
/// order.
fn slices(network: &NetworkAddresses, declared: &[&str]) -> Vec<NodeSlice> {
    let mut slices = network
        .nodes
        .iter()
        .map(|(node, held)| NodeSlice {
            node: node.clone(),
            slice: held.slice,
            network: held.network.clone(),
        })
        .collect::<Vec<_>>();
    slices.sort_by_key(|held| {
        declared
            .iter()
            .position(|name| *name == held.node)
            .unwrap_or(declared.len())
    });
    slices
}

fn host_ranges(report: &NodeNetworkReport) -> NodeHostRanges {
    NodeHostRanges {
        node: report.name.clone(),
        answered: report.status.is_some(),
        problem: report.problem.clone(),
        ranges: report
            .status
            .as_ref()
            .map(|status| status.host_ranges.clone())
            .unwrap_or_default(),
    }
}
