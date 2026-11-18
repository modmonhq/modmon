//! The sockets of a node's name resolver: lookups on each cluster network's
//! gateway address, and the exchange between nodes on the mesh address.
//! Never on any other address: a socket here is bound to exactly one.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::task::JoinHandle;

use super::NameResolver;
use crate::domain::discovery::wire::{DATAGRAM_LIMIT, STREAM_LIMIT};

/// A stream that sends nothing for this long is closed.
const IDLE: Duration = Duration::from_secs(5);
/// Room for a query that announces a large datagram size.
const DATAGRAM_BUFFER: usize = 4096;

#[derive(Default)]
pub(super) struct Listeners {
    lookups: BTreeMap<Ipv4Addr, [JoinHandle<()>; 2]>,
    exchange: Option<(Ipv4Addr, JoinHandle<()>)>,
}

impl Listeners {
    /// Stops listening everywhere, also across a restart of `mmd`.
    pub(super) fn close(&mut self, resolver: &NameResolver) {
        for (address, tasks) in std::mem::take(&mut self.lookups) {
            tasks.iter().for_each(JoinHandle::abort);
            resolver.sockets.forget(lookup_address(resolver, address));
        }
        if let Some((address, task)) = self.exchange.take() {
            task.abort();
            resolver.sockets.forget(exchange_address(resolver, address));
        }
    }

    /// Listens for lookups on each of `gateways` and for the other nodes on
    /// `mesh`, and stops listening on addresses that are no longer wanted.
    /// A gateway is listened on before the host has its address
    /// ([`lookup_sockets`]); the mesh address the host does not have yet
    /// is tried again at the next call. Returns whether an address is
    /// still waited for, and what keeps the node from listening on an
    /// address.
    pub(super) async fn bind(
        &mut self,
        resolver: &Arc<NameResolver>,
        gateways: &[Ipv4Addr],
        mesh: Option<Ipv4Addr>,
    ) -> (bool, Option<String>) {
        self.lookups.retain(|address, tasks| {
            let keep = gateways.contains(address) && tasks.iter().all(|task| !task.is_finished());
            if !keep {
                tasks.iter().for_each(JoinHandle::abort);
                resolver.sockets.forget(lookup_address(resolver, *address));
            }
            keep
        });
        if self
            .exchange
            .as_ref()
            .is_some_and(|(address, task)| Some(*address) != mesh || task.is_finished())
            && let Some((address, task)) = self.exchange.take()
        {
            task.abort();
            resolver.sockets.forget(exchange_address(resolver, address));
        }
        let mut problems = Vec::new();
        let mut waiting = false;
        for gateway in gateways {
            if self.lookups.contains_key(gateway) {
                continue;
            }
            let address = lookup_address(resolver, *gateway);
            match lookup_sockets(resolver, address) {
                Ok((datagrams, streams)) => {
                    let tasks = [
                        tokio::spawn(serve_datagrams(Arc::clone(resolver), Arc::new(datagrams))),
                        tokio::spawn(serve_streams(Arc::clone(resolver), streams)),
                    ];
                    self.lookups.insert(*gateway, tasks);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrNotAvailable => waiting = true,
                Err(error) => problems.push(format!(
                    "cannot answer names on {address} ({error}), so containers on that network resolve nothing. If another DNS server on this host listens on every address, bind it to its own addresses; mmd takes the port within ten seconds"
                )),
            }
        }
        if let (Some(mesh), None) = (mesh, &self.exchange) {
            let address = exchange_address(resolver, mesh);
            match stream_socket(resolver, address) {
                Ok(listener) => {
                    let task = tokio::spawn(super::exchange::serve(Arc::clone(resolver), listener));
                    self.exchange = Some((mesh, task));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrNotAvailable => waiting = true,
                Err(error) => problems.push(format!(
                    "cannot tell the other nodes what runs here on {address} ({error}), so this node's containers are missing from their answers. Free that TCP port on the mesh address; mmd takes it within ten seconds"
                )),
            }
        }
        (waiting, (!problems.is_empty()).then(|| problems.join("; ")))
    }
}

fn lookup_address(resolver: &NameResolver, gateway: Ipv4Addr) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(gateway, resolver.options.lookup_port))
}

fn exchange_address(resolver: &NameResolver, mesh: Ipv4Addr) -> SocketAddr {
    SocketAddr::V4(SocketAddrV4::new(mesh, resolver.options.exchange_port))
}

/// The two sockets lookups arrive on at `address`: the ones the previous
/// `mmd` left, or new ones bound before the host has the address
/// ([`free_udp`]), kept for the next. Neither is kept when one of them
/// cannot be had.
fn lookup_sockets(
    resolver: &NameResolver,
    address: SocketAddr,
) -> std::io::Result<(UdpSocket, TcpListener)> {
    let datagrams = resolver
        .sockets
        .udp_bound_by(address, free_udp)
        .and_then(|socket| {
            socket.set_nonblocking(true)?;
            UdpSocket::from_std(socket)
        });
    let sockets =
        datagrams.and_then(|datagrams| Ok((datagrams, stream_socket(resolver, address)?)));
    if sockets.is_err() {
        resolver.sockets.forget(address);
    }
    sockets
}

fn stream_socket(resolver: &NameResolver, address: SocketAddr) -> std::io::Result<TcpListener> {
    let listener = resolver.sockets.tcp_bound_by(address, free_tcp)?;
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

/// A UDP socket bound to `address` before the host has it (`IP_FREEBIND`):
/// Podman creates a network's bridge, and with it the gateway address, with
/// the network's first container, which asks for a name as soon as it
/// starts. Bound only once the address existed, those first lookups went
/// unanswered and the container's first request by name failed. The socket
/// answers as soon as the address is there, and is bound to it alone.
/// Blocking, as a new socket of [`KeptSockets`](crate::adapters::kept_sockets::KeptSockets).
pub fn free_udp(address: SocketAddr) -> std::io::Result<std::net::UdpSocket> {
    let socket = free_socket(rustix::net::SocketType::DGRAM)?;
    rustix::net::bind(&socket, &address)?;
    Ok(std::net::UdpSocket::from(socket))
}

/// A listening TCP socket bound as [`free_udp`].
pub fn free_tcp(address: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let socket = free_socket(rustix::net::SocketType::STREAM)?;
    rustix::net::sockopt::set_socket_reuseaddr(&socket, true)?;
    rustix::net::bind(&socket, &address)?;
    rustix::net::listen(&socket, 1024)?;
    Ok(std::net::TcpListener::from(socket))
}

/// An IPv4 socket that may bind an address the host does not have yet.
fn free_socket(kind: rustix::net::SocketType) -> std::io::Result<std::os::fd::OwnedFd> {
    let socket = rustix::net::socket_with(
        rustix::net::AddressFamily::INET,
        kind,
        rustix::net::SocketFlags::CLOEXEC,
        None,
    )?;
    #[cfg(target_os = "linux")]
    rustix::net::sockopt::set_ip_freebind(&socket, true)?;
    Ok(socket)
}

async fn serve_datagrams(resolver: Arc<NameResolver>, socket: Arc<UdpSocket>) {
    let mut buffer = vec![0_u8; DATAGRAM_BUFFER];
    loop {
        let (length, peer) = match socket.recv_from(&mut buffer).await {
            Ok(received) => received,
            Err(error) => {
                tracing::debug!(%error, "receiving a name lookup failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(permit) = Arc::clone(&resolver.in_flight).try_acquire_owned() else {
            continue;
        };
        let message = buffer[..length].to_vec();
        let (resolver, socket) = (Arc::clone(&resolver), Arc::clone(&socket));
        tokio::spawn(async move {
            if let Some(reply) = resolver.reply(&message, peer.ip(), DATAGRAM_LIMIT).await
                && let Err(error) = socket.send_to(&reply, peer).await
            {
                tracing::debug!(%error, "sending a name answer failed");
            }
            drop(permit);
        });
    }
}

async fn serve_streams(resolver: Arc<NameResolver>, listener: TcpListener) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(%error, "accepting a name lookup stream failed");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        let Ok(permit) = Arc::clone(&resolver.in_flight).try_acquire_owned() else {
            continue;
        };
        let resolver = Arc::clone(&resolver);
        tokio::spawn(async move {
            if let Err(error) = serve_stream(&resolver, stream, peer).await {
                tracing::debug!(%error, "a name lookup stream ended early");
            }
            drop(permit);
        });
    }
}

/// Lookups over a stream: each message and each answer follows a two-byte
/// length. The stream stays open for more until the client closes it or
/// goes quiet.
async fn serve_stream(
    resolver: &NameResolver,
    mut stream: TcpStream,
    peer: SocketAddr,
) -> std::io::Result<()> {
    loop {
        let mut length = [0_u8; 2];
        match tokio::time::timeout(IDLE, stream.read_exact(&mut length)).await {
            Ok(Ok(_)) => {}
            // The client is done, or went quiet.
            Ok(Err(_)) | Err(_) => return Ok(()),
        }
        let mut message = vec![0_u8; usize::from(u16::from_be_bytes(length))];
        tokio::time::timeout(IDLE, stream.read_exact(&mut message))
            .await
            .map_err(|_| std::io::ErrorKind::TimedOut)??;
        let Some(reply) = resolver.reply(&message, peer.ip(), STREAM_LIMIT).await else {
            return Ok(());
        };
        let Ok(length) = u16::try_from(reply.len()) else {
            return Ok(());
        };
        stream.write_all(&length.to_be_bytes()).await?;
        stream.write_all(&reply).await?;
    }
}
