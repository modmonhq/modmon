//! An environment that comes in through a Cloudflare Tunnel on a cluster
//! target: its configuration, the proxy and connectors an ingress node
//! runs for it, the token that stays in its file, and what status and
//! doctor say about each connector.

use std::collections::{BTreeMap, BTreeSet};

use mm::adapters::cluster_ingress::{IngressOptions, ready_connections, write_connector_files};
use mm::adapters::config::{ConfigLoader, validate};
use mm::domain::cluster_ingress::{
    Balance, IngressRoute, IngressTable, METRICS_PORTS, NodeIngress, NodeTunnel, ProxyNeeds,
    SealedIngress, SealedTunnel, TableTunnel, TunnelStatus, connector_config, connector_service,
    metrics_port, proxy_compose, render_dynamic, route_lines, token_tunnel, tunnel_diagnostics,
};
use mm::domain::config::{EnvironmentConfig, IngressType};
use mm::domain::runtime::RuntimeKind;
use mm::domain::workspace::NodeStatus;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const TUNNEL: &str = "6ff42ae2-765d-4adf-8112-31c55c1551ef";
/// A token as `cloudflared tunnel token` prints it; its secret part is
/// what must never leave the token file.
const SECRET: &str = "c2VjcmV0LXRoYXQtbXVzdC1zdGF5LWluLWl0cy1maWxl";

fn token() -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(format!(
        r#"{{"a":"0123456789abcdef0123456789abcdef","t":"{TUNNEL}","s":"{SECRET}"}}"#
    ))
}

fn route(router: &str, host: &str, path: &str) -> IngressRoute {
    IngressRoute {
        router: router.to_owned(),
        host: host.to_owned(),
        path: path.to_owned(),
        priority: 10,
        stack: "app".to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        balance: Balance::LocalFirst,
        headers: BTreeMap::new(),
        allow: Vec::new(),
    }
}

fn tunneled(port: Option<u16>, generation: u64) -> IngressTable {
    IngressTable {
        network: "0123456789ab-production-ingress".to_owned(),
        routes: vec![
            route("mm-web", "shop.example.com", "/"),
            route("mm-api", "shop.example.com", "/api"),
            route("mm-docs", "docs.example.com", "/"),
        ],
        certificate: None,
        engine_network: Some("mm-0123-production-ingress".to_owned()),
        tunnel: Some(TableTunnel {
            token: "cloudflare.tunnel-token".to_owned(),
            image: "cloudflare/cloudflared@sha256:abc".to_owned(),
            metrics_port: port,
            generation,
        }),
    }
}

fn public() -> IngressTable {
    IngressTable {
        network: "0123456789ab-staging-ingress".to_owned(),
        routes: vec![route("mm-staging", "staging.example.com", "/")],
        certificate: None,
        engine_network: None,
        tunnel: None,
    }
}

#[test]
fn the_connector_sends_each_host_to_the_node_proxy_and_404_to_the_rest() -> TestResult {
    let config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&connector_config(&tunneled(Some(7960), 1).routes)?)?;
    let rules = config["ingress"].as_sequence().ok_or("no ingress")?;
    assert_eq!(rules.len(), 3, "one rule per host, and the last one");
    assert_eq!(rules[0]["hostname"], "docs.example.com");
    assert_eq!(rules[1]["hostname"], "shop.example.com");
    assert_eq!(rules[1]["service"], "http://127.0.0.1:7955");
    assert!(rules[2]["hostname"].is_null());
    assert_eq!(rules[2]["service"], "http_status:404");
    Ok(())
}

#[test]
fn a_tunnel_only_node_opens_no_public_port_and_runs_a_hardened_connector() -> TestResult {
    let table = tunneled(Some(7961), 3);
    let text = proxy_compose(
        "/var/lib/mm/ingress",
        "traefik:v3",
        &[("shop-production", &table)],
    )?;
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
    let strings = |value: &serde_yaml_ng::Value| {
        value
            .as_sequence()
            .map(|items| {
                items
                    .iter()
                    .filter_map(serde_yaml_ng::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let proxy = &compose["services"]["proxy"];
    assert_eq!(
        strings(&proxy["ports"]),
        ["127.0.0.1:7954:8082", "127.0.0.1:7955:8081"],
        "the proxy takes the tunnel on the loopback address only"
    );
    let command = strings(&proxy["command"]);
    assert!(command.contains(&"--entrypoints.mm-tunnel.address=:8081".to_owned()));
    assert!(command.contains(&"--entrypoints.mm-tunnel.forwardedHeaders.insecure=true".to_owned()));
    let service = connector_service("shop-production");
    assert!(
        service.starts_with("tunnel-") && service.len() == 19,
        "{service}"
    );
    let connector = &compose["services"][service.as_str()];
    assert_eq!(connector["image"], "cloudflare/cloudflared@sha256:abc");
    assert_eq!(connector["user"], "65532:65532");
    assert_eq!(connector["network_mode"], "host");
    assert_eq!(connector["read_only"], true);
    assert_eq!(connector["cap_drop"][0], "ALL");
    assert_eq!(connector["security_opt"][0], "no-new-privileges:true");
    assert_eq!(connector["userns_mode"], "host");
    assert_eq!(
        strings(&connector["volumes"]),
        ["/var/lib/mm/ingress/tunnels/shop-production:/etc/cloudflared:ro"]
    );
    assert_eq!(
        strings(&connector["entrypoint"]),
        ["cloudflared", "--no-autoupdate"]
    );
    assert_eq!(
        strings(&connector["command"]),
        [
            "tunnel",
            "--config",
            "/etc/cloudflared/config.yaml",
            "--metrics",
            "127.0.0.1:7961",
            "--grace-period",
            "25s",
            "--management-diagnostics=false",
            "run",
            "--token-file",
            "/etc/cloudflared/token",
        ]
    );
    assert_eq!(connector["labels"]["io.mm.token-generation"], "3");
    assert!(connector["environment"].is_null() && connector["ports"].is_null());

    let unassigned = tunneled(None, 0);
    let before = proxy_compose(
        "/var/lib/mm/ingress",
        "traefik:v3",
        &[("shop-production", &unassigned)],
    )?;
    assert!(
        !before.contains("tunnel-"),
        "a connector starts once its node gave it a port"
    );
    Ok(())
}

#[test]
fn a_node_with_public_and_tunnel_environments_keeps_each_on_its_entrypoint() -> TestResult {
    let tables = [
        ("shop-production".to_owned(), tunneled(Some(7960), 1)),
        ("shop-staging".to_owned(), public()),
    ];
    let needs = ProxyNeeds::of(tables.iter().map(|(_, table)| table));
    assert_eq!(
        needs,
        ProxyNeeds {
            public: true,
            denied: false,
            tunnel: true,
        }
    );
    assert!(!ProxyNeeds::of([&tunneled(Some(7960), 1)]).public);
    let text = render_dynamic(&tables, &BTreeMap::new(), 4)?;
    let file: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
    let routers = &file["http"]["routers"];
    assert_eq!(routers["mm-web"]["entryPoints"][0], "mm-tunnel");
    assert!(
        routers["mm-web"]["tls"].is_null(),
        "Cloudflare ends TLS: the tunnel's routes ask nothing of ACME"
    );
    assert!(routers["mm-web-redirect"].is_null());
    assert_eq!(routers["mm-staging"]["entryPoints"][0], "websecure");
    assert_eq!(routers["mm-staging-redirect"]["entryPoints"][0], "web");
    for (name, router) in routers.as_mapping().ok_or("no routers")? {
        let entry = router["entryPoints"][0].as_str().unwrap_or_default();
        let name = name.as_str().unwrap_or_default();
        if ["mm-web", "mm-api", "mm-docs"].contains(&name) {
            assert_eq!(entry, "mm-tunnel", "{name}");
        } else {
            assert_ne!(entry, "mm-tunnel", "{name}");
        }
    }
    Ok(())
}

#[test]
fn each_connector_keeps_its_port_and_takes_the_lowest_free_one_first() {
    let mut other = tunneled(Some(7960), 1);
    other.routes.clear();
    let tables = vec![("other".to_owned(), other)];
    assert_eq!(
        metrics_port("shop", None, &tables, METRICS_PORTS),
        Some(7961)
    );
    let previous = tunneled(Some(7975), 2);
    assert_eq!(
        metrics_port("shop", Some(&previous), &tables, METRICS_PORTS),
        Some(7975)
    );
    assert_eq!(metrics_port("shop", None, &tables, 7960..=7960), None);
}

#[test]
fn a_token_names_its_tunnel_and_nothing_else_is_taken_for_one() -> TestResult {
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(token())?;
    assert_eq!(token_tunnel(&decoded)?, TUNNEL);
    for wrong in [
        b"not json".to_vec(),
        br#"{"a":"x","t":"y"}"#.to_vec(),
        br#"{"a":"x","t":"y\";","s":"z"}"#.to_vec(),
        format!(r#"{{"AccountTag":"x","TunnelID":"{TUNNEL}","TunnelSecret":"{SECRET}"}}"#)
            .into_bytes(),
    ] {
        let problem = token_tunnel(&wrong).err().ok_or("taken for a token")?;
        assert!(
            !problem.contains(SECRET) && !problem.contains("not json"),
            "{problem}"
        );
    }
    Ok(())
}

#[test]
fn an_environment_comes_in_through_a_tunnel_only_with_a_token() -> TestResult {
    let tunneled: EnvironmentConfig = serde_yaml_ng::from_str(
        "target: live\ningress: {type: cloudflared, cloudflared: {token: cloudflare.tunnel-token}}\n",
    )?;
    let ingress = tunneled.ingress.ok_or("no ingress")?;
    assert_eq!(ingress.kind, IngressType::Cloudflared);
    let (secret, image) = ingress.tunnel().ok_or("no tunnel")?;
    assert_eq!(secret, "cloudflare.tunnel-token");
    assert!(
        image.starts_with("docker.io/cloudflare/cloudflared@sha256:"),
        "{image}"
    );
    let default: EnvironmentConfig = serde_yaml_ng::from_str("target: live\ningress: {}\n")?;
    assert_eq!(
        default.ingress.ok_or("no ingress")?.kind,
        IngressType::Public
    );
    let missing = serde_yaml_ng::from_str::<EnvironmentConfig>(
        "target: live\ningress: {type: cloudflared}\n",
    )
    .err()
    .ok_or("loaded without a token")?
    .to_string();
    assert!(
        missing.contains("cloudflared.token") && missing.contains("mm secret set"),
        "{missing}"
    );
    let typeless = serde_yaml_ng::from_str::<EnvironmentConfig>(
        "target: live\ningress: {cloudflared: {token: t}}\n",
    )
    .err()
    .ok_or("a tunnel without its type loaded")?
    .to_string();
    assert!(typeless.contains("type: cloudflared"), "{typeless}");
    let written = serde_yaml_ng::to_string(&serde_yaml_ng::from_str::<EnvironmentConfig>(
        "target: live\ningress: {type: cloudflared, cloudflared: {token: t}, balance: spread}\n",
    )?)?;
    assert!(written.contains("type: cloudflared") && written.contains("balance: spread"));
    Ok(())
}

fn project(environment: &str) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let root = temp_dirs::tempdir()?;
    std::fs::create_dir_all(root.path().join(".config"))?;
    std::fs::write(
        root.path().join(".config/mm.yaml"),
        format!(
            "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app: {{files: [compose.yaml]}}
secrets:
  cloudflare.tunnel-token: {{storage: {{provider: age, location: cloudflare.tunnel-token}}}}
environments:
  production:
    target: local
{environment}
"
        ),
    )?;
    std::fs::write(
        root.path().join("compose.yaml"),
        "services: {web: {image: busybox}}\n",
    )?;
    Ok(root)
}

#[test]
fn validation_names_an_undeclared_token_and_a_certificate_cloudflare_ends() -> TestResult {
    let declared =
        project("    ingress: {type: cloudflared, cloudflared: {token: cloudflare.tunnel-token}}")?;
    let report = validate(&ConfigLoader.load_from(declared.path())?, "production");
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<BTreeSet<_>>();
    assert!(!codes.contains("MM_TUNNEL_TOKEN_MISSING"), "{codes:?}");
    assert!(
        !codes.contains("MM_SECRET_UNUSED"),
        "the tunnel's token counts as used"
    );
    let undeclared = project(
        "    tls: {certificate: cloudflare.tunnel-token, key: cloudflare.tunnel-token}
    ingress: {type: cloudflared, cloudflared: {token: other.token}}",
    )?;
    let report = validate(&ConfigLoader.load_from(undeclared.path())?, "production");
    let messages = report
        .diagnostics
        .iter()
        .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("MM_TUNNEL_TOKEN_MISSING")
                && message.contains("'other.token'")
                && message.contains("mm -e production secret set other.token --stdin")),
        "{messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|message| message.starts_with("MM_ENVIRONMENT_TLS_INVALID")
                && message.contains("cloudflared")),
        "{messages:?}"
    );
    Ok(())
}

#[test]
fn the_token_stays_in_its_file() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let options = IngressOptions {
        connector_user: None,
        ..IngressOptions::on_node(directory.path(), RuntimeKind::DockerCompose)
    };
    let table = tunneled(Some(7960), 1);
    write_connector_files(
        &options,
        "shop-production",
        &table,
        format!("{}\n", token()).as_bytes(),
    )?;
    let own = directory.path().join("ingress/tunnels/shop-production");
    assert_eq!(std::fs::read(own.join("token"))?, token().as_bytes());
    {
        use std::os::unix::fs::PermissionsExt;
        for (file, mode) in [("token", 0o400), ("config.yaml", 0o400)] {
            assert_eq!(
                std::fs::metadata(own.join(file))?.permissions().mode() & 0o777,
                mode,
                "{file}"
            );
        }
        assert_eq!(std::fs::metadata(&own)?.permissions().mode() & 0o777, 0o700);
    }
    let sealed = SealedIngress {
        nodes: vec!["app-1".to_owned()],
        network: table.network.clone(),
        routes: table.routes.clone(),
        certificate: None,
        tunnel: Some(SealedTunnel {
            token: "cloudflare.tunnel-token".to_owned(),
            image: "cloudflare/cloudflared@sha256:abc".to_owned(),
            id: TUNNEL.to_owned(),
            open_ports: BTreeMap::new(),
        }),
    };
    let node = NodeIngress {
        certificate: None,
        tunnel: Some(NodeTunnel {
            token: "cloudflare.tunnel-token".to_owned(),
            image: "cloudflare/cloudflared@sha256:abc".to_owned(),
            id: TUNNEL.to_owned(),
            open_ports_for: Vec::new(),
        }),
    };
    let tables = [("shop-production".to_owned(), table.clone())];
    let written = [
        proxy_compose(
            &options.root.to_string_lossy(),
            "traefik:v3",
            &[("shop-production", &table)],
        )?,
        render_dynamic(&tables, &BTreeMap::new(), 1)?,
        std::fs::read_to_string(own.join("config.yaml"))?,
        serde_json::to_string(&sealed)?,
        serde_json::to_string(&node)?,
        serde_json::to_string(&table)?,
        format!("{sealed:?} {node:?} {table:?}"),
        route_lines(&sealed).join("\n"),
    ];
    for text in &written {
        assert!(
            !text.contains(SECRET) && !text.contains(&token()),
            "the token is in: {text}"
        );
    }
    sealed.validate()?;
    let mut broken = sealed.clone();
    if let Some(tunnel) = broken.tunnel.as_mut() {
        tunnel.image = "cloudflared --privileged".to_owned();
    }
    assert!(broken.validate().is_err());
    let without: serde_json::Value = serde_json::to_value(SealedIngress {
        tunnel: None,
        ..sealed
    })?;
    assert!(
        without.get("tunnel").is_none(),
        "an ingress without a tunnel is written as before"
    );
    Ok(())
}

#[test]
fn doctor_reports_a_connector_without_connections_and_open_public_ports() {
    let node = |name: &str, connections: Option<u32>, public_ports: bool| NodeStatus {
        reachable: true,
        tunnel: Some(TunnelStatus {
            token: "cloudflare.tunnel-token".to_owned(),
            connections,
            public_ports,
        }),
        ..NodeStatus::pending(name.to_owned())
    };
    let found = tunnel_diagnostics(&[
        node("app-1", Some(4), false),
        node("app-2", Some(0), false),
        node("app-3", None, true),
        NodeStatus::pending("app-4".to_owned()),
    ]);
    let codes = found
        .iter()
        .map(|diagnostic| (diagnostic.code.as_str(), diagnostic.message.clone()))
        .collect::<Vec<_>>();
    assert_eq!(codes.len(), 3, "{codes:?}");
    assert!(codes[0].0 == "MM_TUNNEL_NOT_CONNECTED" && codes[0].1.contains("'app-2'"));
    assert!(codes[1].0 == "MM_TUNNEL_NOT_CONNECTED" && codes[1].1.contains("does not answer"));
    assert!(codes[2].0 == "MM_TUNNEL_PUBLIC_PORTS" && codes[2].1.contains("'app-3'"));
    assert!(
        found[0]
            .help
            .as_deref()
            .is_some_and(|help| help.contains("mm secret set cloudflare.tunnel-token"))
    );
}

/// A connector's readiness endpoint that answers `status` with `body`.
async fn ready_endpoint(status: &'static str, body: &'static str) -> std::io::Result<u16> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut request = [0_u8; 512];
            let _ = stream.read(&mut request).await;
            let _ = stream
                .write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\r\n{body}")
                        .as_bytes(),
                )
                .await;
        }
    });
    Ok(port)
}

#[tokio::test]
async fn readiness_counts_the_connections_cloudflared_reports() -> TestResult {
    let ready = ready_endpoint(
        "200 OK",
        r#"{"status":200,"readyConnections":4,"connectorId":"x"}"#,
    )
    .await?;
    assert_eq!(ready_connections(ready).await, Some(4));
    let waiting = ready_endpoint(
        "503 Service Unavailable",
        r#"{"status":503,"readyConnections":0,"connectorId":"x"}"#,
    )
    .await?;
    assert_eq!(ready_connections(waiting).await, Some(0));
    let plain = ready_endpoint("200 OK", "ok").await?;
    assert_eq!(ready_connections(plain).await, Some(1));
    let closed = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await?
        .local_addr()?
        .port();
    assert_eq!(ready_connections(closed).await, None);
    Ok(())
}
