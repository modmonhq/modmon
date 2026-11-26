//! The sockets a node serves its metrics on: one per address the domain's
//! [`listeners`] names, each answering `GET /metrics` to the peers it
//! admits, with the scope it was opened for.

use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, SystemTime};

use subtle::ConstantTimeEq as _;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::app::NodeMetricsCollector;
use crate::app::ports::{ClusterNetworkHost, MeshHost};
use crate::domain::host_policy::HostMetrics;
use crate::domain::node_metrics::{
    MAX_BODY_BYTES, MAX_REQUEST_BYTES, METRICS_PORT, MetricsListener, MetricsScope, RequestProblem,
    listeners, parse_request, render,
};

/// How often the node looks again where it should serve.
const REFRESH: Duration = Duration::from_secs(10);
/// The most scrapes answered at once; one more is closed unanswered.
const MAX_CONNECTIONS: usize = 16;
/// How long a scraper may take to send its request.
const REQUEST_PATIENCE: Duration = Duration::from_secs(5);
/// How long one connection may last in all.
const CONNECTION_PATIENCE: Duration = Duration::from_secs(30);
const MAX_TOKEN_BYTES: u64 = 4096;

/// What decides where the node serves.
pub struct EndpointSources {
    /// `/etc/mm/host.yaml`.
    pub host_policy: PathBuf,
    pub networks: Arc<dyn ClusterNetworkHost>,
    pub mesh: Arc<dyn MeshHost>,
}

type Token = Option<Zeroizing<Vec<u8>>>;

pub struct MetricsEndpoint {
    collector: Arc<NodeMetricsCollector>,
    sources: EndpointSources,
    connections: Arc<Semaphore>,
    token: RwLock<Token>,
    policy: Mutex<Option<(SystemTime, u64, HostMetrics)>>,
    problems: Mutex<BTreeSet<String>>,
}

impl std::fmt::Debug for MetricsEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetricsEndpoint")
            .field("host_policy", &self.sources.host_policy)
            .finish_non_exhaustive()
    }
}

impl MetricsEndpoint {
    #[must_use]
    pub fn new(collector: Arc<NodeMetricsCollector>, sources: EndpointSources) -> Arc<Self> {
        Arc::new(Self {
            collector,
            sources,
            connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            token: RwLock::new(None),
            policy: Mutex::new(None),
            problems: Mutex::new(BTreeSet::new()),
        })
    }

    /// Serves until the process ends, looking every few seconds where.
    pub async fn run(self: Arc<Self>) {
        let mut serving = Vec::new();
        loop {
            let mut problems = Vec::new();
            if let Some(wanted) = self.wanted(&mut problems).await {
                self.reconcile(&mut serving, wanted, &mut problems);
            }
            self.report(problems);
            tokio::time::sleep(REFRESH).await;
        }
    }

    /// Warns about each problem once, until it goes away.
    fn report(&self, problems: Vec<String>) {
        let current = problems.into_iter().collect::<BTreeSet<_>>();
        let mut known = self.problems.lock().unwrap_or_else(PoisonError::into_inner);
        for problem in current.difference(&known) {
            tracing::warn!("{problem}");
        }
        *known = current;
    }

    fn host_metrics(&self, problems: &mut Vec<String>) -> Option<HostMetrics> {
        let path = &self.sources.host_policy;
        let metadata = std::fs::metadata(path).ok()?;
        let stamp = (
            metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            metadata.len(),
        );
        let mut known = self.policy.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((modified, length, policy)) = known.as_ref()
            && (*modified, *length) == stamp
        {
            return Some(policy.clone());
        }
        match crate::adapters::remote::load_host_policy(path) {
            Ok((host, _)) => {
                let policy = host.metrics.unwrap_or_default();
                *known = Some((stamp.0, stamp.1, policy.clone()));
                Some(policy)
            }
            Err(error) => {
                problems.push(format!(
                    "metrics: cannot read {}: {error}; the node serves no metrics until it can",
                    path.display()
                ));
                *known = None;
                None
            }
        }
    }

    async fn wanted(&self, problems: &mut Vec<String>) -> Option<Vec<MetricsListener>> {
        let Some(policy) = self.host_metrics(problems) else {
            return Some(Vec::new());
        };
        let token = match &policy.token_file {
            Some(path) => match read_token(path) {
                Ok(token) => Some(token),
                Err(problem) => {
                    problems.push(format!(
                        "metrics: {problem}; the node serves no metrics until it can"
                    ));
                    return Some(Vec::new());
                }
            },
            None => None,
        };
        *self.token.write().unwrap_or_else(PoisonError::into_inner) = token;
        if !policy.enabled {
            return Some(Vec::new());
        }
        let deployed = self.collector.deployed().await?;
        let networks = self.sources.networks.stored().ok().flatten();
        let mesh = policy
            .mesh
            .then(|| self.sources.mesh.stored().ok().flatten())
            .flatten();
        Some(listeners(
            &policy,
            networks.as_ref(),
            &deployed,
            mesh.as_ref(),
        ))
    }

    fn reconcile(
        self: &Arc<Self>,
        serving: &mut Vec<(MetricsListener, JoinHandle<()>)>,
        wanted: Vec<MetricsListener>,
        problems: &mut Vec<String>,
    ) {
        serving.retain(|(listener, task)| {
            let keep = wanted.contains(listener) && !task.is_finished();
            if !keep {
                task.abort();
            }
            keep
        });
        for listener in wanted {
            if serving.iter().any(|(known, _)| *known == listener) {
                continue;
            }
            let address = SocketAddr::V4(SocketAddrV4::new(listener.address, METRICS_PORT));
            let socket = crate::adapters::discovery::free_tcp(address).and_then(|socket| {
                socket.set_nonblocking(true)?;
                TcpListener::from_std(socket)
            });
            match socket {
                Ok(socket) => {
                    let task = tokio::spawn(Arc::clone(self).serve(socket, Arc::new(listener.clone())));
                    serving.push((listener, task));
                }
                Err(error) => problems.push(format!(
                    "metrics: cannot listen on {address} ({error}); free that TCP port on that address, and mmd takes it within ten seconds"
                )),
            }
        }
    }

    async fn serve(self: Arc<Self>, socket: TcpListener, listener: Arc<MetricsListener>) {
        loop {
            let (stream, peer) = match socket.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    tracing::debug!(%error, "accepting a metrics scrape failed");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };
            let Ok(permit) = Arc::clone(&self.connections).try_acquire_owned() else {
                continue;
            };
            let endpoint = Arc::clone(&self);
            let listener = Arc::clone(&listener);
            tokio::spawn(async move {
                let _permit = permit;
                let _ = tokio::time::timeout(
                    CONNECTION_PATIENCE,
                    endpoint.connection(stream, peer, &listener),
                )
                .await;
            });
        }
    }

    async fn connection(
        &self,
        mut stream: TcpStream,
        peer: SocketAddr,
        listener: &MetricsListener,
    ) {
        let admitted = match peer.ip() {
            IpAddr::V4(address) => listener.admits(address),
            IpAddr::V6(_) => false,
        };
        let token = self
            .token
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let scrape = Scrape {
            admitted,
            token: token.as_deref().map(Vec::as_slice),
            scope: &listener.scope,
        };
        if let Err(error) = answer(&mut stream, &scrape, &self.collector).await {
            tracing::debug!(%error, %peer, "answering a metrics scrape failed");
        }
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
    stream.write_all(header.as_bytes()).await?;
    if !head_only {
        stream.write_all(body.as_bytes()).await?;
    }
    stream.shutdown().await
}

/// The token in `path`: a regular file only root can read, of at most a
/// few kilobytes. Never says what it holds.
fn read_token(path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot read metrics.token-file {} ({error})",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() > MAX_TOKEN_BYTES {
        return Err(format!(
            "metrics.token-file {} must be a regular file of at most {MAX_TOKEN_BYTES} bytes, not a link",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        if metadata.uid() != rustix::process::getuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            let shown = path.display();
            return Err(format!(
                "metrics.token-file {shown} must belong to root and be readable by root only: chown root: {shown} && chmod 600 {shown}"
            ));
        }
    }
    let content = Zeroizing::new(std::fs::read(path).map_err(|error| {
        format!(
            "cannot read metrics.token-file {} ({error})",
            path.display()
        )
    })?);
    let token = Zeroizing::new(content.trim_ascii().to_vec());
    if token.is_empty() {
        return Err(format!("metrics.token-file {} is empty", path.display()));
    }
    Ok(token)
}
