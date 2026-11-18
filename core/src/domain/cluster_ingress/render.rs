//! The two files of an ingress node: the proxy's Compose file, and the one
//! file of routes and backends the proxy reads and watches.

use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    ACME_DIRECTORY, ACME_FILE, Backends, CERTIFICATES_DIRECTORY, CHECK_PORT, CONNECTOR_CONFIG,
    CONNECTOR_DIRECTORY, CONNECTOR_TOKEN, CONNECTOR_USER, DYNAMIC_DIRECTORY, HTTP_PORT, HTTPS_PORT,
    IngressTable, PROXY_PROJECT, TUNNEL_PORT, TUNNELS_DIRECTORY, TableTunnel, allow_expression,
    connector_config, connector_service,
};

/// The ACME resolver of the proxy, as on a single host.
const CERTIFICATE_RESOLVER: &str = "mm-http";
const REDIRECT_MIDDLEWARE: &str = "mm-http-to-https";
const LOADED_ROUTER: &str = "mm-loaded";
const DENIED_SERVICE: &str = "mm-denied";
const HTTP_ENTRYPOINT: &str = "web";
const HTTPS_ENTRYPOINT: &str = "websecure";
const CHECK_ENTRYPOINT: &str = "mm-check";
const DENIED_ENTRYPOINT: &str = "mm-denied";
const TUNNEL_ENTRYPOINT: &str = "mm-tunnel";
/// Traefik's power of two choices: the one of two random servers with fewer requests in flight.
const LEAST_BUSY_OF_TWO: &str = "p2c";
/// Inside the proxy's container: unprivileged ports, so it needs no
/// capability at all.
const PROXY_HTTP: u16 = 8080;
const PROXY_HTTPS: u16 = 8443;
const PROXY_CHECK: u16 = 8082;
const PROXY_TUNNEL: u16 = 8081;
/// The entrypoint with no router: every request sent there gets 404.
const PROXY_DENIED: u16 = 8083;
const CONTAINER_DYNAMIC: &str = "/etc/traefik/dynamic";
const CONTAINER_CERTIFICATES: &str = "/etc/traefik/certificates";
const CONTAINER_ACME: &str = "/var/lib/traefik";

/// What the proxy's Compose file has to provide for the tables it serves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProxyNeeds {
    /// Ports 80 and 443 on every address of the node.
    pub public: bool,
    /// The entrypoint the requests an allowlist turns away are sent to.
    pub denied: bool,
    /// The entrypoint the tunnel connectors send to, on the loopback
    /// address.
    pub tunnel: bool,
}

impl ProxyNeeds {
    /// What `tables` need.
    #[must_use]
    pub fn of<'a>(tables: impl IntoIterator<Item = &'a IngressTable>) -> Self {
        tables
            .into_iter()
            .fold(Self::default(), |needs, table| Self {
                public: needs.public || (table.tunnel.is_none() && !table.routes.is_empty()),
                denied: needs.denied || table.routes.iter().any(|route| !route.allow.is_empty()),
                tunnel: needs.tunnel || table.tunnel.is_some(),
            })
    }
}

/// The path the proxy answers once it runs the file of `generation`.
#[must_use]
pub fn loaded_path(generation: u64) -> String {
    format!("/mm-loaded/{generation}")
}

/// The generation a file [`render_dynamic`] wrote carries.
#[must_use]
pub fn loaded_marker(file: &str) -> Option<u64> {
    let (_, rest) = file.split_once("Path(`/mm-loaded/")?;
    rest.split('`').next()?.parse().ok()
}

/// The proxy's whole configuration: every environment's routes, each with
/// the containers it sends to now, and a route that tells `mmd` which
/// version of the file the proxy runs.
pub fn render_dynamic(
    tables: &[(String, IngressTable)],
    backends: &Backends,
    generation: u64,
) -> Result<String, String> {
    let mut http = Http::default();
    let mut certificates = Vec::new();
    for (_, table) in tables {
        if let Some(stem) = &table.certificate {
            certificates.push(Certificate {
                cert_file: format!("{CONTAINER_CERTIFICATES}/{stem}.crt"),
                key_file: format!("{CONTAINER_CERTIFICATES}/{stem}.key"),
            });
        }
        for route in &table.routes {
            add_route(route, table, backends, &mut http);
        }
    }
    http.routers.insert(
        LOADED_ROUTER.to_owned(),
        Router {
            entry_points: vec![CHECK_ENTRYPOINT],
            rule: format!("Path(`{}`)", loaded_path(generation)),
            service: "ping@internal".to_owned(),
            middlewares: Vec::new(),
            priority: 1,
            tls: None,
        },
    );
    serde_yaml_ng::to_string(&Dynamic {
        http,
        tls: (!certificates.is_empty()).then_some(TlsStore { certificates }),
    })
    .map_err(|error| format!("cannot write the proxy's routes: {error}"))
}

/// A route's routers and its service with the containers it sends to now.
fn add_route(
    route: &super::IngressRoute,
    table: &IngressTable,
    backends: &Backends,
    http: &mut Http,
) {
    let base = rule(&route.host, &route.path);
    let priority = route.priority.saturating_mul(2).saturating_add(1);
    let tunnel = table.tunnel.is_some();
    let entry_point = match (tunnel, route.tls) {
        (true, _) => TUNNEL_ENTRYPOINT,
        (false, true) => HTTPS_ENTRYPOINT,
        (false, false) => HTTP_ENTRYPOINT,
    };
    let mut middlewares = Vec::new();
    if !route.headers.is_empty() {
        let name = format!("{}-headers", route.router);
        http.middlewares.insert(
            name.clone(),
            Middleware {
                headers: Some(Headers {
                    custom_response_headers: route.headers.clone(),
                }),
                ..Middleware::default()
            },
        );
        middlewares.push(name);
    }
    if route.tls && !tunnel {
        http.middlewares.insert(
            REDIRECT_MIDDLEWARE.to_owned(),
            Middleware {
                redirect_scheme: Some(RedirectScheme {
                    scheme: "https",
                    permanent: true,
                }),
                ..Middleware::default()
            },
        );
        http.routers.insert(
            format!("{}-redirect", route.router),
            Router {
                entry_points: vec![HTTP_ENTRYPOINT],
                rule: base.clone(),
                service: route.router.clone(),
                middlewares: vec![REDIRECT_MIDDLEWARE.to_owned()],
                priority,
                tls: None,
            },
        );
    }
    let tls = (route.tls && !tunnel).then(|| Tls {
        cert_resolver: table.certificate.is_none().then_some(CERTIFICATE_RESOLVER),
    });
    let rule = if route.allow.is_empty() {
        base
    } else {
        add_denied(
            http,
            Router {
                entry_points: vec![entry_point],
                rule: base.clone(),
                service: DENIED_SERVICE.to_owned(),
                middlewares: Vec::new(),
                priority: priority.saturating_sub(1),
                tls: tls.clone(),
            },
            &route.router,
        );
        format!("{base} && ({})", allow_expression(&route.allow))
    };
    http.routers.insert(
        route.router.clone(),
        Router {
            entry_points: vec![entry_point],
            rule,
            service: route.router.clone(),
            middlewares,
            priority,
            tls,
        },
    );
    let servers = backends
        .get(&route.router)
        .into_iter()
        .flatten()
        .map(|address| Server {
            url: format!("http://{address}:{}", route.port),
        })
        .collect();
    http.services.insert(
        route.router.clone(),
        Service {
            load_balancer: LoadBalancer {
                strategy: LEAST_BUSY_OF_TWO,
                servers,
            },
        },
    );
}

/// The router that sends what a route's allowlist turns away to 404.
fn add_denied(http: &mut Http, router: Router, route: &str) {
    http.routers.insert(format!("{route}-denied"), router);
    http.services
        .entry(DENIED_SERVICE.to_owned())
        .or_insert_with(|| Service {
            load_balancer: LoadBalancer {
                strategy: LEAST_BUSY_OF_TWO,
                servers: vec![Server {
                    url: format!("http://127.0.0.1:{PROXY_DENIED}"),
                }],
            },
        });
}

fn rule(host: &str, path: &str) -> String {
    if path == "/" {
        format!("Host(`{host}`)")
    } else {
        format!("Host(`{host}`) && (Path(`{path}`) || PathPrefix(`{path}/`))")
    }
}

#[derive(Serialize)]
struct Dynamic {
    http: Http,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<TlsStore>,
}

#[derive(Serialize, Default)]
struct Http {
    routers: BTreeMap<String, Router>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    services: BTreeMap<String, Service>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    middlewares: BTreeMap<String, Middleware>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Router {
    entry_points: Vec<&'static str>,
    rule: String,
    service: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    middlewares: Vec<String>,
    priority: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<Tls>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Tls {
    #[serde(skip_serializing_if = "Option::is_none")]
    cert_resolver: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Service {
    load_balancer: LoadBalancer,
}

#[derive(Serialize)]
struct LoadBalancer {
    strategy: &'static str,
    servers: Vec<Server>,
}

#[derive(Serialize)]
struct Server {
    url: String,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Middleware {
    #[serde(skip_serializing_if = "Option::is_none")]
    redirect_scheme: Option<RedirectScheme>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<Headers>,
}

#[derive(Serialize)]
struct RedirectScheme {
    scheme: &'static str,
    permanent: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Headers {
    custom_response_headers: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct TlsStore {
    certificates: Vec<Certificate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Certificate {
    cert_file: String,
    key_file: String,
}

/// The proxy's Compose file. `root` is the node's ingress directory.
///
/// The container runs with a read-only root, no capabilities at all (its
/// ports inside are unprivileged), and no new privileges; it writes only
/// the ACME directory and a scratch `/tmp`. These are the defaults of
/// `hardening: strict`, whatever a project says: the proxy serves every
/// project on the node. It keeps the host's user ids, so its root reads
/// and writes the root-owned files `mmd` keeps for it (certificates, ACME
/// state) as their owner also on Podman, where other containers get ids of
/// their own (Docker's default anyway). It mounts no engine socket: `mmd`
/// writes what it routes to. `mmd` attaches it to each environment's
/// ingress network itself, so a new network never restarts it.
pub fn proxy_compose(
    root: &str,
    image: &str,
    serving: &[(&str, &IngressTable)],
) -> Result<String, String> {
    let needs = ProxyNeeds::of(serving.iter().map(|(_, table)| *table));
    let mut command = vec![
        "--ping=true".to_owned(),
        format!("--ping.entrypoint={CHECK_ENTRYPOINT}"),
        format!("--providers.file.directory={CONTAINER_DYNAMIC}"),
        "--providers.file.watch=true".to_owned(),
        format!("--entrypoints.{HTTP_ENTRYPOINT}.address=:{PROXY_HTTP}"),
        format!("--entrypoints.{HTTPS_ENTRYPOINT}.address=:{PROXY_HTTPS}"),
        format!("--entrypoints.{CHECK_ENTRYPOINT}.address=:{PROXY_CHECK}"),
        format!(
            "--certificatesresolvers.{CERTIFICATE_RESOLVER}.acme.storage={CONTAINER_ACME}/{ACME_FILE}"
        ),
        format!(
            "--certificatesresolvers.{CERTIFICATE_RESOLVER}.acme.httpchallenge.entrypoint={HTTP_ENTRYPOINT}"
        ),
    ];
    if needs.denied {
        command.push(format!(
            "--entrypoints.{DENIED_ENTRYPOINT}.address=127.0.0.1:{PROXY_DENIED}"
        ));
    }
    let mut ports = Vec::new();
    if needs.public {
        ports.push(format!("{HTTP_PORT}:{PROXY_HTTP}"));
        ports.push(format!("{HTTPS_PORT}:{PROXY_HTTPS}"));
    }
    ports.push(format!("127.0.0.1:{CHECK_PORT}:{PROXY_CHECK}"));
    if needs.tunnel {
        command.push(format!(
            "--entrypoints.{TUNNEL_ENTRYPOINT}.address=:{PROXY_TUNNEL}"
        ));
        command.push(format!(
            "--entrypoints.{TUNNEL_ENTRYPOINT}.forwardedHeaders.insecure=true"
        ));
        ports.push(format!("127.0.0.1:{TUNNEL_PORT}:{PROXY_TUNNEL}"));
    }
    let proxy = ProxyService {
        image: image.to_owned(),
        restart: "unless-stopped",
        command,
        ports,
        volumes: vec![
            format!("{root}/{DYNAMIC_DIRECTORY}:{CONTAINER_DYNAMIC}:ro"),
            format!("{root}/{CERTIFICATES_DIRECTORY}:{CONTAINER_CERTIFICATES}:ro"),
            format!("{root}/{ACME_DIRECTORY}:{CONTAINER_ACME}"),
        ],
        read_only: true,
        tmpfs: vec!["/tmp"],
        cap_drop: vec!["ALL"],
        security_opt: vec!["no-new-privileges:true"],
        userns_mode: "host",
        labels: BTreeMap::from([("io.mm.component", "cluster-ingress")]),
        stop_grace_period: "15s",
    };
    let mut services = BTreeMap::from([("proxy".to_owned(), ComposeService::Proxy(proxy))]);
    for (table, routes) in serving {
        if let Some(tunnel) = &routes.tunnel
            && let Some(port) = tunnel.metrics_port
        {
            services.insert(
                connector_service(table),
                ComposeService::Connector(connector(root, table, routes, tunnel, port)?),
            );
        }
    }
    serde_yaml_ng::to_string(&ProxyCompose {
        name: PROXY_PROJECT,
        services,
    })
    .map_err(|error| format!("cannot write the proxy's Compose file: {error}"))
}

/// The Compose service of an environment's tunnel connector: cloudflared
/// in the node's network namespace, reading its configuration and token
/// from the files `mmd` writes, with no capability and a read-only root.
fn connector(
    root: &str,
    table: &str,
    routes: &IngressTable,
    tunnel: &TableTunnel,
    port: u16,
) -> Result<ConnectorService, String> {
    let config = connector_config(&routes.routes)?;
    Ok(ConnectorService {
        image: tunnel.image.clone(),
        restart: "unless-stopped",
        user: format!("{CONNECTOR_USER}:{CONNECTOR_USER}"),
        network_mode: "host",
        entrypoint: vec!["cloudflared", "--no-autoupdate"],
        command: vec![
            "tunnel".to_owned(),
            "--config".to_owned(),
            format!("{CONNECTOR_DIRECTORY}/{CONNECTOR_CONFIG}"),
            "--metrics".to_owned(),
            format!("127.0.0.1:{port}"),
            "--grace-period".to_owned(),
            "25s".to_owned(),
            "--management-diagnostics=false".to_owned(),
            "run".to_owned(),
            "--token-file".to_owned(),
            format!("{CONNECTOR_DIRECTORY}/{CONNECTOR_TOKEN}"),
        ],
        volumes: vec![format!(
            "{root}/{TUNNELS_DIRECTORY}/{table}:{CONNECTOR_DIRECTORY}:ro"
        )],
        read_only: true,
        tmpfs: vec!["/tmp"],
        cap_drop: vec!["ALL"],
        security_opt: vec!["no-new-privileges:true"],
        userns_mode: "host",
        labels: BTreeMap::from([
            ("io.mm.component", "cluster-tunnel".to_owned()),
            (
                "io.mm.config",
                blake3::hash(config.as_bytes()).to_hex()[..16].to_owned(),
            ),
            ("io.mm.token-generation", tunnel.generation.to_string()),
        ]),
        stop_grace_period: "30s",
    })
}

#[derive(Serialize)]
struct ProxyCompose {
    name: &'static str,
    services: BTreeMap<String, ComposeService>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ComposeService {
    Proxy(ProxyService),
    Connector(ConnectorService),
}

#[derive(Serialize)]
struct ConnectorService {
    image: String,
    restart: &'static str,
    user: String,
    network_mode: &'static str,
    entrypoint: Vec<&'static str>,
    command: Vec<String>,
    volumes: Vec<String>,
    read_only: bool,
    tmpfs: Vec<&'static str>,
    cap_drop: Vec<&'static str>,
    security_opt: Vec<&'static str>,
    userns_mode: &'static str,
    labels: BTreeMap<&'static str, String>,
    stop_grace_period: &'static str,
}

#[derive(Serialize)]
struct ProxyService {
    image: String,
    restart: &'static str,
    command: Vec<String>,
    ports: Vec<String>,
    volumes: Vec<String>,
    read_only: bool,
    tmpfs: Vec<&'static str>,
    cap_drop: Vec<&'static str>,
    security_opt: Vec<&'static str>,
    userns_mode: &'static str,
    labels: BTreeMap<&'static str, &'static str>,
    stop_grace_period: &'static str,
}
