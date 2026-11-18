//! Lookups that are no container's go to the host's own resolvers, the
//! `nameserver` lines of its `/etc/resolv.conf`, in the form they arrived
//! in. The file is read for every lookup, so a change takes effect at once.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};

use crate::domain::discovery::wire;

/// How long one resolver gets to answer.
const PATIENCE: Duration = Duration::from_secs(2);
/// As many resolvers as the C library reads from the file.
const SERVERS: usize = 3;

pub(super) struct Upstream {
    resolv_conf: PathBuf,
    port: u16,
}

/// What the host's resolvers made of a lookup.
pub(super) enum Forwarded {
    Reply(Vec<u8>),
    /// Every resolver that answered refuses to resolve the name.
    Refused,
    /// No resolver answered in time.
    Silent,
}

impl Upstream {
    pub(super) const fn new(resolv_conf: PathBuf, port: u16) -> Self {
        Self { resolv_conf, port }
    }

    fn servers(&self) -> Vec<SocketAddr> {
        let text = match std::fs::read_to_string(&self.resolv_conf) {
            Ok(text) => text,
            Err(error) => {
                tracing::debug!(%error, "cannot read the host's resolver configuration");
                return Vec::new();
            }
        };
        text.lines()
            .filter_map(|line| {
                let mut words = line.split_whitespace();
                (words.next() == Some("nameserver")).then(|| words.next())?
            })
            // A scoped address (`fe80::1%eth0`) is skipped.
            .filter_map(|address| address.parse::<IpAddr>().ok())
            .take(SERVERS)
            .map(|address| SocketAddr::new(address, self.port))
            .collect()
    }

    /// The first resolver's answer to `query`, over a stream when the
    /// client used one. A resolver that refuses the name is passed over
    /// for the next one, as the C library does.
    pub(super) async fn forward(&self, query: &[u8], stream: bool) -> Forwarded {
        let mut refused = false;
        for server in self.servers() {
            let asked = if stream {
                tokio::time::timeout(PATIENCE * 2, over_stream(server, query)).await
            } else {
                tokio::time::timeout(PATIENCE, over_datagram(server, query)).await
            };
            match asked {
                Ok(Ok(reply)) if wire::is_refusal(&reply) => refused = true,
                Ok(Ok(reply)) => return Forwarded::Reply(reply),
                Ok(Err(error)) => tracing::debug!(%server, %error, "a resolver of the host failed"),
                Err(_) => tracing::debug!(%server, "a resolver of the host did not answer"),
            }
        }
        if refused {
            Forwarded::Refused
        } else {
            Forwarded::Silent
        }
    }
}

/// Asks from a socket of its own under an identifier of its own, so only
/// the resolver that was asked can answer, and gives the answer the
/// client's identifier back.
async fn over_datagram(server: SocketAddr, query: &[u8]) -> std::io::Result<Vec<u8>> {
    let local: SocketAddr = if server.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(local).await?;
    socket.connect(server).await?;
    let mut random = [0_u8; 2];
    getrandom::fill(&mut random).map_err(|error| std::io::Error::other(error.to_string()))?;
    let identifier = u16::from_be_bytes(random);
    socket
        .send(&wire::with_message_id(query, identifier))
        .await?;
    let mut buffer = vec![0_u8; wire::STREAM_LIMIT];
    loop {
        let length = socket.recv(&mut buffer).await?;
        let reply = &buffer[..length];
        if wire::message_id(reply) == Some(identifier) && wire::is_response(reply) {
            let original = wire::message_id(query).unwrap_or_default();
            return Ok(wire::with_message_id(reply, original));
        }
    }
}

async fn over_stream(server: SocketAddr, query: &[u8]) -> std::io::Result<Vec<u8>> {
    let length = u16::try_from(query.len()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
    let mut stream = TcpStream::connect(server).await?;
    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(query).await?;
    let mut length = [0_u8; 2];
    stream.read_exact(&mut length).await?;
    let mut reply = vec![0_u8; usize::from(u16::from_be_bytes(length))];
    stream.read_exact(&mut reply).await?;
    Ok(reply)
}
