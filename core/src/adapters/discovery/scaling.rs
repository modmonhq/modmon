//! What the nodes tell each other about their scaled services
//! (`domain::autoscale`), carried with the records they already exchange.

use std::time::Duration;

use super::NameResolver;
use crate::domain::autoscale::ScaleReport;
use crate::domain::discovery::PEER_EXPIRY_MS;

impl NameResolver {
    /// This node's name, once it holds a cluster network.
    #[must_use]
    pub fn node_name(&self) -> Option<String> {
        self.read()
            .networks
            .as_ref()
            .map(|networks| networks.node.clone())
    }

    /// What this node tells the others about its scaled services from now
    /// on, with its next answer.
    pub fn publish_scaling(&self, reports: Vec<ScaleReport>) {
        self.write().scaling = reports;
    }

    /// What every node heard from recently says about its scaled
    /// services, by node. A node that has not answered for a while is not
    /// listed: it counts as not answering.
    #[must_use]
    pub fn peer_scaling(&self) -> Vec<(String, Vec<ScaleReport>)> {
        let expiry = Duration::from_millis(PEER_EXPIRY_MS);
        self.read()
            .peers
            .iter()
            .filter(|(_, peer)| peer.heard.elapsed() < expiry)
            .filter_map(|(name, peer)| {
                peer.records
                    .as_ref()
                    .map(|records| (name.clone(), records.scaling.clone()))
            })
            .collect()
    }
}
