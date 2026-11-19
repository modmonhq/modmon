//! `mm target mesh status`: what every node's WireGuard interface reports,
//! and what is wrong with it in words that say what to do.

use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{MESH_INTERFACE, endpoint_host};
use crate::domain::diagnostic::Diagnostic;

/// With a keepalive every 25 seconds a live peer renews its session about
/// every two minutes; WireGuard drops a session after three. An older
/// handshake means the peer does not answer.
pub const MESH_HANDSHAKE_STALE_SECONDS: u64 = 180;

/// One peer as a node's interface reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MeshPeerStatus {
    /// The peer's node name; `None` for a peer that is on the interface but
    /// not in the state `mmd` applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub public_key: String,
    /// Where the node sends to now. WireGuard follows a peer that roams, so
    /// this can differ from the configured endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// The `host:port` the applied plan configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configured_endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    /// Seconds since the last handshake by the node's own clock; `None`
    /// when there has never been one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handshake_age_seconds: Option<u64>,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    /// `false` when the applied state lists the peer and the interface does
    /// not have it.
    pub on_interface: bool,
}

impl MeshPeerStatus {
    /// Whether the peer answered recently enough to carry traffic.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.handshake_age_seconds
            .is_some_and(|age| age <= MESH_HANDSHAKE_STALE_SECONDS)
    }
}

/// One node's WireGuard interface, as its `mmd` reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MeshNodeStatus {
    /// The node's own name from `/etc/mm/node.env`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// `None` when `mmd` manages no WireGuard interface on the host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    /// Whether the interface exists and is up.
    pub up: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<Ipv4Addr>,
    /// The UDP port the interface listens on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    pub peers: Vec<MeshPeerStatus>,
    /// What the firewall chains that keep the mesh between the hosts
    /// (`MM-MESH`, `MM-MESH-IN`) have that differs from what the node's
    /// mesh and networks ask for; empty when they are in place.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guard_drift: Vec<String>,
}

/// One row of `mm target mesh status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeshNodeReport {
    pub name: String,
    pub host: String,
    /// Whether the node's `mmd` answered.
    pub reachable: bool,
    /// Why there is no status, and what to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<MeshNodeStatus>,
}

impl MeshNodeReport {
    /// The node's mesh in a few words for a node list, such as
    /// `mesh 2/2 peers`; `None` when the node did not report one.
    #[must_use]
    pub fn summary(&self) -> Option<String> {
        let status = self.status.as_ref()?;
        status.interface.as_ref()?;
        if !status.up {
            return Some("mesh down".to_owned());
        }
        let connected = status.peers.iter().filter(|peer| peer.connected()).count();
        Some(format!("mesh {connected}/{} peers", status.peers.len()))
    }
}

/// The mesh of one target: every node's interface and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeshReport {
    pub target: String,
    /// The private interface the nodes already share, when the target names
    /// one (`cluster.mesh.interface`): mm then manages no WireGuard mesh and
    /// the report lists no nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_interface: Option<String>,
    pub nodes: Vec<MeshNodeReport>,
    /// Each problem with the command or setting that fixes it.
    pub problems: Vec<Diagnostic>,
}

impl MeshReport {
    #[must_use]
    pub fn new(target: String, nodes: Vec<MeshNodeReport>) -> Self {
        let problems = mesh_diagnostics(&target, &nodes);
        Self {
            target,
            existing_interface: None,
            nodes,
            problems,
        }
    }

    /// The report of a target whose nodes use a private network they
    /// already have.
    #[must_use]
    pub fn on_existing_interface(target: String, interface: String) -> Self {
        Self {
            target,
            existing_interface: Some(interface),
            nodes: Vec::new(),
            problems: Vec::new(),
        }
    }

    /// Whether every node answered and nothing is wrong with the mesh.
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.problems.is_empty() && self.nodes.iter().all(|node| node.reachable)
    }
}

const SETUP_HELP: &str =
    "run `mm target setup plan` and apply it; the plan shows the peer changes on every node";

fn age(seconds: Option<u64>) -> String {
    match seconds {
        None => "never".to_owned(),
        Some(seconds) if seconds < 120 => format!("{seconds} s ago"),
        Some(seconds) if seconds < 7200 => format!("{} min ago", seconds / 60),
        Some(seconds) => format!("{} h ago", seconds / 3600),
    }
}

/// What is wrong with a target's mesh: nodes without the interface, peer
/// entries that are missing, stale, or hold another key than the peer uses,
/// and pairs of nodes with no recent handshake, each with what to do.
#[must_use]
pub fn mesh_diagnostics(target: &str, nodes: &[MeshNodeReport]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut silent = Vec::<(usize, usize)>::new();
    for (index, report) in nodes.iter().enumerate() {
        let Some(status) = &report.status else {
            continue;
        };
        let node = &report.name;
        if status.interface.is_none() {
            diagnostics.push(
                Diagnostic::error(
                    "MM_MESH_NOT_SET_UP",
                    format!("node '{node}' has no mesh interface: its mmd holds no mesh state"),
                )
                .with_help(SETUP_HELP),
            );
            continue;
        }
        if !status.up {
            diagnostics.push(
                Diagnostic::error(
                    "MM_MESH_DOWN",
                    format!("node '{node}': the interface {MESH_INTERFACE} is not up"),
                )
                .with_help(format!(
                    "on {node}, `systemctl restart mm-mesh` brings it up from the state mmd keeps, and `journalctl -u mm-mesh` says why it is down"
                )),
            );
            continue;
        }
        if !status.guard_drift.is_empty() {
            diagnostics.push(guard_drifted(node, &status.guard_drift));
        }
        for (other_index, other) in nodes.iter().enumerate() {
            if other_index == index {
                continue;
            }
            let peer = status
                .peers
                .iter()
                .find(|peer| peer.name.as_deref() == Some(other.name.as_str()));
            match peer_problem(node, &other.name, peer, other.status.as_ref()) {
                PeerFinding::Problem(diagnostic) => diagnostics.push(diagnostic),
                PeerFinding::Silent => {
                    let pair = (index.min(other_index), index.max(other_index));
                    if !silent.contains(&pair) {
                        silent.push(pair);
                    }
                }
                PeerFinding::Connected => {}
            }
        }
        for peer in &status.peers {
            let known = peer
                .name
                .as_deref()
                .is_some_and(|name| nodes.iter().any(|other| other.name == name));
            if !known {
                diagnostics.push(
                    Diagnostic::warning(
                        "MM_MESH_EXTRA_PEER",
                        format!(
                            "node '{node}' has a peer that is not a node of '{target}': {}",
                            peer.name.as_deref().unwrap_or(&peer.public_key)
                        ),
                    )
                    .with_help(SETUP_HELP),
                );
            }
        }
    }
    diagnostics.extend(
        silent
            .into_iter()
            .map(|(left, right)| no_handshake(&nodes[left], &nodes[right])),
    );
    diagnostics
}

/// The node's firewall chains for the mesh are not what its `mmd` wants.
fn guard_drifted(node: &str, drift: &[String]) -> Diagnostic {
    Diagnostic::error(
        "MM_MESH_GUARD",
        format!(
            "node '{node}': the firewall rules that keep the mesh between the hosts are not in place: {}",
            drift.join("; ")
        ),
    )
    .with_help(format!(
        "on {node}, `systemctl reload mm-mesh` loads them again from what mmd stored, and `journalctl -u mm-mesh` says why they are missing"
    ))
}

enum PeerFinding {
    Connected,
    /// No recent handshake; reported once for the pair.
    Silent,
    Problem(Diagnostic),
}

/// What is wrong with `node`'s entry for `other`, given what `other` says
/// about itself.
fn peer_problem(
    node: &str,
    other: &str,
    peer: Option<&MeshPeerStatus>,
    other_status: Option<&MeshNodeStatus>,
) -> PeerFinding {
    let Some(peer) = peer else {
        return PeerFinding::Problem(
            Diagnostic::error(
                "MM_MESH_PEER_MISSING",
                format!("node '{node}' has no peer entry for '{other}'"),
            )
            .with_help(SETUP_HELP),
        );
    };
    if !peer.on_interface {
        return PeerFinding::Problem(
            Diagnostic::error(
                "MM_MESH_DRIFT",
                format!(
                    "node '{node}': peer '{other}' is in mmd's state but not on {MESH_INTERFACE}"
                ),
            )
            .with_help(format!(
                "on {node}, `systemctl restart mm-mesh` applies the stored state again"
            )),
        );
    }
    if let Some(actual) = other_status.and_then(|status| status.public_key.as_deref())
        && actual != peer.public_key
    {
        return PeerFinding::Problem(
            Diagnostic::error(
                "MM_MESH_KEY_MISMATCH",
                format!(
                    "node '{node}' holds another WireGuard key for '{other}' than '{other}' uses, so they cannot connect"
                ),
            )
            .with_help(SETUP_HELP),
        );
    }
    if peer.connected() {
        PeerFinding::Connected
    } else {
        PeerFinding::Silent
    }
}

/// Two nodes that do not hear each other: which UDP port has to be open
/// between which addresses.
fn no_handshake(left: &MeshNodeReport, right: &MeshNodeReport) -> Diagnostic {
    let entry = |from: &MeshNodeReport, to: &MeshNodeReport| {
        from.status
            .as_ref()
            .and_then(|status| {
                status
                    .peers
                    .iter()
                    .find(|peer| peer.name.as_deref() == Some(to.name.as_str()))
            })
            .cloned()
    };
    let (left_entry, right_entry) = (entry(left, right), entry(right, left));
    // Each side's entry for the other holds the address it sends to.
    let underlay = |entry: &Option<MeshPeerStatus>, node: &MeshNodeReport| {
        entry
            .as_ref()
            .and_then(|peer| peer.configured_endpoint.as_deref())
            .map_or_else(
                || node.host.clone(),
                |endpoint| endpoint_host(endpoint).to_owned(),
            )
    };
    let port = [&left_entry, &right_entry]
        .into_iter()
        .flatten()
        .filter_map(|peer| peer.configured_endpoint.as_deref())
        .find_map(|endpoint| endpoint.rsplit_once(':').map(|(_, port)| port.to_owned()))
        .or_else(|| {
            [left, right]
                .into_iter()
                .filter_map(|node| node.status.as_ref()?.port)
                .map(|port| port.to_string())
                .next()
        })
        .unwrap_or_else(|| "51820".to_owned());
    let seen = |entry: &Option<MeshPeerStatus>, node: &MeshNodeReport| match entry {
        Some(peer) => format!(
            "{} last heard from it {}",
            node.name,
            age(peer.handshake_age_seconds)
        ),
        None => format!("{} did not report", node.name),
    };
    Diagnostic::error(
        "MM_MESH_NO_HANDSHAKE",
        format!(
            "'{}' and '{}' have no WireGuard handshake ({}; {})",
            left.name,
            right.name,
            seen(&left_entry, left),
            seen(&right_entry, right)
        ),
    )
    .with_help(format!(
        "UDP port {port} must be open between {} ({}) and {} ({}) in both directions: allow it in each host's firewall and in the provider's firewall or security group, then run `mm target mesh status` again",
        underlay(&right_entry, left),
        left.name,
        underlay(&left_entry, right),
        right.name
    ))
}
