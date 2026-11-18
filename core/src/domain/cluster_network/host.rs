//! The address ranges a node already uses for something else. The mesh
//! subnet and the space of the cluster's networks must stay clear of them:
//! a container address that the node also routes to its provider's private
//! network would reach one of the two, never both.

use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::repo_target::Ipv4Cidr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HostRangeKind {
    /// An address of one of the node's interfaces.
    Address,
    /// A route of the node's main routing table.
    Route,
}

/// One range a node uses outside mm: an address it holds or a route it
/// has. The mesh interface and the bridges of cluster networks are mm's
/// own and never listed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HostRange {
    pub range: Ipv4Cidr,
    pub kind: HostRangeKind,
    /// The interface that holds the address or carries the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
}

impl HostRange {
    /// `the route 10.106.0.0/20 through eth1`.
    #[must_use]
    pub fn described(&self) -> String {
        let interface = self
            .interface
            .as_deref()
            .map(|interface| match self.kind {
                HostRangeKind::Address => format!(" on {interface}"),
                HostRangeKind::Route => format!(" through {interface}"),
            })
            .unwrap_or_default();
        match self.kind {
            HostRangeKind::Address => format!("the address {}{interface}", self.range.address),
            HostRangeKind::Route => format!("the route {}{interface}", self.range),
        }
    }
}

/// Ranges to offer instead of one a node already uses, in order: a /24
/// for the mesh, a /12 for the networks.
fn candidates(mesh: bool) -> Vec<Ipv4Cidr> {
    let (second, prefix): (&[u8], u8) = if mesh {
        (&[90, 92, 94, 87, 86], 24)
    } else {
        (&[64, 32, 112, 160, 208], 12)
    };
    second
        .iter()
        .map(|second| Ipv4Cidr {
            address: Ipv4Addr::new(10, *second, 0, 0),
            prefix,
        })
        .collect()
}

/// Why node `node` cannot take part with these settings: the first of its
/// own ranges that overlaps the mesh subnet or the space of the cluster's
/// networks, with the key to set. `held` is every node's ranges that are
/// known, so the range offered instead is free on all of them.
#[must_use]
pub fn range_conflict(
    target: &str,
    node: &str,
    held: &[(String, Vec<HostRange>)],
    mesh_subnet: Option<Ipv4Cidr>,
    networks: Option<Ipv4Cidr>,
) -> Option<String> {
    let own = held
        .iter()
        .find(|(name, _)| name == node)
        .map(|(_, ranges)| ranges.as_slice())?;
    let settings = [
        (
            mesh_subnet,
            "cluster.mesh.subnet",
            "the nodes' mesh addresses",
        ),
        (
            networks,
            "cluster.networks",
            "the addresses of networks that span the nodes",
        ),
    ];
    for (setting, key, purpose) in settings {
        let Some(setting) = setting else {
            continue;
        };
        // An address inside a route that also overlaps says more.
        let Some(range) = own
            .iter()
            .filter(|range| range.range.overlaps(setting))
            .min_by_key(|range| range.kind != HostRangeKind::Route)
        else {
            continue;
        };
        let mesh = key == "cluster.mesh.subnet";
        let other = if mesh { networks } else { mesh_subnet };
        let free = candidates(mesh).into_iter().find(|candidate| {
            !other.is_some_and(|other| other.overlaps(*candidate))
                && !held
                    .iter()
                    .flat_map(|(_, ranges)| ranges)
                    .any(|range| range.range.overlaps(*candidate))
        });
        let offer = free.map_or_else(String::new, |free| format!(", such as {free}"));
        return Some(format!(
            "node '{node}' already has {}, which overlaps {key} {setting} ({purpose}). Set `{key}` under targets.{target}.cluster to a private range no node uses{offer}, then plan again",
            range.described()
        ));
    }
    None
}
