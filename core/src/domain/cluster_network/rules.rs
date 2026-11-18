//! What one node holds for the cluster's networks: its bridges, the routes
//! to the other nodes' slices, and the firewall rules that let two
//! containers talk only when they share a network.
//!
//! The rules are the same for both engines; where they enter the host's
//! packet filter is not ([`super::forward_hook`]). Docker Engine drops
//! whatever arrives for a container from another host; mm's chain accepts
//! exactly the traffic between the slices of one network over the mesh,
//! and drops everything else that addresses the cluster's networks.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::NetworkTable;
use crate::domain::mesh::MeshState;
use crate::domain::repo_target::Ipv4Cidr;

/// mm's chain for forwarded traffic, entered where the engine lets the
/// host's own rules in ([`super::forward_hook`]).
pub const FILTER_CHAIN: &str = "MM-CLUSTER";
/// mm's chain in the nat table, entered from `POSTROUTING`.
pub const NAT_CHAIN: &str = "MM-CLUSTER-NAT";

/// What the name of every cluster network's bridge interface starts with.
pub const BRIDGE_PREFIX: &str = "mmn-";

/// The bridge interface of a network on every node, from the network's
/// name across the cluster: a fixed name, so the rules that name it can be
/// loaded before the engine creates it.
#[must_use]
pub fn bridge_name(key: &str) -> String {
    let digest = blake3::hash(key.as_bytes()).to_hex();
    format!(
        "{BRIDGE_PREFIX}{}",
        digest.chars().take(11).collect::<String>()
    )
}

/// One cluster network as this node holds it, which is also what the
/// engine is asked to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalNetwork {
    /// The network's name across the cluster.
    pub key: String,
    /// The network's name on this node's engine.
    pub network: String,
    pub bridge: String,
    /// The network's whole block: the subnet of the bridge network.
    pub block: Ipv4Cidr,
    /// This node's slice: the range containers get addresses from.
    pub slice: Ipv4Cidr,
    /// The node's own address on the bridge.
    pub gateway: Ipv4Addr,
    pub internal: bool,
    pub mtu: u16,
    /// The interface the other nodes' slices are reached through.
    pub mesh_interface: String,
}

impl LocalNetwork {
    /// The addresses of the slice that no container gets, by name: its
    /// first and its last, which read like a network's own address to
    /// people and to some software. With the node's own address that
    /// leaves 253 for containers. The block's own first and last address
    /// are not listed: the engine keeps those back by itself.
    #[must_use]
    pub fn reserved(&self) -> Vec<(&'static str, Ipv4Addr)> {
        let mut reserved = Vec::new();
        if self.slice.address != self.block.address {
            reserved.push(("first", self.slice.address));
        }
        if self.slice.last_address() != self.block.last_address() {
            reserved.push(("last", self.slice.last_address()));
        }
        reserved
    }
}

/// Everything a node stores about the cluster's networks. The firewall
/// rules, the routes, and the bridges all follow from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeNetworks {
    /// This node's name.
    pub node: String,
    /// The address space new networks come from.
    pub space: Ipv4Cidr,
    /// The interface the nodes reach each other on.
    pub mesh_interface: String,
    pub mtu: u16,
    pub table: NetworkTable,
}

/// The rules of mm's two chains, one rule per line as `iptables -S` prints
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Firewall {
    pub filter: Vec<String>,
    pub nat: Vec<String>,
}

impl Firewall {
    /// Every rule, filter chain first.
    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.filter.iter().chain(&self.nat)
    }

    /// Input for `iptables-restore --noflush`: both chains are created when
    /// missing and replaced as a whole, and nothing else is touched. The
    /// forward chain lives in the table `hook` names.
    #[must_use]
    pub fn restore_input(&self, hook: super::ForwardHook) -> String {
        let mut input = format!("*{}\n:{FILTER_CHAIN} - [0:0]\n", hook.table);
        for rule in &self.filter {
            input.push_str(rule);
            input.push('\n');
        }
        input.push_str(&format!("COMMIT\n*nat\n:{NAT_CHAIN} - [0:0]\n"));
        for rule in &self.nat {
            input.push_str(rule);
            input.push('\n');
        }
        input.push_str("COMMIT\n");
        input
    }
}

impl NodeNetworks {
    /// The networks this node has a slice of, by name.
    #[must_use]
    pub fn local(&self) -> Vec<LocalNetwork> {
        self.table
            .networks
            .iter()
            .filter_map(|(key, network)| {
                let held = network.nodes.get(&self.node)?;
                let slice = held.slice;
                Some(LocalNetwork {
                    key: key.clone(),
                    network: held.network.clone(),
                    bridge: bridge_name(key),
                    block: network.block,
                    slice,
                    gateway: Ipv4Addr::from(u32::from(slice.address).saturating_add(1)),
                    internal: network.internal,
                    mtu: self.mtu,
                    mesh_interface: self.mesh_interface.clone(),
                })
            })
            .collect()
    }

    /// One network of this node, by its name across the cluster.
    #[must_use]
    pub fn local_network(&self, key: &str) -> Option<LocalNetwork> {
        self.local().into_iter().find(|local| local.key == key)
    }

    /// The network `key` as this node holds it when a plan may keep it as
    /// it is. A network in the sizes of an earlier mm is not one to keep:
    /// every plan that uses it gives it new addresses, so its engine
    /// network counts as one to create again, before and after the node
    /// stored the plan's addresses.
    #[must_use]
    pub fn kept_network(&self, key: &str) -> Option<LocalNetwork> {
        self.table
            .networks
            .get(key)
            .filter(|network| network.is_current())?;
        self.local_network(key)
    }

    /// The address ranges cluster networks live in: the configured space,
    /// and each block given out of an earlier one.
    #[must_use]
    pub fn spaces(&self) -> Vec<Ipv4Cidr> {
        let mut spaces = vec![self.space];
        for network in self.table.networks.values() {
            if !spaces.iter().any(|space| space.overlaps(network.block)) {
                spaces.push(network.block);
            }
        }
        spaces
    }

    /// The slices routed to each other node, by node name.
    #[must_use]
    pub fn routed(&self) -> BTreeMap<String, Vec<Ipv4Cidr>> {
        let mut routed = BTreeMap::<String, Vec<Ipv4Cidr>>::new();
        for network in self.table.networks.values() {
            for (node, held) in &network.nodes {
                if *node != self.node {
                    routed.entry(node.clone()).or_default().push(held.slice);
                }
            }
        }
        for slices in routed.values_mut() {
            slices.sort();
        }
        routed
    }

    /// The routes as a plan shows them: `10.64.1.0/24 to app-2`.
    #[must_use]
    pub fn route_lines(&self) -> Vec<String> {
        self.routed()
            .iter()
            .flat_map(|(node, slices)| slices.iter().map(move |slice| format!("{slice} to {node}")))
            .collect()
    }

    /// The firewall rules of this node.
    #[must_use]
    pub fn firewall(&self) -> Firewall {
        let mesh = self.mesh_interface.as_str();
        let rule = |chain: &str, matches: String, comment: &str, verdict: &str| {
            format!("-A {chain}{matches} -m comment --comment \"{comment}\" -j {verdict}")
        };
        let mut filter = vec![
            rule(
                FILTER_CHAIN,
                " -m conntrack --ctstate RELATED,ESTABLISHED".to_owned(),
                "mm replies",
                "RETURN",
            ),
            // A published port arrives translated; the engine's own rules
            // decide about it.
            rule(
                FILTER_CHAIN,
                " -m conntrack --ctstate DNAT".to_owned(),
                "mm published ports",
                "RETURN",
            ),
        ];
        let mut nat = Vec::new();
        for local in self.local() {
            let (block, bridge, name) = (local.block, local.bridge.as_str(), local.key.as_str());
            let comment = format!("mm network {name}");
            filter.push(rule(
                FILTER_CHAIN,
                format!(" -s {block} -d {block} -i {mesh} -o {bridge}"),
                &comment,
                "ACCEPT",
            ));
            filter.push(rule(
                FILTER_CHAIN,
                format!(" -s {block} -d {block} -i {bridge} -o {mesh}"),
                &comment,
                "ACCEPT",
            ));
            // Two containers of the network on this node: where the kernel
            // filters bridged traffic too, the engine's own rules decide,
            // as for any network of the engine.
            filter.push(rule(
                FILTER_CHAIN,
                format!(" -i {bridge} -o {bridge}"),
                &comment,
                "RETURN",
            ));
            if local.internal {
                filter.push(rule(
                    FILTER_CHAIN,
                    format!(" -i {bridge}"),
                    &format!("mm internal network {name}"),
                    "DROP",
                ));
                continue;
            }
            // Traffic inside the network keeps the container's address;
            // everything else leaves with the node's.
            let slice = local.slice;
            nat.push(rule(
                NAT_CHAIN,
                format!(" -s {slice} -d {block}"),
                &comment,
                "RETURN",
            ));
            nat.push(rule(
                NAT_CHAIN,
                format!(" -s {slice} -o {mesh}"),
                &comment,
                "RETURN",
            ));
            nat.push(rule(
                NAT_CHAIN,
                format!(" -s {slice}"),
                &comment,
                "MASQUERADE",
            ));
        }
        for space in self.spaces() {
            filter.push(rule(
                FILTER_CHAIN,
                format!(" -d {space}"),
                "mm cluster networks",
                "DROP",
            ));
            filter.push(rule(
                FILTER_CHAIN,
                format!(" -s {space} -o {mesh}"),
                "mm cluster networks",
                "DROP",
            ));
        }
        Firewall { filter, nat }
    }
}

/// `state` with every slice the table routes to a peer added to that peer's
/// allowed addresses. A prefix that this node or another peer already holds
/// is left out, so a stale table can never take an address away.
#[must_use]
pub fn with_routes(state: &MeshState, networks: Option<&NodeNetworks>) -> MeshState {
    let mut effective = state.clone();
    let Some(networks) = networks else {
        return effective;
    };
    let routed = networks.routed();
    let mut held = effective
        .peers
        .iter()
        .flat_map(|peer| peer.allowed_ips.iter().copied())
        .collect::<Vec<_>>();
    for peer in &mut effective.peers {
        for prefix in routed.get(&peer.name).into_iter().flatten() {
            let clashes =
                prefix.contains(state.address) || held.iter().any(|other| other.overlaps(*prefix));
            if !clashes {
                peer.allowed_ips.push(*prefix);
                held.push(*prefix);
            }
        }
    }
    effective
}
