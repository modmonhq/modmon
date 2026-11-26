//! The sockets a node serves its metrics on: one per address, each
//! answering `GET /metrics` to the peers its listener admits, with the
//! scope it was opened for. A peer it does not admit is closed at once,
//! before it can take one of the address's few connections.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use subtle::ConstantTimeEq as _;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::app::NodeMetricsCollector;
use crate::domain::node_metrics::{
    MAX_BODY_BYTES, MAX_REQUEST_BYTES, MetricsListener, MetricsScope, RequestProblem,
    parse_request, render,
};

/// The most scrapes one address answers at once; one more is closed
/// unanswered.
const CONNECTIONS_PER_ADDRESS: usize = 4;
/// How long a scraper may take to send its request.
const REQUEST_PATIENCE: Duration = Duration::from_secs(5);
/// How long one connection may last in all.
const CONNECTION_PATIENCE: Duration = Duration::from_secs(30);
/// How long a scraper may take to take the answer.
const WRITE_PATIENCE: Duration = Duration::from_secs(10);

/// A bearer token, when the host sets one.
pub type Token = Option<Zeroizing<Vec<u8>>>;

/// The token of each kind of scope: one the projects hold, and one of the
/// mesh's that no project holds.
#[derive(Default)]
struct Tokens {
    project: Token,
    mesh: Token,
}

type SharedListener = Arc<RwLock<Arc<MetricsListener>>>;

/// An address served: what it admits, which changes in place, and the
/// task that accepts on it.
struct Served {
    listener: SharedListener,
    local: SocketAddr,
    task: JoinHandle<()>,
}

pub struct MetricsServer {
    collector: Arc<NodeMetricsCollector>,
    port: u16,
    tokens: RwLock<Tokens>,
    served: Mutex<BTreeMap<Ipv4Addr, Served>>,
}

impl std::fmt::Debug for MetricsServer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetricsServer")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl MetricsServer {
    /// Serves on `port` of each address it is given; port 0 takes any.
    #[must_use]
    pub fn new(collector: Arc<NodeMetricsCollector>, port: u16) -> Arc<Self> {
        Arc::new(Self {
            collector,
            port,
            tokens: RwLock::new(Tokens::default()),
            served: Mutex::new(BTreeMap::new()),
        })
    }

    /// The tokens scrapes of a project's network and of the mesh send.
    pub fn set_tokens(&self, project: Token, mesh: Token) {
        *self.tokens.write().unwrap_or_else(PoisonError::into_inner) = Tokens { project, mesh };
    }

    /// Where it listens now.
    #[must_use]
    pub fn listening(&self) -> Vec<SocketAddr> {
        let served = self.served.lock().unwrap_or_else(PoisonError::into_inner);
        served.values().map(|held| held.local).collect()
    }

    /// Serves exactly `wanted`, and says what keeps it from serving one. An
    /// address it serves already keeps its socket and takes the new
    /// listener's peers and scope in place.
    pub fn serve(self: &Arc<Self>, wanted: Vec<MetricsListener>) -> Vec<String> {
        let mut problems = Vec::new();
        let mut served = self.served.lock().unwrap_or_else(PoisonError::into_inner);
        served.retain(|address, held| {
            let keep = !held.task.is_finished()
                && wanted.iter().any(|listener| listener.address == *address);
            if !keep {
                held.task.abort();
            }
            keep
        });
        for listener in wanted {
            if let Some(held) = served.get(&listener.address) {
                *held
                    .listener
                    .write()
                    .unwrap_or_else(PoisonError::into_inner) = Arc::new(listener);
                continue;
            }
            let address = SocketAddr::V4(SocketAddrV4::new(listener.address, self.port));
            let socket = crate::adapters::discovery::free_tcp(address).and_then(|socket| {
                socket.set_nonblocking(true)?;
                let socket = TcpListener::from_std(socket)?;
                let local = socket.local_addr()?;
                Ok((socket, local))
            });
            match socket {
                Ok((socket, local)) => {
                    let shared = Arc::new(RwLock::new(Arc::new(listener.clone())));
                    let task = tokio::spawn(Arc::clone(self).accept(socket, Arc::clone(&shared)));
                    served.insert(
                        listener.address,
                        Served {
                            listener: shared,
                            local,
                            task,
                        },
                    );
                }
                Err(error) => problems.push(format!(
                    "metrics: cannot listen on {address} ({error}); free that TCP port on that address, and mmd takes it within ten seconds"
                )),
            }
        }
        problems
    }

    async fn accept(self: Arc<Self>, socket: TcpListener, shared: SharedListener) {
        let slots = Arc::new(Semaphore::new(CONNECTIONS_PER_ADDRESS));
        let busy = Arc::new(Mutex::new(BTreeSet::new()));
        loop {
            let (stream, peer) = match socket.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    tracing::debug!(%error, "accepting a metrics scrape failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let listener = Arc::clone(&shared.read().unwrap_or_else(PoisonError::into_inner));
            let IpAddr::V4(address) = peer.ip() else {
                continue;
            };
            if !listener.admits(address) {
                continue;
            }
            let Some(peer_turn) = PeerTurn::take(&busy, address) else {
                continue;
            };
            let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
                continue;
            };
            let server = Arc::clone(&self);
            tokio::spawn(async move {
                let _permit = permit;
                let _peer_turn = peer_turn;
                if tokio::time::timeout(
                    CONNECTION_PATIENCE,
                    server.connection(stream, peer, &listener),
                )
                .await
                .is_err()
                {
                    tracing::debug!(%peer, "closed a metrics connection that ran past its deadline");
                }
            });
        }
    }

    async fn connection(
        &self,
        mut stream: TcpStream,
        peer: SocketAddr,
        listener: &MetricsListener,
    ) {
        let token = {
            let tokens = self.tokens.read().unwrap_or_else(PoisonError::into_inner);
            match listener.scope {
                MetricsScope::Host => tokens.mesh.clone(),
                MetricsScope::Environment { .. } => tokens.project.clone(),
            }
        };
        let scrape = Scrape {
            admitted: true,
            token: token.as_deref().map(Vec::as_slice),
            scope: &listener.scope,
        };
        if let Err(error) = answer(&mut stream, &scrape, &self.collector).await {
            tracing::debug!(%error, %peer, "answering a metrics scrape failed");
        }
    }
}

/// One connection of a peer at a time, so no peer holds more than one of an
/// address's slots.
struct PeerTurn {
    busy: Arc<Mutex<BTreeSet<Ipv4Addr>>>,
    peer: Ipv4Addr,
}

impl PeerTurn {
    fn take(busy: &Arc<Mutex<BTreeSet<Ipv4Addr>>>, peer: Ipv4Addr) -> Option<Self> {
        busy.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(peer)
            .then(|| Self {
                busy: Arc::clone(busy),
                peer,
            })
    }
}

impl Drop for PeerTurn {
    fn drop(&mut self) {
        self.busy
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.peer);
    }
}

/// Who asks, and what they may see.
#[derive(Debug, Clone, Copy)]
pub struct Scrape<'a> {
    /// Whether the peer is one the listener admits.
    pub admitted: bool,
    /// The bearer token every scrape has to send, when the host sets one.
    pub token: Option<&'a [u8]>,
    pub scope: &'a MetricsScope,
}

/// Reads one request from `stream` and answers it.
pub async fn answer<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    scrape: &Scrape<'_>,
    collector: &NodeMetricsCollector,
) -> std::io::Result<()> {
    let head = tokio::time::timeout(REQUEST_PATIENCE, read_head(stream))
        .await
        .unwrap_or(Ok(None))?;
    let request = match head.as_deref().map(parse_request) {
        None | Some(Err(RequestProblem::BadRequest)) => {
            return respond(stream, "400 Bad Request", "", false).await;
        }
        Some(Err(problem)) => return respond(stream, problem.status(), "", false).await,
        Some(Ok(request)) => request,
    };
    if !scrape.admitted {
        return respond(stream, "403 Forbidden", "", false).await;
    }
    if let Some(expected) = scrape.token {
        let given = request.bearer.as_deref().unwrap_or_default().as_bytes();
        if !bool::from(given.ct_eq(expected)) {
            return respond(stream, "401 Unauthorized", "", false).await;
        }
    }
    let snapshot = collector.snapshot().await;
    let body = render(&snapshot, scrape.scope, MAX_BODY_BYTES);
    respond(stream, "200 OK", &body, request.head).await
}

/// The request's line and headers, up to the empty line. `None` when they
/// are longer than [`MAX_REQUEST_BYTES`], not text, or cut off.
async fn read_head<S: AsyncRead + Unpin>(stream: &mut S) -> std::io::Result<Option<String>> {
    let mut head = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(None);
        }
        head.extend_from_slice(buffer.get(..read).unwrap_or_default());
        let end = head.windows(4).position(|window| window == b"\r\n\r\n");
        if end.unwrap_or(head.len()) > MAX_REQUEST_BYTES {
            return Ok(None);
        }
        if let Some(end) = end {
            head.truncate(end);
            return Ok(String::from_utf8(head).ok());
        }
    }
}

async fn respond<S: AsyncWrite + Unpin>(
    stream: &mut S,
    status: &str,
    body: &str,
    head_only: bool,
) -> std::io::Result<()> {
    let challenge = if status.starts_with("401") {
        "WWW-Authenticate: Bearer\r\n"
    } else {
        ""
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n{challenge}Connection: close\r\n\r\n",
        body.len()
    );
    let write = async {
        stream.write_all(header.as_bytes()).await?;
        if !head_only {
            stream.write_all(body.as_bytes()).await?;
        }
        stream.shutdown().await
    };
    tokio::time::timeout(WRITE_PATIENCE, write)
        .await
        .unwrap_or_else(|_| Err(std::io::ErrorKind::TimedOut.into()))
}
