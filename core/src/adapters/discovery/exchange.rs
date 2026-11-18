//! How the nodes tell each other what they run: a node connects to another
//! node's mesh address, reads one JSON document, and the connection closes.
//! Nothing is ever read from the connecting side, so the port cannot be
//! made to change anything. It answers the mesh addresses of the cluster's
//! nodes and closes every other connection without a byte; a container
//! keeps its own address when it connects, so it is not one of them.

use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket};

use super::NameResolver;
use crate::domain::discovery::{EXCHANGE_LIMIT_BYTES, NodeRecords};

/// What the node at `address` runs, as it says itself. The connection
/// leaves from this node's own mesh address `from`, which is how the other
/// node knows who asks.
pub(super) async fn fetch(
    from: Ipv4Addr,
    address: Ipv4Addr,
    port: u16,
    patience: Duration,
) -> Result<NodeRecords, String> {
    let read = async {
        let socket = TcpSocket::new_v4()?;
        socket.bind(SocketAddrV4::new(from, 0).into())?;
        let stream = socket
            .connect(SocketAddrV4::new(address, port).into())
            .await?;
        let mut document = Vec::new();
        stream
            .take(EXCHANGE_LIMIT_BYTES)
            .read_to_end(&mut document)
            .await?;
        Ok::<_, std::io::Error>(document)
    };
    let document = match tokio::time::timeout(patience, read).await {
        Ok(Ok(document)) => document,
        Ok(Err(error)) => return Err(error.to_string()),
        Err(_) => return Err(format!("no answer within {} ms", patience.as_millis())),
    };
    if document.is_empty() {
        return Err("it closed the connection without an answer: this node is not one of its mesh peers, or it holds no cluster network yet".to_owned());
    }
    serde_json::from_slice(&document)
        .map_err(|error| format!("its answer is not a record list: {error}"))
}

pub(super) async fn serve(resolver: Arc<NameResolver>, listener: TcpListener) {
    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(%error, "accepting a node's connection failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        if !resolver.is_mesh_member(peer.ip()) {
            continue;
        }
        // No share of the lookups' allowance: containers that flood the
        // resolver must not keep the nodes from hearing each other.
        let Some(records) = resolver.records_for_peers() else {
            continue;
        };
        tokio::spawn(async move {
            let sent = tokio::time::timeout(Duration::from_secs(5), async {
                stream.write_all(&records).await?;
                stream.shutdown().await
            })
            .await;
            if let Ok(Err(error)) = sent {
                tracing::debug!(%error, "sending the node's records failed");
            }
        });
    }
}
