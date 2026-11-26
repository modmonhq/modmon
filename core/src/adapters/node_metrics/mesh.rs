//! The node's mesh peers, as its WireGuard interface shows them.

use std::sync::Arc;

use crate::app::ports::{MeshHost, MeshPeers};
use crate::domain::error::MmError;
use crate::domain::node_metrics::PeerSample;

pub struct InterfacePeers {
    mesh: Arc<dyn MeshHost>,
}

impl std::fmt::Debug for InterfacePeers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InterfacePeers")
            .finish_non_exhaustive()
    }
}

impl InterfacePeers {
    #[must_use]
    pub fn new(mesh: Arc<dyn MeshHost>) -> Self {
        Self { mesh }
    }
}

#[async_trait::async_trait]
impl MeshPeers for InterfacePeers {
    async fn peers(&self) -> Result<Option<Vec<PeerSample>>, MmError> {
        let status = self.mesh.status().await?;
        if status.interface.is_none() {
            return Ok(None);
        }
        Ok(Some(
            status
                .peers
                .into_iter()
                .map(|peer| PeerSample {
                    peer: peer
                        .name
                        .unwrap_or_else(|| peer.public_key.chars().take(12).collect()),
                    handshake_age_seconds: peer.handshake_age_seconds,
                    received_bytes: peer.received_bytes,
                    sent_bytes: peer.sent_bytes,
                    on_interface: peer.on_interface,
                })
                .collect(),
        ))
    }
}
