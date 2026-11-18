//! The `wg set` commands that bring an interface's peers to the desired
//! set. A peer that does not change gets no command, so its session and its
//! roamed endpoint stay as they are.

use std::collections::{BTreeMap, BTreeSet};

use super::kernel::KernelPeer;
use crate::domain::mesh::{MESH_KEEPALIVE_SECONDS, MeshPeer};

/// One change to the interface: what it does in words, and the arguments
/// `wg` takes for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PeerCommand {
    pub action: String,
    pub arguments: Vec<String>,
}

fn short(key: &str) -> String {
    format!("{}…", key.chars().take(12).collect::<String>())
}

/// `resolved` maps a peer's name to the address its endpoint resolves to;
/// a peer missing from it is configured without an endpoint and learns it
/// from the first packet the other side sends. `previous` is the state
/// `mmd` applied before, which tells a changed endpoint from a roamed one.
pub(super) fn peer_commands(
    interface: &str,
    kernel: &[KernelPeer],
    desired: &[MeshPeer],
    previous: Option<&[MeshPeer]>,
    resolved: &BTreeMap<String, String>,
) -> Vec<PeerCommand> {
    let previous = previous.unwrap_or_default();
    let set = |key: &str| {
        vec![
            "set".to_owned(),
            interface.to_owned(),
            "peer".to_owned(),
            key.to_owned(),
        ]
    };
    let mut commands = kernel
        .iter()
        .filter(|held| {
            !desired
                .iter()
                .any(|peer| peer.public_key == held.public_key)
        })
        .map(|held| {
            let name = previous
                .iter()
                .find(|peer| peer.public_key == held.public_key)
                .map_or_else(String::new, |peer| format!("{} ", peer.name));
            let mut arguments = set(&held.public_key);
            arguments.push("remove".to_owned());
            PeerCommand {
                action: format!("removed peer {name}(key {})", short(&held.public_key)),
                arguments,
            }
        })
        .collect::<Vec<_>>();
    for peer in desired {
        let allowed = peer
            .allowed_ips
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let endpoint = resolved.get(&peer.name);
        let held = kernel
            .iter()
            .find(|held| held.public_key == peer.public_key);
        let mut arguments = set(&peer.public_key);
        let mut changed = Vec::new();
        if held.and_then(|held| held.keepalive) != Some(MESH_KEEPALIVE_SECONDS) {
            arguments.extend([
                "persistent-keepalive".to_owned(),
                MESH_KEEPALIVE_SECONDS.to_string(),
            ]);
            changed.push("keepalive");
        }
        let endpoint_stale = match held {
            None => true,
            Some(held) if held.endpoint.is_none() => true,
            Some(held) => match previous.iter().find(|before| before.name == peer.name) {
                Some(before) => before.endpoint != peer.endpoint,
                None => held.endpoint.as_ref() != endpoint,
            },
        };
        if let Some(endpoint) = endpoint.filter(|_| endpoint_stale) {
            arguments.extend(["endpoint".to_owned(), endpoint.clone()]);
            changed.push("endpoint");
        }
        let same_allowed = held.is_some_and(|held| {
            held.allowed_ips.iter().collect::<BTreeSet<_>>()
                == allowed.iter().collect::<BTreeSet<_>>()
        });
        if !same_allowed {
            arguments.extend(["allowed-ips".to_owned(), allowed.join(",")]);
            changed.push("allowed addresses");
        }
        if changed.is_empty() {
            continue;
        }
        commands.push(PeerCommand {
            action: if held.is_some() {
                format!("updated peer {} ({})", peer.name, changed.join(", "))
            } else {
                format!("added peer {} (key {})", peer.name, short(&peer.public_key))
            },
            arguments,
        });
    }
    commands
}
