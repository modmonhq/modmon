//! Where a node serves its metrics, and to whom.

use std::net::Ipv4Addr;

use super::{DeployedEnvironment, MetricsScope};
use crate::domain::cluster_network::NodeNetworks;
use crate::domain::host_policy::HostMetrics;
use crate::domain::mesh::MeshState;
use crate::domain::repo_target::Ipv4Cidr;

/// One address the node serves its metrics on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsListener {
    pub address: Ipv4Addr,
    /// The only addresses a scrape may come from.
    pub peers: Vec<Ipv4Cidr>,
    /// The node's own addresses, which a scrape never comes from: a
    /// process on the host itself reaches every one of them.
    pub own: Vec<Ipv4Addr>,
    pub scope: MetricsScope,
}

impl MetricsListener {
    #[must_use]
    pub fn admits(&self, peer: Ipv4Addr) -> bool {
        !self.own.contains(&peer) && self.peers.iter().any(|allowed| allowed.contains(peer))
    }
}

/// Every address the node serves its metrics on: its own address on each
/// cluster network an environment asks for them on (`metrics: true`),
/// for the containers of that network, and, when the host's admin asks
/// for it, its mesh address for the other hosts of the mesh. `mesh` is
/// the node's mesh whether or not it serves there: its address is never a
/// peer.
#[must_use]
pub fn listeners(
    policy: &HostMetrics,
    networks: Option<&NodeNetworks>,
    deployed: &[DeployedEnvironment],
    mesh: Option<&MeshState>,
) -> Vec<MetricsListener> {
    if !policy.enabled {
        return Vec::new();
    }
    let mut own = networks
        .map(NodeNetworks::local)
        .unwrap_or_default()
        .into_iter()
        .map(|local| local.gateway)
        .chain(mesh.map(|mesh| mesh.address))
        .collect::<Vec<_>>();
    own.sort_unstable();
    own.dedup();
    let mut listeners = Vec::new();
    for environment in deployed {
        for key in &environment.metrics_networks {
            let Some(local) = networks.and_then(|networks| networks.local_network(key)) else {
                continue;
            };
            listeners.push(MetricsListener {
                address: local.gateway,
                peers: vec![local.block],
                own: own.clone(),
                scope: MetricsScope::Environment {
                    project_id: environment.project_id,
                    environment: environment.environment.clone(),
                },
            });
        }
    }
    if let (true, Some(mesh)) = (policy.mesh, mesh) {
        listeners.push(MetricsListener {
            address: mesh.address,
            peers: mesh
                .peer_addresses()
                .into_values()
                .map(|address| Ipv4Cidr {
                    address,
                    prefix: 32,
                })
                .collect(),
            own: own.clone(),
            scope: MetricsScope::Host,
        });
    }
    listeners.sort_by_key(|listener| listener.address);
    listeners.dedup_by_key(|listener| listener.address);
    listeners
}
