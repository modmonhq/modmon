//! Networks that span the nodes of a cluster target.
//!
//! Every mm network gets one block of the cluster's address space, and every
//! node that runs a service on the network gets one slice of that block. On
//! a node the network is an ordinary bridge network whose subnet is the
//! block and whose containers get addresses from the node's slice. A
//! container therefore sees every member of the network as a neighbour on
//! that network's own interface, whichever of its networks carries its
//! default route; the node answers for the members on other nodes and the
//! mesh routes each slice to its node.
//!
//! An address, once given, never changes. Each node stores the table it
//! last applied; the client merges the tables of all nodes before it plans,
//! so two projects on the same hosts never get the same addresses.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::repo_target::Ipv4Cidr;

#[path = "cluster_network/address_table.rs"]
mod address_table;
#[path = "cluster_network/hook.rs"]
mod hook;
#[path = "cluster_network/host.rs"]
mod host;
#[path = "cluster_network/plan.rs"]
mod plan;
#[path = "cluster_network/rules.rs"]
mod rules;
#[path = "cluster_network/status.rs"]
mod status;

pub use address_table::{AddressTable, AddressTableNetwork, NodeHostRanges, NodeSlice};
pub use hook::{ForwardHook, forward_hook};
pub use host::{HostRange, HostRangeKind, range_conflict};
pub use plan::{
    ChangeSet, ClusterNetworkPlan, NetworkUse, NodeNetworkPlan, ObservedNetwork, PlannedNetwork,
    PlannedNode, SealedNetworks, plan_networks, plan_networks_retiring,
};
pub use rules::{
    BRIDGE_PREFIX, FILTER_CHAIN, Firewall, LocalNetwork, NAT_CHAIN, NodeNetworks, bridge_name,
    with_routes,
};
pub use status::{
    NetworkDrift, NetworkDriftKind, NetworkReport, NodeNetworkReport, NodeNetworkStatus,
    network_diagnostics,
};

/// Prefix length of the block one network gets: room for sixteen nodes.
pub const NETWORK_BLOCK_PREFIX: u8 = 20;
/// Prefix length of the slice one node gets of a network's block: 256
/// addresses. The node's bridge takes one and the slice's first and last
/// are kept back ([`LocalNetwork::reserved`]), so a node has room for 253
/// containers.
pub const NETWORK_SLICE_PREFIX: u8 = 24;

fn is_false(value: &bool) -> bool {
    !*value
}

/// One node's part of a network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeAddresses {
    /// The node's slice of the network's block.
    pub slice: Ipv4Cidr,
    /// The network's name on that node's engine. Each node names its
    /// networks after its own identity, so the names differ between nodes.
    pub network: String,
}

/// The addresses of one network: its block and each node's slice of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NetworkAddresses {
    pub block: Ipv4Cidr,
    /// Containers on the network reach its members and nothing else.
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal: bool,
    /// Each node's part, by node name.
    pub nodes: BTreeMap<String, NodeAddresses>,
}

impl NetworkAddresses {
    /// Whether the network has a block and slices of the sizes mm gives out
    /// now. An earlier mm gave out smaller ones; a node keeps those working
    /// until a deployment that uses the network gives it new addresses.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.block.prefix == NETWORK_BLOCK_PREFIX
            && self
                .nodes
                .values()
                .all(|held| held.slice.prefix == NETWORK_SLICE_PREFIX)
    }
}

/// The first 12 hex digits of a project's id, which its objects on a host are named after.
#[must_use]
pub fn project_key(project_id: crate::domain::workspace::ProjectId) -> String {
    project_id
        .0
        .as_simple()
        .to_string()
        .chars()
        .take(12)
        .collect()
}

/// The name a network has on every node of the cluster: its project, its
/// environment, and its name in `mm.yaml`.
#[must_use]
pub fn network_key(
    project_id: crate::domain::workspace::ProjectId,
    environment: &str,
    name: &str,
) -> String {
    format!("{}-{environment}-{name}", project_key(project_id))
}

/// The name in `mm.yaml` of the network `key` names, when it is one of
/// `environment`'s, of the project's `environments`. Keys join the
/// project, the environment and the name with `-`, which names may
/// contain: a key that a longer environment of the project starts, such as
/// `staging-eu`'s for `staging`, is that environment's.
#[must_use]
pub fn environment_network(
    key: &str,
    project_id: crate::domain::workspace::ProjectId,
    environment: &str,
    environments: &[String],
) -> Option<String> {
    let name = key.strip_prefix(&network_key(project_id, environment, ""))?;
    let longer = environments.iter().any(|other| {
        other.len() > environment.len()
            && other.starts_with(environment)
            && key.starts_with(&network_key(project_id, other, ""))
    });
    (!name.is_empty() && !longer).then(|| name.to_owned())
}

/// Who holds which addresses, by [`network_key`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NetworkTable {
    #[serde(default)]
    pub networks: BTreeMap<String, NetworkAddresses>,
}

/// A network one node needs addresses on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkNeed<'a> {
    /// The network's [`network_key`].
    pub key: &'a str,
    pub node: &'a str,
    /// The network's name on the node's engine.
    pub network: &'a str,
    pub internal: bool,
}

/// The `index`th sub-network of `parent` with prefix length `prefix`.
fn subnet(parent: Ipv4Cidr, prefix: u8, index: u32) -> Option<Ipv4Cidr> {
    let bits = u32::from(prefix.checked_sub(parent.prefix)?);
    if prefix > 32 || bits >= 32 || index >= (1_u32 << bits) {
        return None;
    }
    let step = 1_u32.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
    Some(Ipv4Cidr {
        address: Ipv4Addr::from(u32::from(parent.address).checked_add(index.checked_mul(step)?)?),
        prefix,
    })
}

fn subnets(parent: Ipv4Cidr, prefix: u8) -> impl Iterator<Item = Ipv4Cidr> {
    (0_u32..).map_while(move |index| subnet(parent, prefix, index))
}

/// Names end up in rule comments and command lines: letters, digits, and
/// `-`, `_`, `.` only.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn within(inner: Ipv4Cidr, outer: Ipv4Cidr) -> bool {
    inner.prefix >= outer.prefix && outer.contains(inner.address)
}

/// Where the no-room message says to set `cluster.networks`. The table
/// does not know the target's name; whoever plans for a target puts
/// `under targets.<name>.cluster` in its place, as the message about an
/// address range a node already uses says it.
pub const UNDER_THE_TARGET: &str = "under the target";

impl NetworkTable {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.networks.is_empty()
    }

    /// Every slice routed to `node`.
    #[must_use]
    pub fn slices_of(&self, node: &str) -> Vec<Ipv4Cidr> {
        let mut slices = self
            .networks
            .values()
            .filter_map(|network| network.nodes.get(node).map(|held| held.slice))
            .collect::<Vec<_>>();
        slices.sort();
        slices
    }

    /// Refuses a table mm must not apply: a slice outside its network's
    /// block, or addresses held twice. The sizes are not checked here, so
    /// a table an earlier mm wrote still reads; [`Self::outdated`] names
    /// its networks.
    pub fn validate(&self) -> Result<(), String> {
        let mut blocks = Vec::<(&str, Ipv4Cidr)>::new();
        for (name, network) in &self.networks {
            let names = std::iter::once(name)
                .chain(network.nodes.keys())
                .chain(network.nodes.values().map(|held| &held.network));
            for written in names {
                if !is_plain_name(written) {
                    return Err(format!(
                        "'{written}' is not a name a network table can hold"
                    ));
                }
            }
            if let Some((other, block)) = blocks
                .iter()
                .find(|(_, block)| block.overlaps(network.block))
            {
                return Err(format!("networks '{other}' and '{name}' both hold {block}"));
            }
            blocks.push((name, network.block));
            let mut slices = Vec::<Ipv4Cidr>::new();
            for (node, held) in &network.nodes {
                let slice = held.slice;
                if slice.prefix > 30 || !within(slice, network.block) {
                    return Err(format!(
                        "network '{name}' gives node '{node}' {slice}, which is not a part of its block {}",
                        network.block
                    ));
                }
                if slices.iter().any(|other| other.overlaps(slice)) {
                    return Err(format!("network '{name}' gives {slice} to two nodes"));
                }
                slices.push(slice);
            }
        }
        Ok(())
    }

    /// The networks whose addresses an earlier mm gave out in other sizes,
    /// by name.
    pub fn outdated(&self) -> impl Iterator<Item = (&String, &NetworkAddresses)> {
        self.networks
            .iter()
            .filter(|(_, network)| !network.is_current())
    }

    /// Refuses a table with addresses in other sizes than mm gives out:
    /// what a plan gives a network must be a block and slices of today's.
    pub fn validate_sizes(&self, names: &[&str]) -> Result<(), String> {
        match self
            .outdated()
            .find(|(name, _)| names.contains(&name.as_str()))
        {
            Some((name, network)) => Err(format!(
                "network '{name}' has the block {}, which is not a /{NETWORK_BLOCK_PREFIX} with a /{NETWORK_SLICE_PREFIX} per node",
                network.block
            )),
            None => Ok(()),
        }
    }

    /// One table out of what every node holds. `tables` names the node each
    /// table was read from, for the message when two nodes disagree.
    pub fn merge<'a>(
        tables: impl IntoIterator<Item = (&'a str, &'a NetworkTable)>,
    ) -> Result<Self, String> {
        let mut merged = Self::default();
        let mut origin = BTreeMap::<String, &str>::new();
        for (holder, table) in tables {
            for (name, network) in &table.networks {
                let Some(known) = merged.networks.get_mut(name) else {
                    merged.networks.insert(name.clone(), network.clone());
                    origin.insert(name.clone(), holder);
                    continue;
                };
                let first = origin.get(name).copied().unwrap_or_default();
                // A node that was left out when the network got addresses
                // of today's sizes still holds the earlier ones.
                if known.is_current() != network.is_current() {
                    if network.is_current() {
                        *known = network.clone();
                        origin.insert(name.clone(), holder);
                    }
                    continue;
                }
                if known.block != network.block {
                    return Err(format!(
                        "nodes '{first}' and '{holder}' disagree about network '{name}': {} and {}",
                        known.block, network.block
                    ));
                }
                for (node, addresses) in &network.nodes {
                    match known.nodes.get(node) {
                        Some(held) if held.slice != addresses.slice => {
                            return Err(format!(
                                "nodes '{first}' and '{holder}' disagree about the addresses of node '{node}' on network '{name}': {} and {}",
                                held.slice, addresses.slice
                            ));
                        }
                        Some(_) => {}
                        None => {
                            known.nodes.insert(node.clone(), addresses.clone());
                        }
                    }
                }
            }
        }
        merged.validate().map_err(|message| {
            format!("the nodes hold network addresses that collide: {message}")
        })?;
        Ok(merged)
    }

    /// Whether every address in `self` is in `other` unchanged. A network
    /// with addresses of an earlier mm's sizes may get new ones.
    #[must_use]
    pub fn is_kept_by(&self, other: &Self) -> bool {
        self.networks.iter().all(|(name, network)| {
            other.networks.get(name).is_some_and(|kept| {
                if !network.is_current() && kept.is_current() {
                    return true;
                }
                kept.block == network.block
                    && network.nodes.iter().all(|(node, held)| {
                        kept.nodes
                            .get(node)
                            .is_some_and(|kept| kept.slice == held.slice)
                    })
            })
        })
    }

    /// Gives `need` its addresses: the ones it already has, else the lowest
    /// free slice of the network's block, and for a new network the lowest
    /// free block of `space`. Returns whether the slice is new.
    pub fn assign(&mut self, space: Ipv4Cidr, need: &NetworkNeed<'_>) -> Result<bool, String> {
        self.assign_around(space, need, &[])
    }

    /// [`Self::assign`], with a new network's block also kept clear of
    /// `taken`: ranges no entry of the table holds any more that an engine
    /// still has a network on.
    pub fn assign_around(
        &mut self,
        space: Ipv4Cidr,
        need: &NetworkNeed<'_>,
        taken: &[Ipv4Cidr],
    ) -> Result<bool, String> {
        if !self.networks.contains_key(need.key) {
            let block = self.free_block(space, taken).ok_or_else(|| {
                let room = networks_in(space);
                format!(
                    "cluster.networks {space} has no room for network '{}': it holds {room} network(s) and all are taken. Set `cluster.networks` {UNDER_THE_TARGET} to another private range for new networks, such as {}; networks that already have addresses keep them",
                    need.key,
                    suggestion(space)
                )
            })?;
            self.networks.insert(
                need.key.to_owned(),
                NetworkAddresses {
                    block,
                    internal: need.internal,
                    nodes: BTreeMap::new(),
                },
            );
        }
        let Some(network) = self.networks.get_mut(need.key) else {
            return Ok(false);
        };
        network.internal = need.internal;
        if let Some(held) = network.nodes.get_mut(need.node) {
            need.network.clone_into(&mut held.network);
            return Ok(false);
        }
        let taken = network
            .nodes
            .values()
            .map(|held| held.slice)
            .collect::<Vec<_>>();
        let slice = subnets(network.block, NETWORK_SLICE_PREFIX)
            .find(|slice| !taken.contains(slice))
            .ok_or_else(|| {
                format!(
                    "network '{}' already spans {} nodes, the most one network can; node '{}' cannot join it. Place the service on a node the network already reaches, or use a second network",
                    need.key,
                    taken.len(),
                    need.node
                )
            })?;
        network.nodes.insert(
            need.node.to_owned(),
            NodeAddresses {
                slice,
                network: need.network.to_owned(),
            },
        );
        Ok(true)
    }

    fn free_block(&self, space: Ipv4Cidr, taken: &[Ipv4Cidr]) -> Option<Ipv4Cidr> {
        subnets(space, NETWORK_BLOCK_PREFIX).find(|block| {
            !self
                .networks
                .values()
                .map(|network| network.block)
                .chain(taken.iter().copied())
                .any(|held| held.overlaps(*block))
        })
    }
}

/// How many networks `space` holds.
fn networks_in(space: Ipv4Cidr) -> u32 {
    NETWORK_BLOCK_PREFIX
        .checked_sub(space.prefix)
        .and_then(|bits| 1_u32.checked_shl(u32::from(bits)))
        .unwrap_or(0)
}

/// Another private range to offer when `space` is full.
fn suggestion(space: Ipv4Cidr) -> Ipv4Cidr {
    let default = crate::domain::repo_target::DEFAULT_CLUSTER_NETWORKS;
    if default.overlaps(space) {
        Ipv4Cidr {
            address: Ipv4Addr::new(10, 32, 0, 0),
            prefix: 12,
        }
    } else {
        default
    }
}

/// Why `space` cannot hold cluster networks, when it cannot.
pub fn space_problem(space: Ipv4Cidr) -> Result<(), String> {
    if space.prefix > NETWORK_BLOCK_PREFIX {
        let default = crate::domain::repo_target::DEFAULT_CLUSTER_NETWORKS;
        return Err(format!(
            "cluster.networks {space} is too small to hold a network: every network takes a /{NETWORK_BLOCK_PREFIX} (16 nodes, each with room for 253 containers). Use a /{NETWORK_BLOCK_PREFIX} or larger: a /16 holds {} networks, and the default {default} holds {}",
            networks_in(Ipv4Cidr {
                address: space.address,
                prefix: 16
            }),
            networks_in(default)
        ));
    }
    Ok(())
}
