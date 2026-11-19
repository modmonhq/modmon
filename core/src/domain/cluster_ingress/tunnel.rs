//! A Cloudflare Tunnel in front of an environment's routes on a cluster
//! target: a connector next to the proxy on every ingress node, configured
//! by mm from the routes, reaching the proxy on the node's loopback address.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{IngressRoute, IngressTable};
use crate::domain::diagnostic::Diagnostic;

/// cloudflared 2026.8.1, which reads its token from a file
/// (`tunnel run --token-file`).
pub const DEFAULT_CLOUDFLARED_IMAGE: &str = "docker.io/cloudflare/cloudflared@sha256:d9ff527dc19799e24d3689058161fb01e752a24be1823577bae54c9c2489275f";

/// The port on the node's loopback address where the proxy takes the
/// requests of every tunnel.
pub const TUNNEL_PORT: u16 = 7955;

/// The ports on the node's loopback address the connectors' metrics and
/// readiness answer on, one each.
pub const METRICS_PORTS: RangeInclusive<u16> = 7960..=7999;

/// Under the ingress directory: one directory per environment with a
/// tunnel, holding the connector's configuration and token.
pub const TUNNELS_DIRECTORY: &str = "tunnels";
pub const CONNECTOR_CONFIG: &str = "config.yaml";
pub const CONNECTOR_TOKEN: &str = "token";
/// The user the connector runs as, and that owns its two files.
pub const CONNECTOR_USER: u32 = 65532;
/// Where the connector finds its two files.
pub const CONNECTOR_DIRECTORY: &str = "/etc/cloudflared";

/// An environment's tunnel as its configuration names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SealedTunnel {
    /// The secret that holds the tunnel's token.
    pub token: String,
    /// The connector's image.
    pub image: String,
    /// The tunnel the token belongs to.
    pub id: String,
    /// The nodes that keep ports 80 and 443 open for other environments
    /// they serve, with those environments.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub open_ports: BTreeMap<String, Vec<String>>,
}

/// What a node's plan says about the tunnel of an environment it serves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeTunnel {
    pub token: String,
    pub image: String,
    /// The tunnel the token this node holds belongs to.
    pub id: String,
    /// The other environments on the node that keep ports 80 and 443 open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_ports_for: Vec<String>,
}

impl SealedTunnel {
    pub fn validate(&self) -> Result<(), String> {
        let named = |value: &str, extra: &[u8]| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || extra.contains(&byte))
        };
        let image = !self.image.is_empty()
            && self.image.len() <= 512
            && !self
                .image
                .chars()
                .any(|character| character.is_whitespace() || character.is_control());
        if named(&self.token, b".-_") && named(&self.id, b"-") && image {
            Ok(())
        } else {
            Err(format!(
                "the tunnel of the plan names secret '{}', tunnel '{}', and image '{}', which mm does not run",
                self.token, self.id, self.image
            ))
        }
    }
}

/// What an ingress node keeps of an environment's tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TableTunnel {
    pub token: String,
    pub image: String,
    /// The connector's metrics port on the loopback address, given out by
    /// the node when it first serves the tunnel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics_port: Option<u16>,
    /// Counts the tokens the node was given; a new one restarts the
    /// connector.
    #[serde(default)]
    pub generation: u64,
}

/// What an ingress node says about the connector of an environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct TunnelStatus {
    /// The secret that holds the tunnel's token.
    pub token: String,
    /// The connections the connector holds to Cloudflare; `None` when it
    /// does not answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<u32>,
    /// Whether the node's proxy has ports 80 or 443 open on its public
    /// addresses.
    #[serde(default)]
    pub public_ports: bool,
}

/// The hosts a connector sends to its node's proxy.
#[must_use]
pub fn tunnel_hosts(routes: &[IngressRoute]) -> Vec<String> {
    routes
        .iter()
        .map(|route| route.host.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The connector's configuration: each host to the node's proxy, and 404
/// for anything else.
pub fn connector_config(routes: &[IngressRoute]) -> Result<String, String> {
    #[derive(Serialize)]
    struct Config {
        ingress: Vec<Rule>,
    }
    #[derive(Serialize)]
    struct Rule {
        #[serde(skip_serializing_if = "Option::is_none")]
        hostname: Option<String>,
        service: String,
    }
    let proxy = format!("http://127.0.0.1:{TUNNEL_PORT}");
    let mut ingress = tunnel_hosts(routes)
        .into_iter()
        .map(|host| Rule {
            hostname: Some(host),
            service: proxy.clone(),
        })
        .collect::<Vec<_>>();
    ingress.push(Rule {
        hostname: None,
        service: "http_status:404".to_owned(),
    });
    serde_yaml_ng::to_string(&Config { ingress })
        .map_err(|error| format!("cannot write the tunnel connector's configuration: {error}"))
}

/// The Compose service of an environment's connector.
#[must_use]
pub fn connector_service(table: &str) -> String {
    format!("tunnel-{}", &blake3::hash(table.as_bytes()).to_hex()[..12])
}

/// The metrics port for `table`: the one it has, else the lowest one no
/// other table holds.
#[must_use]
pub fn metrics_port(
    table: &str,
    previous: Option<&IngressTable>,
    tables: &[(String, IngressTable)],
    ports: RangeInclusive<u16>,
) -> Option<u16> {
    if let Some(port) = previous
        .and_then(|previous| previous.tunnel.as_ref())
        .and_then(|tunnel| tunnel.metrics_port)
    {
        return Some(port);
    }
    let taken = tables
        .iter()
        .filter(|(name, _)| name != table)
        .filter_map(|(_, other)| other.tunnel.as_ref()?.metrics_port)
        .collect::<BTreeSet<_>>();
    ports.into_iter().find(|port| !taken.contains(port))
}

/// The tunnel a token belongs to: a Cloudflare Tunnel token is base64 of a
/// JSON object whose `t` is the tunnel's ID.
pub fn token_tunnel(decoded: &[u8]) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Token {
        a: String,
        t: String,
        s: String,
    }
    let token: Token = serde_json::from_slice(decoded)
        .map_err(|_| "it is not a Cloudflare Tunnel token".to_owned())?;
    let named = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    };
    if named(&token.a) && named(&token.t) && !token.s.is_empty() {
        Ok(token.t)
    } else {
        Err("it is not a Cloudflare Tunnel token".to_owned())
    }
}

/// What `mm doctor` finds about each node's connector of the environment.
#[must_use]
pub fn tunnel_diagnostics(nodes: &[crate::domain::workspace::NodeStatus]) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for node in nodes {
        let Some(tunnel) = &node.tunnel else {
            continue;
        };
        if tunnel.connections.unwrap_or_default() == 0 {
            found.push(
                Diagnostic::error(
                    "MM_TUNNEL_NOT_CONNECTED",
                    format!(
                        "the Cloudflare Tunnel connector on node '{}' holds no connection to Cloudflare{}",
                        node.name,
                        if tunnel.connections.is_none() {
                            " and does not answer"
                        } else {
                            ""
                        }
                    ),
                )
                .with_help(format!(
                    "`docker logs` or `podman logs` of the connector's container (mm-ingress-tunnel-…) on '{}' says why; a token that Cloudflare refuses is stored again with `mm secret set {} --stdin`, and the next deployment restarts the connector",
                    node.name, tunnel.token
                )),
            );
        }
        if tunnel.public_ports {
            found.push(
                Diagnostic::warning(
                    "MM_TUNNEL_PUBLIC_PORTS",
                    format!(
                        "node '{}' keeps ports 80 and 443 open for an environment with public ingress; this environment's routes are not served on them",
                        node.name
                    ),
                )
                .with_help(
                    "give that environment `ingress.type: cloudflared`, or move its ingress to other nodes with `cluster.ingress`, to close them",
                ),
            );
        }
    }
    found
}
