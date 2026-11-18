//! Traffic into a cluster target: the nodes that run the ingress proxy, the
//! routes they serve, and what `mmd` writes for the proxy.
//!
//! The routes are part of the reviewed plan: each node's plan carries the
//! routes of the services placed on it, and the client puts them together
//! in the cluster plan's seal, with the nodes that run the proxy. Which
//! containers a route sends to is not: the ingress node's `mmd` writes that
//! from its service map, the one it answers names from, and rewrites it the
//! moment the map changes. Like a name, a route goes to the healthy
//! containers on the proxy's own node first and to every other node's when
//! there is none, or with [`Balance::Spread`] to every node's at once, and
//! never to a container that is leaving. Each request goes to the less busy
//! of two of them picked at random. The proxy (Traefik) reads one file and
//! picks up each change without a restart and without dropping a
//! connection; it has no Docker socket.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::EnvironmentTlsConfig;
use crate::domain::workspace::ProjectId;

#[path = "cluster_ingress/acme.rs"]
mod acme;
pub use acme::{ForgottenCertificates, forget_hosts, provided_hosts};
#[path = "cluster_ingress/render.rs"]
mod render;
pub use render::{ProxyNeeds, loaded_marker, loaded_path, proxy_compose, render_dynamic};
#[path = "cluster_ingress/rules.rs"]
mod rules;
#[path = "cluster_ingress/tunnel.rs"]
mod tunnel;
pub use rules::{
    AllowRule, MAX_PRIORITY, allow_expression, allow_summary, header_problems, rule_problems,
};
pub use tunnel::{
    CONNECTOR_CONFIG, CONNECTOR_DIRECTORY, CONNECTOR_TOKEN, CONNECTOR_USER,
    DEFAULT_CLOUDFLARED_IMAGE, METRICS_PORTS, NodeTunnel, SealedTunnel, TUNNEL_PORT,
    TUNNELS_DIRECTORY, TableTunnel, TunnelStatus, connector_config, connector_service,
    metrics_port, token_tunnel, tunnel_diagnostics, tunnel_hosts,
};

/// The network of `mm.yaml` the proxy and every routed service share on a
/// cluster target. mm adds it when the configuration does not declare it.
pub const INGRESS_NETWORK: &str = "ingress";

/// What the proxy is called where a network lists its members.
pub const PROXY_MEMBER: &str = "mm-ingress-proxy";

/// The proxy's Compose project on an ingress node.
pub const PROXY_PROJECT: &str = "mm-ingress";

/// The ports the proxy takes on every address of an ingress node.
pub const HTTP_PORT: u16 = 80;
pub const HTTPS_PORT: u16 = 443;

/// The port on the node's loopback address where `mmd` asks the proxy
/// whether it runs the newest file.
pub const CHECK_PORT: u16 = 7954;

/// Under the ingress directory of an ingress node (`/var/lib/mm/ingress`):
/// the routes of each project's environment, the file the proxy reads,
/// provided certificates, and the ACME account and certificates.
pub const TABLES_DIRECTORY: &str = "routes";
pub const DYNAMIC_DIRECTORY: &str = "dynamic";
pub const CERTIFICATES_DIRECTORY: &str = "certificates";
pub const ACME_DIRECTORY: &str = "acme";
pub const DYNAMIC_FILE: &str = "mm.yaml";
pub const COMPOSE_FILE: &str = "compose.yaml";
pub const ACME_FILE: &str = "acme.json";

/// One route of an environment as the ingress serves it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IngressRoute {
    /// The proxy's name for the route, unique across projects.
    pub router: String,
    pub host: String,
    /// `/` or a path prefix without a trailing slash.
    pub path: String,
    pub priority: i64,
    pub stack: String,
    pub service: String,
    /// The port of the service's containers.
    pub port: u16,
    pub tls: bool,
    #[serde(default, skip_serializing_if = "Balance::is_local_first")]
    pub balance: Balance,
    /// Response headers the proxy sets.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    /// The requests the route lets through; every other one gets 404.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<AllowRule>,
}

/// Which of a service's healthy containers an ingress node sends a route's
/// requests to.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Balance {
    /// The ones on the proxy's own node, and every node's when it has none.
    #[default]
    LocalFirst,
    /// The ones on every node.
    Spread,
}

impl Balance {
    #[must_use]
    pub const fn is_local_first(&self) -> bool {
        matches!(self, Self::LocalFirst)
    }

    #[must_use]
    pub const fn reach(self) -> crate::domain::discovery::Reach {
        match self {
            Self::LocalFirst => crate::domain::discovery::Reach::LocalFirst,
            Self::Spread => crate::domain::discovery::Reach::EveryNode,
        }
    }
}

impl IngressRoute {
    /// The address people use, such as `https://shop.example.com/api`.
    #[must_use]
    pub fn url(&self) -> String {
        format!(
            "{}://{}{}",
            if self.tls { "https" } else { "http" },
            self.host,
            if self.path == "/" { "" } else { &self.path }
        )
    }

    fn validate(&self) -> Result<(), String> {
        let named = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        };
        let host = !self.host.is_empty()
            && self.host.len() <= 253
            && self.host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            });
        let path = self.path.starts_with('/')
            && (self.path == "/" || !self.path.ends_with('/'))
            && !self.path.contains("//")
            && !self
                .path
                .split('/')
                .any(|segment| matches!(segment, "." | ".."))
            && self.path.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~')
            });
        let service = !self.service.is_empty()
            && self
                .service
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
        let rules = rule_problems(&self.headers, &self.allow, None);
        if let Some(problem) = rules.first() {
            return Err(format!("route {}: {problem}", self.url()));
        }
        if named(&self.router) && host && path && service && self.port != 0 {
            Ok(())
        } else {
            Err(format!(
                "route {} to service '{}' port {} is not one mm writes",
                self.url(),
                self.service,
                self.port
            ))
        }
    }
}

/// What a node's plan says when the node runs the ingress proxy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct NodeIngress {
    /// The environment's `tls:` secrets, when it provides its certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<EnvironmentTlsConfig>,
    /// The environment's Cloudflare Tunnel, when it comes in through one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<NodeTunnel>,
}

/// The ingress of a cluster plan: carried by the plan's seal and applied by
/// every ingress node when the rollout finishes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct SealedIngress {
    /// The nodes that run the proxy, in the target's order. Empty when the
    /// environment has no route: every node takes away what it served.
    pub nodes: Vec<String>,
    /// The network the proxy and the routed services share, by its name
    /// across the cluster.
    pub network: String,
    pub routes: Vec<IngressRoute>,
    /// The environment's `tls:` secrets, when it provides its certificate;
    /// otherwise each HTTPS route's certificate comes from ACME.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<EnvironmentTlsConfig>,
    /// The Cloudflare Tunnel the routes come in through, in place of ports
    /// 80 and 443.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<SealedTunnel>,
}

impl SealedIngress {
    #[must_use]
    pub fn serves(&self, node: &str) -> bool {
        self.nodes.iter().any(|name| name == node)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.is_empty() && !self.routes.is_empty() {
            return Err("the ingress of the plan has routes and no node to serve them".to_owned());
        }
        for route in &self.routes {
            route.validate()?;
        }
        if let Some(tunnel) = &self.tunnel {
            tunnel.validate()?;
        }
        Ok(())
    }
}

/// What an ingress node keeps for one project's environment, by its
/// [`table_name`]. `mmd` turns it into the proxy's file together with the
/// backends of the moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct IngressTable {
    /// The network the backends are reached on, by its name across the
    /// cluster.
    pub network: String,
    pub routes: Vec<IngressRoute>,
    /// Set when the environment provides its certificate: a digest of the
    /// two files, so that a new certificate is loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<String>,
    /// The engine's name of `network` on this node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_network: Option<String>,
    /// Set when the routes come in through a Cloudflare Tunnel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel: Option<TableTunnel>,
}

/// The name of an environment's table, certificate, and key files.
#[must_use]
pub fn table_name(project: ProjectId, environment: &str) -> String {
    format!("{}-{environment}", project.0)
}

/// The containers each route sends to now, by router name.
pub type Backends = BTreeMap<String, Vec<Ipv4Addr>>;

/// What the ingress section of a cluster plan shows.
#[must_use]
pub fn route_lines(ingress: &SealedIngress) -> Vec<String> {
    let mut routes = ingress.routes.iter().collect::<Vec<_>>();
    routes.sort_by(|left, right| {
        (&left.host, std::cmp::Reverse(left.priority))
            .cmp(&(&right.host, std::cmp::Reverse(right.priority)))
    });
    routes
        .into_iter()
        .map(|route| {
            let mut line = format!(
                "{} -> {}/{} port {}",
                route.url(),
                route.stack,
                route.service,
                route.port
            );
            if !route.balance.is_local_first() {
                line.push_str(", spread over every node");
            }
            if !route.allow.is_empty() {
                line.push_str(&format!(
                    ", lets through {} and answers 404 to the rest",
                    allow_summary(&route.allow)
                ));
            }
            if !route.headers.is_empty() {
                let names = route.headers.keys().cloned().collect::<Vec<_>>();
                line.push_str(&format!(", sets {}", names.join(", ")));
            }
            line
        })
        .collect()
}
