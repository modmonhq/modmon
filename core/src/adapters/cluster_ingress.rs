//! The ingress proxy of a cluster node and the keeper of its file
//! (`domain::cluster_ingress`).
//!
//! Under `/var/lib/mm/ingress` the node keeps one table of routes per
//! project's environment (`routes/`), the proxy's Compose file, and the
//! directories the proxy mounts: the one file it reads and watches
//! (`dynamic/mm.yaml`), provided certificates, and its ACME state. The
//! keeper writes that one file from the tables and the node's service map,
//! again whenever either changes, and tells the resolver which answers the
//! proxy runs once the proxy says it loaded the file, which is what a
//! leaving container waits for.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

use crate::adapters::discovery::NameResolver;
use crate::adapters::node_files::{
    directory, engine_program, failure, names, no_such_container, read_whole, remove, write_whole,
};
use crate::app::ports::{IngressHost, ProcessRunner, ServiceNames};
use crate::domain::cluster_ingress::{
    Backends, CERTIFICATES_DIRECTORY, CHECK_PORT, COMPOSE_FILE, CONNECTOR_USER, DYNAMIC_DIRECTORY,
    DYNAMIC_FILE, IngressTable, METRICS_PORTS, PROXY_PROJECT, TABLES_DIRECTORY, TunnelStatus,
    loaded_path, render_dynamic,
};
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;
use crate::domain::stored_state::{StoredOn, unknown_in, written_by_newer_mm};

#[path = "cluster_ingress/acme.rs"]
mod acme;
pub use acme::forget_provided_certificates;
#[path = "cluster_ingress/proxy.rs"]
mod proxy;
#[path = "cluster_ingress/tunnel.rs"]
mod tunnel;
pub use tunnel::{ready_connections, tunnel_directory, write_connector_files};

/// Where a node keeps its ingress, under the daemon's state directory.
pub const INGRESS_DIRECTORY: &str = "ingress";

/// The proxy's container, as Compose names it.
const PROXY_CONTAINER: &str = "mm-ingress-proxy-1";
/// How long the keeper waits for the proxy to load a new file, and an
/// apply for the keeper to have it loaded.
const LOAD_PATIENCE: Duration = Duration::from_secs(20);
const APPLY_PATIENCE: Duration = Duration::from_secs(60);
/// How long the proxy may take to start, its image pulled included.
const START_PATIENCE: Duration = Duration::from_secs(300);
/// How often the keeper looks when nothing tells it to.
const LOOK_AGAIN: Duration = Duration::from_secs(2);
/// How long a tunnel connector may take to connect to Cloudflare.
const READY_PATIENCE: Duration = Duration::from_secs(90);

/// Where the proxy is, what it runs, and the engine that runs it; tests use
/// another port and image.
#[derive(Debug, Clone)]
pub struct IngressOptions {
    pub root: PathBuf,
    pub image: String,
    pub check_port: u16,
    /// The engine setup installed on the node.
    pub engine: RuntimeKind,
    /// The user that owns a tunnel connector's files; `None` leaves them
    /// to whoever runs `mmd`.
    pub connector_user: Option<u32>,
    /// The loopback ports tunnel connectors answer their readiness on.
    pub metrics_ports: std::ops::RangeInclusive<u16>,
    pub ready_patience: Duration,
}

impl IngressOptions {
    #[must_use]
    pub fn on_node(state_root: &Path, engine: RuntimeKind) -> Self {
        Self {
            root: state_root.join(INGRESS_DIRECTORY),
            image: crate::domain::target::DEFAULT_TRAEFIK_IMAGE.to_owned(),
            check_port: CHECK_PORT,
            engine,
            connector_user: Some(CONNECTOR_USER),
            metrics_ports: METRICS_PORTS,
            ready_patience: READY_PATIENCE,
        }
    }

    /// Where the proxy's own words are, for an error message.
    fn proxy_log(&self) -> String {
        format!("`{} logs {PROXY_CONTAINER}`", engine_program(self.engine))
    }
}

pub struct IngressKeeper {
    options: IngressOptions,
    names: Arc<NameResolver>,
    process: Arc<dyn ProcessRunner>,
    /// Counts each change of the tables an apply waits for.
    asked: AtomicU64,
    /// The last count whose tables the proxy runs.
    served: tokio::sync::watch::Sender<u64>,
    changed: Notify,
}

impl std::fmt::Debug for IngressKeeper {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("IngressKeeper")
            .field("root", &self.options.root)
            .finish_non_exhaustive()
    }
}

/// What the keeper wrote last, and whether the proxy loaded it.
#[derive(Default)]
struct Written {
    input: Option<(Vec<(String, IngressTable)>, Backends)>,
    marker: u64,
}

impl IngressKeeper {
    #[must_use]
    pub fn new(
        options: IngressOptions,
        names: Arc<NameResolver>,
        process: Arc<dyn ProcessRunner>,
    ) -> Arc<Self> {
        Arc::new(Self {
            options,
            names,
            process,
            asked: AtomicU64::new(0),
            served: tokio::sync::watch::Sender::new(0),
            changed: Notify::new(),
        })
    }

    /// Called before the resolver answers anyone: a node that serves routes
    /// confirms no leave until the proxy runs a file this `mmd` wrote,
    /// because the proxy still runs the last one of the `mmd` before.
    pub fn restore(&self) -> Result<usize, MmError> {
        let tables = self.tables()?;
        if !tables.is_empty() {
            self.names.proxy_runs(Some(0));
        }
        Ok(tables.len())
    }

    /// Keeps the proxy's file up to date until the process ends.
    pub async fn run(self: Arc<Self>) {
        // A file written from an empty map would take every backend away.
        self.names.settled().await;
        let mut generation = self.names.generation();
        let mut written = Written::default();
        loop {
            let asked = self.asked.load(Ordering::SeqCst);
            match self.refresh(&mut written).await {
                Ok(()) => {
                    self.served.send_replace(asked);
                }
                Err(error) => {
                    tracing::warn!(%error, "the ingress proxy does not run the newest routes")
                }
            }
            tokio::select! {
                changed = generation.changed() => {
                    if changed.is_err() {
                        tokio::time::sleep(LOOK_AGAIN).await;
                    }
                }
                () = self.changed.notified() => {}
                () = tokio::time::sleep(LOOK_AGAIN) => {}
            }
        }
    }

    /// Writes the proxy's file when the routes or the containers they send
    /// to changed, and waits until the proxy runs it.
    async fn refresh(&self, written: &mut Written) -> Result<(), MmError> {
        let tables = self.tables()?;
        if tables.is_empty() {
            self.names.proxy_runs(None);
            written.input = None;
            return Ok(());
        }
        // The answers the file follows: taken before the backends are read.
        let generation = *self.names.generation().borrow();
        let mut backends = Backends::new();
        for (_, table) in &tables {
            for route in &table.routes {
                let (addresses, _) =
                    self.names
                        .addresses(&table.network, &route.service, route.balance.reach());
                backends.insert(route.router.clone(), addresses);
            }
        }
        let input = (tables, backends);
        if written.input.as_ref() != Some(&input) {
            written.marker = written.marker.saturating_add(1);
            let text = render_dynamic(&input.0, &input.1, written.marker)
                .map_err(|message| MmError::State { message })?;
            let dynamic = self.options.root.join(DYNAMIC_DIRECTORY);
            directory(&dynamic, 0o755)?;
            write_whole(&dynamic.join(DYNAMIC_FILE), text.as_bytes(), 0o644)?;
            self.wait_loaded(written.marker).await?;
            written.input = Some(input);
        }
        self.names.proxy_runs(Some(generation));
        Ok(())
    }

    /// Waits until the proxy answers the path of `marker`: it runs the file
    /// that carries it.
    async fn wait_loaded(&self, marker: u64) -> Result<(), MmError> {
        let path = loaded_path(marker);
        let deadline = tokio::time::Instant::now() + LOAD_PATIENCE;
        loop {
            if self.answers(&path).await {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MmError::RuntimeResponse {
                    message: format!(
                        "the ingress proxy did not load its new routes within {} s; {} on the node says why",
                        LOAD_PATIENCE.as_secs(),
                        self.options.proxy_log()
                    ),
                });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn answers(&self, path: &str) -> bool {
        answers(self.options.check_port, path).await
    }

    fn tables(&self) -> Result<Vec<(String, IngressTable)>, MmError> {
        read_tables(&self.options.root)
    }

    /// Has the keeper look now, and waits until the proxy runs the tables
    /// as they are now.
    async fn served_now(&self) -> Result<(), MmError> {
        let wanted = self.asked.fetch_add(1, Ordering::SeqCst).saturating_add(1);
        self.changed.notify_one();
        let mut served = self.served.subscribe();
        tokio::time::timeout(APPLY_PATIENCE, served.wait_for(|served| *served >= wanted))
            .await
            .map_err(|_| MmError::RuntimeResponse {
                message: format!(
                    "the ingress proxy of this node does not run the new routes after {} s; {} on the node says why",
                    APPLY_PATIENCE.as_secs(),
                    self.options.proxy_log()
                ),
            })?
            .map(drop)
            .map_err(|_| MmError::State {
                message: "the ingress keeper of this node stopped".to_owned(),
            })
    }

    async fn engine(&self, args: &[&str], timeout: Duration) -> Result<String, MmError> {
        let output = crate::adapters::node_files::engine(
            self.process.as_ref(),
            self.options.engine,
            args,
            timeout,
        )
        .await?;
        if output.success() {
            Ok(output.stdout.to_string())
        } else {
            Err(MmError::RuntimeResponse {
                message: format!(
                    "`{} {}` failed: {}",
                    engine_program(self.options.engine),
                    args.join(" "),
                    failure(&output)
                ),
            })
        }
    }

    fn compose_args<'a>(&'a self, compose: &'a str, rest: &[&'a str]) -> Vec<&'a str> {
        let mut args = vec![
            "compose",
            "--project-name",
            PROXY_PROJECT,
            "--file",
            compose,
        ];
        args.extend_from_slice(rest);
        args
    }
}

#[async_trait::async_trait]
impl IngressHost for IngressKeeper {
    async fn ensure_proxy(&self, table: &str, routes: &IngressTable) -> Result<(), MmError> {
        self.converge(Some((table, routes)), true).await
    }

    fn keep_certificate(
        &self,
        table: &str,
        certificate: &[u8],
        key: &[u8],
    ) -> Result<String, MmError> {
        let mut digest = blake3::Hasher::new();
        digest.update(certificate);
        digest.update(key);
        let stem = format!("{table}-{}", &digest.finalize().to_hex()[..12]);
        let certificates = self.options.root.join(CERTIFICATES_DIRECTORY);
        directory(&certificates, 0o700)?;
        write_whole(
            &certificates.join(format!("{stem}.crt")),
            certificate,
            0o600,
        )?;
        write_whole(&certificates.join(format!("{stem}.key")), key, 0o600)?;
        Ok(stem)
    }

    async fn publish(
        &self,
        table: &str,
        routes: &IngressTable,
        token: Option<&[u8]>,
    ) -> Result<(), MmError> {
        // Ports 80 and 443 belong to every environment on the node: two of
        // them must not ask for the same requests.
        for (other, served) in self.tables()? {
            if other == table {
                continue;
            }
            if let Some(route) = routes.routes.iter().find(|route| {
                served.routes.iter().any(|taken| {
                    taken.host == route.host && paths_overlap(&taken.path, &route.path)
                })
            }) {
                return Err(MmError::Environment {
                    message: format!(
                        "{} is already routed on this node for project environment {other}; give one of the two environments another domain",
                        route.url()
                    ),
                });
            }
        }
        let tables = self.options.root.join(TABLES_DIRECTORY);
        directory(&tables, 0o700)?;
        let path = tables.join(format!("{table}.json"));
        let previous = read_whole(&path)?;
        let (routes, held) = self.with_tunnel(table, routes, previous.as_deref(), token)?;
        let routes = &routes;
        let bytes = serde_json::to_vec_pretty(routes).map_err(|error| MmError::State {
            message: format!("cannot write the routes of {table}: {error}"),
        })?;
        write_whole(&path, &bytes, 0o600)?;
        if let Err(error) = self.served_now().await {
            // The proxy keeps what it served before.
            match previous {
                Some(previous) => write_whole(&path, &previous, 0o600)?,
                None => {
                    remove(&path)?;
                }
            }
            self.changed.notify_one();
            return Err(error);
        }
        // Certificates no table names any more.
        let certificates = self.options.root.join(CERTIFICATES_DIRECTORY);
        let kept = routes.certificate.as_deref().unwrap_or_default();
        for name in names(&certificates, "")? {
            let stem = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(stem, _)| stem);
            if stem.starts_with(&format!("{table}-")) && stem != kept {
                remove(&certificates.join(&name))?;
            }
        }
        // A host that provides its certificate now: the proxy must not
        // keep the one ACME gave it before.
        let forgotten = forget_provided_certificates(&self.options, self.process.as_ref()).await?;
        if !forgotten.is_empty() {
            tracing::info!(domains = ?forgotten, "the ingress proxy forgot ACME certificates");
        }
        self.serve_tunnel(table, routes, token, (previous, held))
            .await
    }

    async fn withdraw(&self, table: &str, network: Option<&str>) -> Result<(), MmError> {
        let tables = self.options.root.join(TABLES_DIRECTORY);
        if remove(&tables.join(format!("{table}.json")))? {
            self.served_now().await?;
        }
        let certificates = self.options.root.join(CERTIFICATES_DIRECTORY);
        for name in names(&certificates, "")? {
            if name.starts_with(&format!("{table}-")) {
                remove(&certificates.join(&name))?;
            }
        }
        let compose = self.options.root.join(COMPOSE_FILE);
        if read_whole(&compose)?.is_none() {
            return Ok(());
        }
        if let Some(network) = network {
            let output = crate::adapters::node_files::engine(
                self.process.as_ref(),
                self.options.engine,
                &["network", "disconnect", network, PROXY_CONTAINER],
                Duration::from_secs(60),
            )
            .await?;
            let gone = output.stderr.contains("is not connected")
                || no_such_container(&output)
                || output.stderr.contains("not found");
            if !output.success() && !gone {
                return Err(MmError::RuntimeResponse {
                    message: format!(
                        "could not take the ingress proxy off network {network}: {}",
                        failure(&output)
                    ),
                });
            }
        }
        // A node that serves no environment runs no proxy.
        if self.tables()?.is_empty() {
            let compose = compose.to_string_lossy().into_owned();
            self.engine(
                &self.compose_args(&compose, &["down", "--remove-orphans"]),
                START_PATIENCE,
            )
            .await?;
            return tunnel::remove_stale_tunnels(&self.options.root, &[]);
        }
        self.converge(None, false).await?;
        tunnel::remove_stale_tunnels(&self.options.root, &self.tables()?)
    }

    async fn tunnel_status(&self, table: &str) -> Option<TunnelStatus> {
        self.connector_status(table).await
    }

    fn public_tables(&self, except: &str) -> Result<Vec<String>, MmError> {
        Ok(self
            .tables()?
            .into_iter()
            .filter(|(name, table)| {
                name != except && table.tunnel.is_none() && !table.routes.is_empty()
            })
            .map(|(name, _)| name)
            .collect())
    }
}

/// Whether the proxy answers `path` with 200 on its check port `port`.
async fn answers(port: u16, path: &str) -> bool {
    let asked = async {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes())
            .await?;
        let mut head = [0_u8; 12];
        stream.read_exact(&mut head).await?;
        Ok::<_, std::io::Error>(head.starts_with(b"HTTP/1.") && head.ends_with(b" 200"))
    };
    matches!(
        tokio::time::timeout(Duration::from_secs(2), asked).await,
        Ok(Ok(true))
    )
}

/// Every environment's table under the ingress directory `root`, by name.
fn read_tables(root: &Path) -> Result<Vec<(String, IngressTable)>, MmError> {
    let tables = root.join(TABLES_DIRECTORY);
    let mut read = Vec::new();
    for name in names(&tables, ".json")? {
        let path = tables.join(&name);
        let Some(bytes) = read_whole(&path)? else {
            continue;
        };
        match serde_json::from_slice::<IngressTable>(&bytes) {
            Ok(table) => read.push((name.trim_end_matches(".json").to_owned(), table)),
            Err(error) => {
                if let Some(unknown) = unknown_in(&error) {
                    return Err(written_by_newer_mm(&path, &unknown, StoredOn::Host));
                }
                tracing::warn!(file = %path.display(), %error, "an ingress table the keeper cannot read is left out");
            }
        }
    }
    Ok(read)
}

/// Whether one route path takes requests of the other: `/` takes all,
/// `/api` takes `/api` and `/api/...`.
fn paths_overlap(left: &str, right: &str) -> bool {
    let contains = |prefix: &str, path: &str| {
        prefix == "/"
            || path == prefix
            || path
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    contains(left, right) || contains(right, left)
}
