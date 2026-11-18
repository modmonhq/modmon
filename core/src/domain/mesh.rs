//! The private WireGuard mesh between the nodes of a cluster target.
//!
//! `mmd` owns one interface per host (`mm0`). Each node generates its own key
//! pair and only public keys ever travel. A node's peer set is a function of
//! the target's nodes and the prefixes routed to each of them, so everything
//! that changes the mesh (a node added or removed, a rotated key, a network
//! slice) is a difference between two peer sets that a plan can show.

use std::net::Ipv4Addr;

use base64::Engine;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::repo_target::{Ipv4Cidr, WireguardMesh};

#[path = "mesh/guard.rs"]
mod guard;
pub use guard::{MESH_FORWARD_CHAIN, MESH_INPUT_CHAIN, MeshGuard};
#[path = "mesh/status.rs"]
mod status;
pub use status::{
    MESH_HANDSHAKE_STALE_SECONDS, MeshNodeReport, MeshNodeStatus, MeshPeerStatus, MeshReport,
    mesh_diagnostics,
};

/// The WireGuard interface `mmd` owns on every node.
pub const MESH_INTERFACE: &str = "mm0";
/// The interface the setup scripts of earlier mm versions created with
/// wg-quick. Setup replaces it with [`MESH_INTERFACE`], keeping the node's key.
pub const LEGACY_MESH_INTERFACE: &str = "wg0";
/// Every peer sends a keepalive this often, so a quiet link keeps its
/// handshake fresh and passes stateful firewalls.
pub const MESH_KEEPALIVE_SECONDS: u16 = 25;

/// One node as the mesh sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshMember {
    pub name: String,
    pub address: Ipv4Addr,
    /// The host or address the other nodes send WireGuard packets to: the
    /// node's `endpoint`, else its `host`.
    pub endpoint: String,
    /// The node's WireGuard public key; `None` until the node has made one.
    pub public_key: Option<String>,
    /// Prefixes routed to this node besides its own mesh address, such as
    /// the slices of cluster networks.
    pub routed: Vec<Ipv4Cidr>,
}

/// One peer as a node holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshPeer {
    /// The peer's node name.
    pub name: String,
    pub public_key: String,
    /// `host:port` this node sends to.
    pub endpoint: String,
    /// The peer's mesh address and every prefix routed to it.
    pub allowed_ips: Vec<Ipv4Cidr>,
}

/// A peer in a plan: as [`MeshPeer`], except that the key of a node that has
/// not made one yet is unknown until apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPeer {
    pub name: String,
    pub public_key: Option<String>,
    pub endpoint: String,
    pub allowed_ips: Vec<Ipv4Cidr>,
}

impl PlannedPeer {
    /// The peer once its key is known.
    #[must_use]
    pub fn resolved(&self) -> Option<MeshPeer> {
        Some(MeshPeer {
            name: self.name.clone(),
            public_key: self.public_key.clone()?,
            endpoint: self.endpoint.clone(),
            allowed_ips: self.allowed_ips.clone(),
        })
    }
}

impl From<&MeshPeer> for PlannedPeer {
    fn from(peer: &MeshPeer) -> Self {
        Self {
            name: peer.name.clone(),
            public_key: Some(peer.public_key.clone()),
            endpoint: peer.endpoint.clone(),
            allowed_ips: peer.allowed_ips.clone(),
        }
    }
}

/// The peers of `node`: every other member, reachable at its endpoint on the
/// mesh port, allowed to send from its mesh address and from each prefix
/// routed to it. The order is the members' order.
#[must_use]
pub fn peer_set(port: u16, members: &[MeshMember], node: &str) -> Vec<PlannedPeer> {
    members
        .iter()
        .filter(|member| member.name != node)
        .map(|member| {
            let mut allowed_ips = vec![Ipv4Cidr {
                address: member.address,
                prefix: 32,
            }];
            let mut routed = member.routed.clone();
            routed.sort();
            routed.dedup();
            allowed_ips.extend(routed);
            PlannedPeer {
                name: member.name.clone(),
                public_key: member.public_key.clone(),
                endpoint: endpoint_with_port(&member.endpoint, port),
                allowed_ips,
            }
        })
        .collect()
}

/// `host:port`, with an IPv6 address in brackets as WireGuard writes it.
#[must_use]
pub fn endpoint_with_port(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// The host part of a `host:port` endpoint, without IPv6 brackets.
#[must_use]
pub fn endpoint_host(endpoint: &str) -> &str {
    let host = endpoint.rsplit_once(':').map_or(endpoint, |(host, _)| host);
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
}

/// A node's interface as `mmd` stores and applies it. It holds no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshState {
    pub interface: String,
    /// The node's own address on the mesh.
    pub address: Ipv4Addr,
    /// Prefix length of the mesh subnet.
    pub prefix: u8,
    /// The UDP port WireGuard listens on.
    pub port: u16,
    pub mtu: u16,
    pub peers: Vec<MeshPeer>,
}

impl MeshState {
    /// The mesh address of every other node this node knows, by node name.
    #[must_use]
    pub fn peer_addresses(&self) -> std::collections::BTreeMap<String, Ipv4Addr> {
        self.peers
            .iter()
            .filter_map(|peer| {
                let own = peer
                    .allowed_ips
                    .iter()
                    .find(|allowed| allowed.prefix == 32)?;
                Some((peer.name.clone(), own.address))
            })
            .collect()
    }

    /// The interface of one node of `mesh`.
    #[must_use]
    pub fn new(mesh: &WireguardMesh, address: Ipv4Addr, peers: Vec<MeshPeer>) -> Self {
        Self {
            interface: MESH_INTERFACE.to_owned(),
            address,
            prefix: mesh.subnet.prefix,
            port: mesh.port,
            mtu: mesh.mtu,
            peers,
        }
    }

    /// The mesh subnet this interface is in.
    #[must_use]
    pub fn subnet(&self) -> Ipv4Cidr {
        let mask = u32::MAX
            .checked_shl(32 - u32::from(self.prefix))
            .unwrap_or(0);
        Ipv4Cidr {
            address: Ipv4Addr::from(u32::from(self.address) & mask),
            prefix: self.prefix,
        }
    }

    /// Refuses a state `mmd` must not apply: another interface than its own,
    /// a malformed key or endpoint, or two peers that claim the same name,
    /// key, or address.
    pub fn validate(&self) -> Result<(), String> {
        if self.interface != MESH_INTERFACE {
            return Err(format!(
                "mmd owns the interface {MESH_INTERFACE}, not '{}'",
                self.interface
            ));
        }
        if self.prefix > 30 || self.port == 0 || !(1280..=9000).contains(&self.mtu) {
            return Err("the mesh prefix, port, or MTU is out of range".to_owned());
        }
        let mut names = std::collections::BTreeSet::new();
        let mut keys = std::collections::BTreeSet::new();
        let mut allowed = std::collections::BTreeSet::new();
        for peer in &self.peers {
            crate::domain::target::validate_target_name(&peer.name)
                .map_err(|_| format!("'{}' is not a valid peer name", peer.name))?;
            validate_public_key(&peer.public_key)
                .map_err(|message| format!("peer {}: {message}", peer.name))?;
            validate_endpoint(&peer.endpoint)
                .map_err(|message| format!("peer {}: {message}", peer.name))?;
            if !names.insert(peer.name.as_str()) || !keys.insert(peer.public_key.as_str()) {
                return Err(format!("peer {} is listed twice", peer.name));
            }
            if peer.allowed_ips.is_empty() {
                return Err(format!("peer {} has no allowed address", peer.name));
            }
            for prefix in &peer.allowed_ips {
                if prefix.contains(self.address) || !allowed.insert(*prefix) {
                    return Err(format!(
                        "peer {} claims {prefix}, which this node or another peer holds",
                        peer.name
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A WireGuard public key: 32 bytes in standard base64.
pub fn validate_public_key(key: &str) -> Result<(), String> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(key)
        .map_err(|_| "the WireGuard public key is not base64".to_owned())?;
    if decoded.len() == 32 && key.len() == 44 {
        Ok(())
    } else {
        Err("the WireGuard public key is not 32 bytes".to_owned())
    }
}

/// `host:port` made of characters a host name, an IPv4 address, or a
/// bracketed IPv6 address can hold.
fn validate_endpoint(endpoint: &str) -> Result<(), String> {
    let invalid = || format!("endpoint '{endpoint}' is not host:port");
    let (host, port) = endpoint.rsplit_once(':').ok_or_else(invalid)?;
    port.parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(invalid)?;
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    let valid = !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('-')
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'));
    if valid { Ok(()) } else { Err(invalid()) }
}

/// What a node says about its own mesh: no key material except public
/// keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct MeshFacts {
    /// The node's WireGuard public key, once it has a key pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// The public half of a key made for a rotation that is not finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_public_key: Option<String>,
    /// The wg-quick interface of an earlier mm version, while it is there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_interface: Option<String>,
    /// The interface as last applied, or as the legacy configuration has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<MeshState>,
    /// The addresses and routes the node has outside mm, which the mesh
    /// subnet and the cluster's networks must not overlap.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_ranges: Vec<crate::domain::cluster_network::HostRange>,
    /// The release of mm the node's `mmd` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<crate::domain::cluster_setup::HostRelease>,
}

/// What an apply changed on a node, for its output and the audit record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct MeshApplied {
    pub changed: bool,
    /// One line per change, such as `added peer app-3`.
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PeerAction {
    Add,
    Change,
    Remove,
    Keep,
}

/// One peer of one node in a plan, and the entry it replaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshPeerPlan {
    pub name: String,
    pub action: PeerAction,
    /// `None`: the peer's node makes or reports its key during apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    pub endpoint: String,
    pub allowed_ips: Vec<Ipv4Cidr>,
    /// The entry the node holds now, when the plan changes or removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<MeshPeer>,
}

impl MeshPeerPlan {
    fn of(peer: &PlannedPeer, action: PeerAction, previous: Option<&MeshPeer>) -> Self {
        Self {
            name: peer.name.clone(),
            action,
            public_key: peer.public_key.clone(),
            endpoint: peer.endpoint.clone(),
            allowed_ips: peer.allowed_ips.clone(),
            previous: previous.cloned(),
        }
    }

    /// What the plan does to this peer, in a few words.
    #[must_use]
    pub fn summary(&self) -> String {
        let key = |key: &str| key.chars().take(12).collect::<String>();
        let allowed = |allowed: &[Ipv4Cidr]| {
            allowed
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let described = format!(
            "{} at {}, key {}",
            allowed(&self.allowed_ips),
            self.endpoint,
            self.public_key.as_deref().map_or_else(
                || format!("made on {} during apply", self.name),
                |value| format!("{}…", key(value))
            )
        );
        match (self.action, &self.previous) {
            (PeerAction::Add, _) => format!("+ {}: {described}", self.name),
            (PeerAction::Keep, _) => format!("= {}: unchanged", self.name),
            (PeerAction::Remove, _) => format!("- {}: removed", self.name),
            (PeerAction::Change, Some(previous)) => {
                let mut changes = Vec::new();
                match &self.public_key {
                    Some(new) if *new != previous.public_key => changes.push(format!(
                        "key {}… -> {}…",
                        key(&previous.public_key),
                        key(new)
                    )),
                    Some(_) => {}
                    None => changes.push(format!(
                        "key {}… -> the key {} reports during apply",
                        key(&previous.public_key),
                        self.name
                    )),
                }
                if self.endpoint != previous.endpoint {
                    changes.push(format!(
                        "endpoint {} -> {}",
                        previous.endpoint, self.endpoint
                    ));
                }
                if self.allowed_ips != previous.allowed_ips {
                    changes.push(format!(
                        "allowed {} -> {}",
                        allowed(&previous.allowed_ips),
                        allowed(&self.allowed_ips)
                    ));
                }
                format!("~ {}: {}", self.name, changes.join(", "))
            }
            (PeerAction::Change, None) => format!("~ {}: {described}", self.name),
        }
    }
}

/// The difference between the peers a node holds and the peers it should
/// hold: one entry per peer, the desired ones first in their order, then
/// the ones to remove. `current` is `None` for a node without a mesh.
#[must_use]
pub fn plan_peers(current: Option<&[MeshPeer]>, desired: &[PlannedPeer]) -> Vec<MeshPeerPlan> {
    let current = current.unwrap_or_default();
    let mut plans = desired
        .iter()
        .map(|peer| {
            let Some(held) = current.iter().find(|held| held.name == peer.name) else {
                return MeshPeerPlan::of(peer, PeerAction::Add, None);
            };
            if peer.resolved().as_ref() == Some(held) {
                MeshPeerPlan::of(peer, PeerAction::Keep, None)
            } else {
                MeshPeerPlan::of(peer, PeerAction::Change, Some(held))
            }
        })
        .collect::<Vec<_>>();
    plans.extend(
        current
            .iter()
            .filter(|held| !desired.iter().any(|peer| peer.name == held.name))
            .map(|held| MeshPeerPlan::of(&held.into(), PeerAction::Remove, Some(held))),
    );
    plans
}

/// Where a node's mesh stands before the plan is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MeshNodeOrigin {
    /// The node has no mesh yet, or its state is read only during apply
    /// because its SSH host key is new.
    New,
    /// The node still runs the wg-quick interface of an earlier mm version;
    /// the plan replaces it and keeps the node's key.
    Legacy,
    /// The node already runs `mm0`.
    Current,
}

/// One node's section of the mesh plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshNodePlan {
    pub node: String,
    pub address: Ipv4Addr,
    /// `host:port` the other nodes reach this node's WireGuard at.
    pub endpoint: String,
    /// `None`: the node makes its key pair during apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    pub origin: MeshNodeOrigin,
    /// Interface settings that change, such as `mtu 1420 -> 1380`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interface_changes: Vec<String>,
    pub peers: Vec<MeshPeerPlan>,
    /// The release of mm the node runs, when its `mmd` said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<crate::domain::cluster_setup::HostRelease>,
}

impl MeshNodePlan {
    /// Whether applying the plan changes anything on this node.
    #[must_use]
    pub fn changes(&self) -> bool {
        self.origin != MeshNodeOrigin::Current
            || !self.interface_changes.is_empty()
            || self
                .peers
                .iter()
                .any(|peer| peer.action != PeerAction::Keep)
    }
}

/// The mesh section of a cluster setup plan: every selected node's interface
/// and what changes in its peer set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshPlan {
    pub interface: String,
    pub subnet: Ipv4Cidr,
    /// The UDP port every node listens on, and must be reachable at from
    /// every other node.
    pub port: u16,
    pub mtu: u16,
    pub nodes: Vec<MeshNodePlan>,
}

impl MeshPlan {
    /// Whether applying the plan changes the mesh on any node.
    #[must_use]
    pub fn changes(&self) -> bool {
        self.nodes.iter().any(MeshNodePlan::changes)
    }
}

/// The interface settings of `state` that differ from `mesh`, in words.
#[must_use]
pub fn interface_changes(
    mesh: &WireguardMesh,
    address: Ipv4Addr,
    state: &MeshState,
) -> Vec<String> {
    let mut changes = Vec::new();
    if state.interface != MESH_INTERFACE {
        changes.push(format!("interface {} -> {MESH_INTERFACE}", state.interface));
    }
    if state.address != address || state.prefix != mesh.subnet.prefix {
        changes.push(format!(
            "address {}/{} -> {address}/{}",
            state.address, state.prefix, mesh.subnet.prefix
        ));
    }
    if state.port != mesh.port {
        changes.push(format!("port {} -> {}", state.port, mesh.port));
    }
    if state.mtu != mesh.mtu {
        changes.push(format!("mtu {} -> {}", state.mtu, mesh.mtu));
    }
    changes
}

/// Gives the peers of a legacy configuration, which records no names, the
/// name of the member whose mesh address each one routes. A peer no member
/// matches is shown by that address.
pub fn name_peers(peers: &mut [MeshPeer], members: &[MeshMember]) {
    for peer in peers.iter_mut().filter(|peer| peer.name.is_empty()) {
        let host = peer.allowed_ips.iter().find(|allowed| allowed.prefix == 32);
        let member = members
            .iter()
            .find(|member| host.is_some_and(|host| host.address == member.address));
        peer.name = match (member, host) {
            (Some(member), _) => member.name.clone(),
            (None, Some(host)) => host.address.to_string(),
            (None, None) => peer.public_key.chars().take(12).collect(),
        };
    }
}

/// The peers `node` should hold after a setup run: the peer set, except
/// that for a node the run leaves out (`left_out`) the entry `node` already
/// holds is kept as it is, because that node's key cannot be read now.
#[must_use]
pub fn desired_peers(
    port: u16,
    members: &[MeshMember],
    node: &str,
    left_out: &[&str],
    held: &[MeshPeer],
) -> Vec<PlannedPeer> {
    peer_set(port, members, node)
        .into_iter()
        .filter_map(|peer| {
            if left_out.contains(&peer.name.as_str()) {
                held.iter()
                    .find(|held| held.name == peer.name)
                    .map(PlannedPeer::from)
            } else {
                Some(peer)
            }
        })
        .collect()
}

/// One node's section of the mesh plan: its interface and the difference
/// between the peers it holds (`facts`, when the node could be read) and
/// the peers it should hold.
#[must_use]
pub fn plan_node(
    mesh: &WireguardMesh,
    members: &[MeshMember],
    node: &MeshMember,
    left_out: &[&str],
    facts: Option<&MeshFacts>,
) -> MeshNodePlan {
    let mut state = facts.and_then(|facts| facts.state.clone());
    if let Some(state) = &mut state {
        name_peers(&mut state.peers, members);
    }
    let held = state.as_ref().map(|state| state.peers.as_slice());
    let desired = desired_peers(
        mesh.port,
        members,
        &node.name,
        left_out,
        held.unwrap_or_default(),
    );
    let origin = match (facts, &state) {
        (Some(facts), _) if facts.legacy_interface.is_some() => MeshNodeOrigin::Legacy,
        (_, Some(state)) if state.interface == MESH_INTERFACE => MeshNodeOrigin::Current,
        _ => MeshNodeOrigin::New,
    };
    MeshNodePlan {
        node: node.name.clone(),
        address: node.address,
        endpoint: endpoint_with_port(&node.endpoint, mesh.port),
        public_key: node.public_key.clone(),
        origin,
        interface_changes: state
            .as_ref()
            .map(|state| interface_changes(mesh, node.address, state))
            .unwrap_or_default(),
        peers: plan_peers(held, &desired),
        release: facts.and_then(|facts| facts.release.clone()),
    }
}

/// One peer's entry in a key rotation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshRotationPeer {
    /// The node that replaces its entry for the rotated node.
    pub node: String,
    /// The key that node holds for the rotated node now; `None` when it has
    /// no entry for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub holds: Option<String>,
}

/// `mm target mesh rotate NODE`: the node makes a new key pair and every
/// other node replaces the one entry it holds for it. Nothing else changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshRotationPlan {
    pub environment: String,
    pub target: String,
    /// The node whose key is replaced.
    pub node: String,
    pub address: Ipv4Addr,
    /// The key the node uses now.
    pub current_public_key: String,
    /// A new key the node already made in an interrupted rotation; the plan
    /// finishes that rotation with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_public_key: Option<String>,
    pub peers: Vec<MeshRotationPeer>,
    pub plan_hash: String,
}

impl MeshRotationPlan {
    /// BLAKE3 of the plan's canonical JSON without its own hash.
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut unhashed = self.clone();
        unhashed.plan_hash.clear();
        let bytes = serde_json::to_vec(&unhashed)
            .map_err(|error| format!("serializing the mesh rotation plan: {error}"))?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"mm-mesh-rotation-plan\0");
        hasher.update(&bytes);
        Ok(hasher.finalize().to_hex().to_string())
    }

    #[must_use]
    pub fn apply_command(&self) -> String {
        format!(
            "mm --environment {} target mesh rotate {} --plan {}",
            self.environment, self.node, self.plan_hash
        )
    }
}

/// What a finished rotation did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MeshRotationResult {
    pub environment: String,
    pub node: String,
    pub previous_public_key: String,
    pub public_key: String,
    /// The nodes that replaced their entry for the rotated node.
    pub updated_peers: Vec<String>,
}
