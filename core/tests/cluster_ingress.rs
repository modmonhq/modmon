//! Ingress on a cluster target: which nodes run the proxy, how routed
//! services meet it on one network, the files the proxy reads, and how a
//! container leaves without dropping a request.

use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::time::Duration;

use mm::adapters::cluster_ingress::{IngressOptions, forget_provided_certificates};
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner};
use mm::domain::cluster_ingress::{
    Balance, INGRESS_NETWORK, IngressRoute, IngressTable, PROXY_MEMBER, SealedIngress,
    forget_hosts, loaded_marker, provided_hosts, proxy_compose, render_dynamic, route_lines,
};
use mm::domain::config::{DurationSpec, EnvironmentConfig, MmConfig};
use mm::domain::discovery::NodeRecords;
use mm::domain::error::MmError;
use mm::domain::leaving::{DEFAULT_DRAIN, LeaveRecord, LeaveStage, drain_of};
use mm::domain::placement::{NodeScope, runs_ingress, view_from_node};
use mm::domain::rollout::RolloutPolicy;
use mm::domain::runtime::RuntimeKind;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(cluster: &str, routes: &str) -> Result<MmConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(&format!(
        "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app:
    files: [app.yaml]
{routes}
  db: {{files: [db.yaml]}}
environments:
  production:
    target: live
    domains: {{shop: shop.example.com}}
modules:
  shop:
    path: .
    services:
      web: {{stack: app, placement: all, drain: 40s}}
      worker: {{stack: app, placement: app-3}}
      db: {{stack: db, placement: app-2}}
targets:
  live:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
      - {{name: app-3, host: 203.0.113.13}}
{cluster}
"
    ))
}

const ROUTE: &str = "    ingress: {domain: shop, service: web}";

fn scope(node: &str) -> NodeScope {
    NodeScope {
        target: "live".to_owned(),
        node: node.to_owned(),
        environment: "production".to_owned(),
        resolver_files: Some(std::path::PathBuf::from("/var/lib/mm/networks/resolv")),
        mesh_addresses: BTreeMap::new(),
    }
}

#[test]
fn the_first_node_runs_the_proxy_unless_the_target_names_others() -> TestResult {
    let first = config("", ROUTE)?;
    let target = first.targets.get("live").ok_or("no target")?;
    assert_eq!(target.ingress_nodes()?, ["app-1"]);
    for (cluster, nodes) in [
        ("    cluster: {ingress: app-2}", vec!["app-2"]),
        (
            "    cluster: {ingress: [app-3, app-1]}",
            vec!["app-1", "app-3"],
        ),
        (
            "    cluster: {ingress: all}",
            vec!["app-1", "app-2", "app-3"],
        ),
        (
            "    cluster: {groups: {edge: [app-2, app-3]}, ingress: edge}",
            vec!["app-2", "app-3"],
        ),
    ] {
        let config = config(cluster, ROUTE)?;
        let target = config.targets.get("live").ok_or("no target")?;
        assert_eq!(target.ingress_nodes()?, nodes, "{cluster}");
        target.validate("live")?;
    }
    let unknown = config("    cluster: {ingress: app-9}", ROUTE)?;
    let target = unknown.targets.get("live").ok_or("no target")?;
    let problem = target.validate("live").err().unwrap_or_default();
    assert!(
        problem.contains("cluster.ingress") && problem.contains("'app-9'"),
        "{problem}"
    );
    Ok(())
}

#[test]
fn routed_services_meet_the_proxy_on_the_ingress_network() -> TestResult {
    // app-1 runs the proxy and web; app-2 runs web and db; app-3 runs web
    // and worker.
    for (node, proxy) in [("app-1", true), ("app-2", false), ("app-3", false)] {
        let mut viewed = config("", ROUTE)?;
        view_from_node(&mut viewed, &scope(node))?;
        let network = viewed
            .networks
            .get(INGRESS_NETWORK)
            .ok_or("no ingress network")?;
        assert_eq!(network.cluster_ingress, Some(proxy), "{node}");
        assert!(network.resolver.is_some(), "{node}: names answered on it");
        let services = &viewed.modules["shop"].services;
        assert_eq!(services["web"].networks, [INGRESS_NETWORK], "{node}");
        assert!(services["worker"].networks.is_empty(), "{node}");
        assert!(services["db"].networks.is_empty(), "{node}");
        assert_eq!(
            runs_ingress(&config("", ROUTE)?, "live", node, "production"),
            proxy
        );
    }
    // Without a route nothing changes, and no node runs a proxy.
    let mut quiet = config("", "")?;
    view_from_node(&mut quiet, &scope("app-1"))?;
    assert!(quiet.networks.is_empty());
    assert!(!runs_ingress(
        &config("", "")?,
        "live",
        "app-1",
        "production"
    ));
    Ok(())
}

#[test]
fn a_declared_ingress_network_is_the_one_the_proxy_joins() -> TestResult {
    let mut viewed = config("", ROUTE)?;
    viewed.networks.insert(
        INGRESS_NETWORK.to_owned(),
        serde_yaml_ng::from_str("internal: true")?,
    );
    view_from_node(&mut viewed, &scope("app-1"))?;
    let network = &viewed.networks[INGRESS_NETWORK];
    assert!(network.internal, "what the configuration says is kept");
    assert_eq!(network.cluster_ingress, Some(true));
    Ok(())
}

fn route(router: &str, path: &str, tls: bool) -> IngressRoute {
    IngressRoute {
        router: router.to_owned(),
        host: "shop.example.com".to_owned(),
        path: path.to_owned(),
        priority: 10,
        stack: "app".to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls,
        balance: Balance::LocalFirst,
        headers: BTreeMap::new(),
        allow: Vec::new(),
    }
}

#[test]
fn the_proxy_file_lists_each_route_with_the_containers_it_sends_to() -> TestResult {
    let table = IngressTable {
        network: "0123456789ab-production-ingress".to_owned(),
        routes: vec![route("mm-web", "/", true), route("mm-api", "/api", false)],
        certificate: None,
        engine_network: None,
        tunnel: None,
    };
    let backends = BTreeMap::from([
        (
            "mm-web".to_owned(),
            vec![Ipv4Addr::new(10, 64, 0, 2), Ipv4Addr::new(10, 64, 1, 2)],
        ),
        ("mm-api".to_owned(), Vec::new()),
    ]);
    let text = render_dynamic(&[("shop".to_owned(), table.clone())], &backends, 7)?;
    let file: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
    let routers = &file["http"]["routers"];
    assert_eq!(routers["mm-web"]["entryPoints"][0], "websecure");
    assert_eq!(routers["mm-web"]["rule"], "Host(`shop.example.com`)");
    assert_eq!(routers["mm-web"]["tls"]["certResolver"], "mm-http");
    assert_eq!(routers["mm-web-redirect"]["entryPoints"][0], "web");
    assert_eq!(
        routers["mm-web-redirect"]["middlewares"][0],
        "mm-http-to-https"
    );
    assert_eq!(
        routers["mm-api"]["rule"],
        "Host(`shop.example.com`) && (Path(`/api`) || PathPrefix(`/api/`))"
    );
    assert!(routers["mm-api"]["tls"].is_null());
    assert_eq!(routers["mm-loaded"]["rule"], "Path(`/mm-loaded/7`)");
    assert_eq!(routers["mm-loaded"]["service"], "ping@internal");
    let services = &file["http"]["services"];
    let servers = services["mm-web"]["loadBalancer"]["servers"]
        .as_sequence()
        .ok_or("no servers")?
        .iter()
        .map(|server| server["url"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(servers, ["http://10.64.0.2:8080", "http://10.64.1.2:8080"]);
    assert_eq!(
        services["mm-web"]["loadBalancer"]["strategy"], "p2c",
        "each request goes to the less busy of two containers"
    );
    assert_eq!(
        services["mm-api"]["loadBalancer"]["servers"]
            .as_sequence()
            .map(Vec::len),
        Some(0),
        "a route with no healthy container answers 503, not a stale one"
    );
    assert!(file["tls"].is_null());

    // A provided certificate: the routers take it, nothing asks ACME.
    let provided = IngressTable {
        certificate: Some("shop-production-0123456789ab".to_owned()),
        ..table
    };
    let text = render_dynamic(&[("shop".to_owned(), provided)], &backends, 8)?;
    let file: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
    assert!(file["http"]["routers"]["mm-web"]["tls"]["certResolver"].is_null());
    assert_eq!(
        file["tls"]["certificates"][0]["certFile"],
        "/etc/traefik/certificates/shop-production-0123456789ab.crt"
    );
    Ok(())
}

#[test]
fn a_spread_route_reaches_every_node_and_says_so_in_the_plan() -> TestResult {
    use mm::domain::discovery::Reach;
    assert_eq!(Balance::LocalFirst.reach(), Reach::LocalFirst);
    assert_eq!(Balance::Spread.reach(), Reach::EveryNode);
    let spread = IngressRoute {
        balance: Balance::Spread,
        ..route("mm-web", "/", true)
    };
    let sealed = SealedIngress {
        nodes: vec!["app-1".to_owned()],
        network: "0123456789ab-production-ingress".to_owned(),
        routes: vec![route("mm-api", "/api", true), spread.clone()],
        certificate: None,
        tunnel: None,
    };
    assert_eq!(
        route_lines(&sealed),
        [
            "https://shop.example.com/api -> app/web port 8080",
            "https://shop.example.com -> app/web port 8080, spread over every node",
        ]
    );
    let local = serde_json::to_value(route("mm-web", "/", true))?;
    assert!(
        local.get("balance").is_none(),
        "a local-first route is written as before"
    );
    let written = serde_json::to_string(&spread)?;
    assert!(written.contains(r#""balance":"spread""#));
    assert_eq!(serde_json::from_str::<IngressRoute>(&written)?, spread);
    Ok(())
}

#[test]
fn the_proxy_runs_read_only_with_no_capability_and_no_engine_socket() -> TestResult {
    let served = IngressTable {
        network: "0123456789ab-production-ingress".to_owned(),
        routes: vec![route("mm-web", "/", true)],
        certificate: None,
        engine_network: None,
        tunnel: None,
    };
    let text = proxy_compose("/var/lib/mm/ingress", "traefik:v3", &[("shop", &served)])?;
    let compose: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text)?;
    let proxy = &compose["services"]["proxy"];
    assert_eq!(compose["name"], "mm-ingress");
    assert_eq!(proxy["read_only"], true);
    assert_eq!(proxy["cap_drop"][0], "ALL");
    assert!(
        proxy["cap_add"].is_null(),
        "its ports inside are unprivileged"
    );
    assert_eq!(proxy["security_opt"][0], "no-new-privileges:true");
    // On Podman hosts too it reads the root-owned files mmd keeps for it.
    assert_eq!(proxy["userns_mode"], "host");
    assert!(!text.contains("docker.sock") && !text.contains("/var/run"));
    let volumes = proxy["volumes"]
        .as_sequence()
        .ok_or("no volumes")?
        .iter()
        .filter_map(serde_yaml_ng::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        volumes,
        [
            "/var/lib/mm/ingress/dynamic:/etc/traefik/dynamic:ro",
            "/var/lib/mm/ingress/certificates:/etc/traefik/certificates:ro",
            "/var/lib/mm/ingress/acme:/var/lib/traefik",
        ]
    );
    assert_eq!(proxy["tmpfs"][0], "/tmp");
    assert_eq!(proxy["ports"][2], "127.0.0.1:7954:8082");
    Ok(())
}

#[test]
fn a_sealed_ingress_refuses_routes_without_a_node_and_routes_mm_never_writes() {
    let ingress = |nodes: Vec<&str>, routes| SealedIngress {
        nodes: nodes.into_iter().map(str::to_owned).collect(),
        network: "net".to_owned(),
        routes,
        certificate: None,
        tunnel: None,
    };
    assert!(
        ingress(vec!["app-1"], vec![route("mm-web", "/", true)])
            .validate()
            .is_ok()
    );
    assert!(
        ingress(Vec::new(), Vec::new()).validate().is_ok(),
        "no route anywhere"
    );
    assert!(
        ingress(Vec::new(), vec![route("mm-web", "/", true)])
            .validate()
            .is_err()
    );
    for broken in [
        route("Mm Web", "/", true),
        route("mm-web", "/../etc", true),
        route("mm-web", "/api/", true),
        IngressRoute {
            host: "shop.example.com`) || Host(`other".to_owned(),
            ..route("mm-web", "/", true)
        },
        IngressRoute {
            port: 0,
            ..route("mm-web", "/", true)
        },
    ] {
        assert!(
            ingress(vec!["app-1"], vec![broken.clone()])
                .validate()
                .is_err(),
            "{broken:?}"
        );
    }
}

#[test]
fn a_leave_waits_for_every_node_then_its_drain_then_stops() {
    let mut record = LeaveRecord::new(
        "0123456789abcdef0123456789abcdef",
        "shop-web-1",
        Duration::from_secs(60),
        1_000,
    );
    assert!(record.validate().is_ok());
    assert_eq!(record.short_id(), "0123456789ab");
    assert_eq!(
        record.stage(500_000),
        LeaveStage::Marked,
        "not confirmed yet"
    );
    record.confirmed_unix_ms = Some(10_000);
    assert_eq!(
        record.stage(69_999),
        LeaveStage::Draining {
            until_unix_ms: 70_000
        }
    );
    assert_eq!(record.stage(70_000), LeaveStage::Stopping);
    for id in ["../../etc/passwd", "0123", "zzzzzzzzzzzzzz"] {
        assert!(
            LeaveRecord::new(id, "x", DEFAULT_DRAIN, 0)
                .validate()
                .is_err(),
            "{id}"
        );
    }
}

#[test]
fn a_restart_leaves_the_same_way_and_a_record_without_it_is_a_removal() -> TestResult {
    let mut record = LeaveRecord::new(
        "0123456789abcdef0123456789abcdef",
        "shop-web-1",
        Duration::from_secs(60),
        1_000,
    );
    record.confirmed_unix_ms = Some(10_000);
    let restarting = record.clone().restarting();
    assert_eq!(restarting.stage(70_000), LeaveStage::Stopping);
    let written = serde_json::to_string(&record)?;
    assert!(!written.contains("restart"), "{written}");
    let read: LeaveRecord = serde_json::from_str(&serde_json::to_string(&restarting)?)?;
    assert!(read.restart);
    let older: LeaveRecord = serde_json::from_str(&written)?;
    assert!(!older.restart);
    Ok(())
}

#[test]
fn a_drain_is_a_minute_unless_the_service_says_otherwise() -> TestResult {
    assert_eq!(drain_of(None)?, Duration::from_secs(60));
    assert_eq!(
        drain_of(Some(&DurationSpec("0s".to_owned())))?,
        Duration::ZERO
    );
    assert_eq!(
        drain_of(Some(&DurationSpec("90s".to_owned())))?,
        Duration::from_secs(90)
    );
    let long = drain_of(Some(&DurationSpec("1h".to_owned())))
        .err()
        .unwrap_or_default();
    assert!(long.contains("15m"), "{long}");
    let policy = RolloutPolicy::from_config(&config("", ROUTE)?, "production")?;
    assert_eq!(policy.drain("app", "web"), Duration::from_secs(40));
    assert_eq!(policy.drain("app", "worker"), DEFAULT_DRAIN);
    Ok(())
}

#[test]
fn an_environment_names_the_secrets_of_its_certificate() -> TestResult {
    let environment: EnvironmentConfig =
        serde_yaml_ng::from_str("target: live\ntls: {certificate: tls.cert, key: tls.key}\n")?;
    let tls = environment.tls.ok_or("no tls")?;
    assert_eq!(
        (tls.certificate.as_str(), tls.key.as_str()),
        ("tls.cert", "tls.key")
    );
    assert!(serde_yaml_ng::from_str::<EnvironmentConfig>("tls: {certificate: a}\n").is_err());
    Ok(())
}

#[test]
fn a_node_tells_the_others_what_leaves_and_what_it_confirmed() -> TestResult {
    let sent: NodeRecords = serde_json::from_str(
        r#"{"node": "app-1", "containers": [], "leaving": ["0123456789ab"], "confirmed": ["abcdefabcdef"]}"#,
    )?;
    assert_eq!(sent.for_peers().leaving, ["0123456789ab"]);
    assert_eq!(
        sent.for_peers().confirmed,
        Some(vec!["abcdefabcdef".to_owned()])
    );
    // A node of an earlier mm says nothing about either.
    let older: NodeRecords = serde_json::from_str(r#"{"node": "app-2", "containers": []}"#)?;
    assert!(older.leaving.is_empty());
    assert_eq!(older.confirmed, None);
    assert_eq!(PROXY_MEMBER, "mm-ingress-proxy");
    Ok(())
}

/// What Traefik keeps in `acme.json`: an account and one certificate per
/// host it ordered, under the resolver's name.
fn acme_file(hosts: &[&str]) -> String {
    let certificates = hosts
        .iter()
        .map(|host| {
            format!(
                r#"{{"domain": {{"main": "{host}"}}, "certificate": "Y2VydA==", "key": "a2V5", "Store": "default"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"{{"mm-http": {{"Account": {{"Email": "", "Registration": {{"uri": "https://acme.test/acct/1"}}, "PrivateKey": "cHJpdmF0ZQ==", "KeyType": "4096"}}, "Certificates": [{certificates}]}}}}"#
    )
}

fn acme_hosts(file: &[u8]) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file: serde_json::Value = serde_json::from_slice(file)?;
    Ok(file["mm-http"]["Certificates"]
        .as_array()
        .ok_or("no certificates")?
        .iter()
        .filter_map(|certificate| certificate["domain"]["main"].as_str().map(str::to_owned))
        .collect())
}

#[test]
fn a_host_that_provides_its_certificate_leaves_the_acme_file() -> TestResult {
    let provided = IngressTable {
        network: "0123456789ab-production-ingress".to_owned(),
        routes: vec![route("mm-web", "/", true), route("mm-api", "/api", false)],
        certificate: Some("shop-production-0123456789ab".to_owned()),
        engine_network: None,
        tunnel: None,
    };
    let acme = IngressTable {
        network: "0123456789ab-staging-ingress".to_owned(),
        routes: vec![IngressRoute {
            router: "mm-staging".to_owned(),
            host: "staging.example.com".to_owned(),
            ..route("mm-staging", "/", true)
        }],
        certificate: None,
        engine_network: None,
        tunnel: None,
    };
    let tables = [
        ("shop-production".to_owned(), provided),
        ("shop-staging".to_owned(), acme),
    ];
    let hosts = provided_hosts(&tables);
    assert_eq!(hosts, BTreeSet::from(["shop.example.com".to_owned()]));

    let stored = acme_file(&["shop.example.com", "staging.example.com"]);
    let forgotten = forget_hosts(stored.as_bytes(), &hosts)?.ok_or("nothing forgotten")?;
    assert_eq!(forgotten.domains, ["shop.example.com"]);
    assert_eq!(acme_hosts(&forgotten.file)?, ["staging.example.com"]);
    let file: serde_json::Value = serde_json::from_slice(&forgotten.file)?;
    assert_eq!(
        file["mm-http"]["Account"]["Registration"]["uri"], "https://acme.test/acct/1",
        "the ACME account stays"
    );
    // Once it is out, or never in, there is nothing to forget.
    assert_eq!(forget_hosts(&forgotten.file, &hosts)?, None);
    assert_eq!(forget_hosts(b"", &hosts)?, None);
    assert_eq!(forget_hosts(stored.as_bytes(), &BTreeSet::new())?, None);
    assert!(forget_hosts(b"{not json", &hosts).is_err());

    // The keeper finds which file the proxy must run after a restart.
    let text = render_dynamic(&tables, &BTreeMap::new(), 41)?;
    assert_eq!(loaded_marker(&text), Some(41));
    Ok(())
}

#[derive(Default)]
struct Proxy {
    commands: Mutex<Vec<Vec<String>>>,
}

#[async_trait::async_trait]
impl ProcessRunner for Proxy {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        self.commands
            .lock()
            .map_err(|_| MmError::State {
                message: "the test engine's lock was poisoned".to_owned(),
            })?
            .push(specification.redacted_record().arguments);
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(String::new()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

/// A proxy's check port that answers the file it runs and its ping.
async fn check_port(marker: u64) -> Result<u16, Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut request = [0_u8; 256];
            let read = stream.read(&mut request).await.unwrap_or_default();
            let line = String::from_utf8_lossy(&request[..read]).into_owned();
            let status = if line.starts_with(&format!("GET /mm-loaded/{marker} ")) {
                "200 OK"
            } else {
                "404 Not Found"
            };
            let _ = stream
                .write_all(format!("HTTP/1.0 {status}\r\n\r\n").as_bytes())
                .await;
        }
    });
    Ok(port)
}

#[tokio::test]
async fn the_proxy_restarts_once_to_forget_the_acme_certificate_of_a_provided_host() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let root = directory.path().join("ingress");
    for sub in ["routes", "dynamic", "acme"] {
        std::fs::create_dir_all(root.join(sub))?;
    }
    // The domain moved from ACME to the environment's certificate: the
    // proxy runs the new routes, and its ACME file still holds the old
    // certificate.
    let table = IngressTable {
        network: "0123456789ab-production-ingress".to_owned(),
        routes: vec![route("mm-web", "/", true)],
        certificate: Some("shop-production-0123456789ab".to_owned()),
        engine_network: None,
        tunnel: None,
    };
    std::fs::write(
        root.join("routes/shop-production.json"),
        serde_json::to_vec(&table)?,
    )?;
    let tables = [("shop-production".to_owned(), table)];
    std::fs::write(
        root.join("dynamic/mm.yaml"),
        render_dynamic(&tables, &BTreeMap::new(), 3)?,
    )?;
    let acme = root.join("acme/acme.json");
    std::fs::write(&acme, acme_file(&["shop.example.com", "other.example.com"]))?;
    let options = IngressOptions {
        image: "traefik@sha256:abc".to_owned(),
        check_port: check_port(3).await?,
        ..IngressOptions::on_node(directory.path(), RuntimeKind::DockerCompose)
    };
    let proxy = Proxy::default();

    let forgotten = forget_provided_certificates(&options, &proxy).await?;
    assert_eq!(forgotten, ["shop.example.com"]);
    assert_eq!(acme_hosts(&std::fs::read(&acme)?)?, ["other.example.com"]);
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&acme)?.permissions().mode() & 0o777,
            0o600
        );
    }
    let compose = root.join("compose.yaml").to_string_lossy().into_owned();
    let commands = proxy.commands.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(
        commands,
        [
            vec![
                "compose",
                "--project-name",
                "mm-ingress",
                "--file",
                compose.as_str(),
                "stop"
            ],
            vec![
                "compose",
                "--project-name",
                "mm-ingress",
                "--file",
                compose.as_str(),
                "up",
                "--detach",
                "--remove-orphans"
            ],
        ],
        "stopped before its file changes, so it cannot write the certificate back"
    );

    // The next deployment finds nothing to forget and leaves the proxy be.
    assert!(
        forget_provided_certificates(&options, &proxy)
            .await?
            .is_empty()
    );
    assert_eq!(proxy.commands.lock().map_err(|_| "poisoned")?.len(), 2);

    // A file the proxy cannot read either is its own: mm leaves it and
    // the proxy alone, and publishes the routes.
    std::fs::write(&acme, b"{not json")?;
    assert!(
        forget_provided_certificates(&options, &proxy)
            .await?
            .is_empty()
    );
    assert_eq!(std::fs::read(&acme)?, b"{not json");
    assert_eq!(proxy.commands.lock().map_err(|_| "poisoned")?.len(), 2);
    Ok(())
}

#[test]
fn a_cluster_plan_counts_the_routes_its_ingress_nodes_serve() -> TestResult {
    let mut plan: mm::domain::remote::ClusterDeploymentPlan =
        serde_json::from_value(serde_json::json!({
            "plan_hash": "0a4b",
            "target": "production",
            "environment": "production",
            "commit": "2e44",
            "expires_at_unix_ms": 0,
            "nodes": [],
        }))?;
    assert_eq!(plan.route_count(), 0);
    plan.ingress = Some(SealedIngress {
        nodes: vec!["app-1".to_owned()],
        network: INGRESS_NETWORK.to_owned(),
        routes: vec![route("web", "/", true), route("api", "/api", true)],
        certificate: None,
        tunnel: None,
    });
    // No node section claims a route on a cluster: the review counts the proxy's.
    assert_eq!(plan.route_count(), 2);
    Ok(())
}
