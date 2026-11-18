//! The Cloudflare Tunnel connectors of an ingress node
//! (`domain::cluster_ingress::tunnel`): their configuration and token
//! files, and whether each one holds connections to Cloudflare.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{IngressKeeper, IngressOptions, PROXY_CONTAINER};
use crate::adapters::node_files::{directory, read_whole, remove, write_whole};
use crate::domain::cluster_ingress::{
    CONNECTOR_CONFIG, CONNECTOR_TOKEN, IngressTable, TABLES_DIRECTORY, TUNNELS_DIRECTORY,
    TunnelStatus, connector_config, connector_service, metrics_port,
};
use crate::domain::error::MmError;

/// The largest answer of a connector's readiness endpoint mm reads.
const READY_BYTES: usize = 4096;

/// Where the connector of `table` finds its configuration and token.
#[must_use]
pub fn tunnel_directory(root: &Path, table: &str) -> PathBuf {
    root.join(TUNNELS_DIRECTORY).join(table)
}

/// The token `table`'s connector reads now.
pub(super) fn held_token(root: &Path, table: &str) -> Result<Option<Vec<u8>>, MmError> {
    read_whole(&tunnel_directory(root, table).join(CONNECTOR_TOKEN))
}

/// Writes the configuration of `table`'s connector from its routes, and
/// its token, readable by the connector's user alone.
pub fn write_connector_files(
    options: &IngressOptions,
    table: &str,
    routes: &IngressTable,
    token: &[u8],
) -> Result<(), MmError> {
    let tunnels = options.root.join(TUNNELS_DIRECTORY);
    directory(&tunnels, 0o755)?;
    let own = tunnel_directory(&options.root, table);
    directory(&own, 0o700)?;
    let config = connector_config(&routes.routes).map_err(|message| MmError::State { message })?;
    write_whole(&own.join(CONNECTOR_CONFIG), config.as_bytes(), 0o400)?;
    write_whole(&own.join(CONNECTOR_TOKEN), token.trim_ascii(), 0o400)?;
    if let Some(user) = options.connector_user {
        for path in [own.join(CONNECTOR_CONFIG), own.join(CONNECTOR_TOKEN), own] {
            std::os::unix::fs::chown(&path, Some(user), Some(user)).map_err(|source| {
                MmError::Io {
                    context: format!("giving {} to the tunnel connector's user", path.display()),
                    source,
                }
            })?;
        }
    }
    Ok(())
}

/// Takes away the files of every connector no table has a tunnel for.
pub(super) fn remove_stale_tunnels(
    root: &Path,
    tables: &[(String, IngressTable)],
) -> Result<(), MmError> {
    let tunnels = root.join(TUNNELS_DIRECTORY);
    let entries = match std::fs::read_dir(&tunnels) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(MmError::Io {
                context: format!("listing {}", tunnels.display()),
                source,
            });
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let served = tables
            .iter()
            .any(|(table, routes)| *table == name && routes.tunnel.is_some());
        if !served {
            std::fs::remove_dir_all(entry.path()).map_err(|source| MmError::Io {
                context: format!("removing {}", entry.path().display()),
                source,
            })?;
        }
    }
    Ok(())
}

/// The connections the connector answering on `port` holds to Cloudflare,
/// as its readiness endpoint says; `None` when nothing answers there.
pub async fn ready_connections(port: u16) -> Option<u32> {
    let asked = async {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
        stream
            .write_all(b"GET /ready HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .await?;
        let mut answer = Vec::new();
        stream
            .take(u64::try_from(READY_BYTES).unwrap_or(u64::MAX))
            .read_to_end(&mut answer)
            .await?;
        Ok::<_, std::io::Error>(answer)
    };
    let answer = tokio::time::timeout(Duration::from_secs(2), asked)
        .await
        .ok()?
        .ok()?;
    let text = String::from_utf8_lossy(&answer);
    let (head, body) = text.split_once("\r\n\r\n")?;
    let ready = head.starts_with("HTTP/1.") && head.split(' ').nth(1) == Some("200");
    let counted = serde_json::from_str::<serde_json::Value>(body.trim())
        .ok()
        .and_then(|value| value.get("readyConnections")?.as_u64())
        .and_then(|count| u32::try_from(count).ok());
    Some(match (ready, counted) {
        (true, Some(count)) => count.max(1),
        (true, None) => 1,
        (false, _) => 0,
    })
}

/// What a publish goes back to when the connector does not connect: the
/// table and the token the node held before.
type Held = (Option<Vec<u8>>, Option<Vec<u8>>);

impl IngressKeeper {
    /// `routes` with the metrics port and token generation of its tunnel,
    /// and the token the node holds now.
    pub(super) fn with_tunnel(
        &self,
        table: &str,
        routes: &IngressTable,
        previous: Option<&[u8]>,
        token: Option<&[u8]>,
    ) -> Result<(IngressTable, Option<Vec<u8>>), MmError> {
        let mut routes = routes.clone();
        let held = held_token(&self.options.root, table)?;
        let (Some(tunnel), Some(token)) = (routes.tunnel.as_mut(), token) else {
            return Ok((routes, held));
        };
        let previous =
            previous.and_then(|bytes| serde_json::from_slice::<IngressTable>(bytes).ok());
        let port = metrics_port(
            table,
            previous.as_ref(),
            &self.tables()?,
            self.options.metrics_ports.clone(),
        )
        .ok_or_else(|| MmError::State {
            message: format!(
                "every loopback port from {} to {} holds another tunnel connector on this node",
                self.options.metrics_ports.start(),
                self.options.metrics_ports.end()
            ),
        })?;
        let generation = previous
            .and_then(|previous| previous.tunnel)
            .map_or(0, |tunnel| tunnel.generation);
        tunnel.metrics_port = Some(port);
        tunnel.generation = if held.as_deref().map(<[u8]>::trim_ascii) == Some(token.trim_ascii()) {
            generation
        } else {
            generation.saturating_add(1)
        };
        Ok((routes, held))
    }

    /// Runs the connectors the tables call for and, when `routes` come in
    /// through a tunnel, waits until its connector connects; a connector
    /// that does not goes back to what the node held before.
    pub(super) async fn serve_tunnel(
        &self,
        table: &str,
        routes: &IngressTable,
        token: Option<&[u8]>,
        held: Held,
    ) -> Result<(), MmError> {
        if let (Some(_), Some(token)) = (&routes.tunnel, token) {
            write_connector_files(&self.options, table, routes, token)?;
        }
        let port = routes
            .tunnel
            .as_ref()
            .and_then(|tunnel| tunnel.metrics_port);
        let started = self.converge(None, false).await;
        let connected = match (started, port) {
            (Ok(()), Some(port)) => self.wait_connected(table, port).await,
            (started, _) => started,
        };
        if let Err(error) = connected {
            if routes.tunnel.is_some() {
                self.put_back(table, held).await;
            }
            return Err(error);
        }
        remove_stale_tunnels(&self.options.root, &self.tables()?)
    }

    /// Puts back the table and token the node held before a connector that
    /// did not connect, and the connectors they call for.
    async fn put_back(&self, table: &str, (previous, token): Held) {
        let path = self
            .options
            .root
            .join(TABLES_DIRECTORY)
            .join(format!("{table}.json"));
        let earlier = previous
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<IngressTable>(bytes).ok());
        let restored = match &previous {
            Some(bytes) => write_whole(&path, bytes, 0o600),
            None => remove(&path).map(drop),
        }
        .and_then(|()| match (&earlier, &token) {
            (Some(earlier), Some(token)) if earlier.tunnel.is_some() => {
                write_connector_files(&self.options, table, earlier, token)
            }
            _ => Ok(()),
        });
        self.changed.notify_one();
        let converged = match restored {
            Ok(()) => self.converge(None, false).await,
            Err(error) => Err(error),
        };
        if let Err(error) =
            converged.and_then(|()| remove_stale_tunnels(&self.options.root, &self.tables()?))
        {
            tracing::warn!(%error, "the node could not go back to the tunnel connector it ran before");
        }
    }

    /// Waits until the connector of `table` holds a connection to
    /// Cloudflare.
    pub(super) async fn wait_connected(&self, table: &str, port: u16) -> Result<(), MmError> {
        let deadline = tokio::time::Instant::now() + self.options.ready_patience;
        loop {
            if ready_connections(port).await.unwrap_or_default() > 0 {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(MmError::Remote {
                    code: "MM_TUNNEL_NOT_CONNECTED",
                    message: format!(
                        "the Cloudflare Tunnel connector of this node holds no connection to Cloudflare after {} s; this node went back to the routes and connector it ran before. {} on the node says why; store a token Cloudflare refuses again with `mm secret set` and deploy again",
                        self.options.ready_patience.as_secs(),
                        self.options.connector_log(table)
                    ),
                });
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// What the connector of `table` says, when the node serves the
    /// environment through a tunnel.
    pub(super) async fn connector_status(&self, table: &str) -> Option<TunnelStatus> {
        let tables = self.tables().ok()?;
        let (_, routes) = tables.iter().find(|(name, _)| name == table)?;
        let tunnel = routes.tunnel.as_ref()?;
        let connections = match tunnel.metrics_port {
            Some(port) => ready_connections(port).await,
            None => None,
        };
        Some(TunnelStatus {
            token: tunnel.token.clone(),
            connections,
            public_ports: self.publishes_publicly().await,
        })
    }

    /// Whether the proxy has a port open on an address other than the
    /// loopback one.
    async fn publishes_publicly(&self) -> bool {
        let Ok(bindings) = self
            .engine(
                &[
                    "inspect",
                    "--type",
                    "container",
                    "--format",
                    "{{json .HostConfig.PortBindings}}",
                    PROXY_CONTAINER,
                ],
                Duration::from_secs(10),
            )
            .await
        else {
            return false;
        };
        let Ok(bindings) = serde_json::from_str::<serde_json::Value>(bindings.trim()) else {
            return false;
        };
        bindings
            .as_object()
            .into_iter()
            .flat_map(|ports| ports.values())
            .filter_map(serde_json::Value::as_array)
            .flatten()
            .any(|binding| {
                let address = binding
                    .get("HostIp")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                !matches!(address, "127.0.0.1" | "::1" | "localhost")
            })
    }
}

impl IngressOptions {
    /// Where a connector's own words are, for an error message.
    fn connector_log(&self, table: &str) -> String {
        format!(
            "`{} logs mm-ingress-{}-1`",
            crate::adapters::node_files::engine_program(self.engine),
            connector_service(table)
        )
    }
}
