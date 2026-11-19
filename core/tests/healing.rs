//! A container whose health check keeps failing, restarted by its node:
//! the policy and its defaults, when a look restarts or stops, how a
//! container past its limit is answered, what status, logs and `mm doctor`
//! say, and what the node keeps on disk.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;

use mm::domain::cluster_network::{NetworkNeed, NetworkTable, NodeNetworks};
use mm::domain::discovery::{
    ContainerRecord, Endpoint, Health, NodeRecords, Reach, Resolution, ServiceMap,
};
use mm::domain::healing::{
    HealthConfig, HealthStep, OnFailure, RESTART_LIMIT, RestartEvent, RestartPolicy,
    ServiceRestarts, log_line, restart_limit_diagnostics, spoken,
};
use mm::domain::repo_target::Ipv4Cidr;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MINUTE: i64 = 60_000;

fn config(yaml: &str) -> Result<HealthConfig, serde_yaml_ng::Error> {
    serde_yaml_ng::from_str(yaml)
}

#[test]
fn a_service_without_health_restarts_after_three_minutes_at_most_five_times_an_hour() -> TestResult
{
    let policy = RestartPolicy::resolve(None, "modules.shop.services.web")
        .map_err(|problems| problems.join("; "))?;
    assert_eq!(
        policy,
        Some(RestartPolicy {
            after_ms: 180_000,
            max_restarts: 5,
            window_ms: 3_600_000,
        })
    );
    assert_eq!(
        policy.map(|policy| policy.describe()).as_deref(),
        Some("restart after 3m unhealthy, at most 5 times in 1h")
    );
    let sample = config("{on-failure: restart, after: 3m, max-restarts: 5, window: 1h}")?;
    assert_eq!(
        RestartPolicy::resolve(Some(&sample), "web").map_err(|problems| problems.join("; "))?,
        policy
    );
    let none = config("{on-failure: none}")?;
    assert_eq!(
        RestartPolicy::resolve(Some(&none), "web").map_err(|problems| problems.join("; "))?,
        None
    );
    assert_eq!(none.on_failure, Some(OnFailure::None));
    Ok(())
}

#[test]
fn every_problem_of_health_names_its_key() -> TestResult {
    let broken = config("{after: 2s, max-restarts: 0, window: soon}")?;
    let problems = RestartPolicy::resolve(Some(&broken), "modules.shop.services.web")
        .err()
        .ok_or("broken settings resolved")?;
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(
        problems[0].starts_with("modules.shop.services.web.health.window:"),
        "{problems:?}"
    );
    assert!(
        problems[1].contains("health.after is shorter than 10s"),
        "{problems:?}"
    );
    assert!(
        problems[2].contains("health.max-restarts is 0")
            && problems[2].contains("on-failure: none"),
        "{problems:?}"
    );
    assert!(
        config("{after: 3m, every: 1h}").is_ok(),
        "an unknown key is ignored"
    );
    Ok(())
}

#[test]
fn a_look_waits_restarts_and_stops_at_the_limit_within_the_window() {
    let policy = RestartPolicy {
        after_ms: 180_000,
        max_restarts: 2,
        window_ms: 3_600_000,
    };
    let now = 100 * MINUTE;
    assert_eq!(policy.step(179_999, &[], now), HealthStep::Wait);
    assert_eq!(
        policy.step(190_000, &[], now),
        HealthStep::Restart {
            reason: "unhealthy for 3m10s".to_owned()
        }
    );
    assert_eq!(
        policy.step(180_000, &[now - 10 * MINUTE], now),
        HealthStep::Restart {
            reason: "unhealthy for 3m".to_owned()
        }
    );
    assert_eq!(
        policy.step(180_000, &[now - 20 * MINUTE, now - 10 * MINUTE], now),
        HealthStep::Limit {
            reason: "unhealthy for 3m after 2 restarts within 1h".to_owned()
        }
    );
    assert_eq!(
        policy.recent(&[now - 61 * MINUTE, now - 10 * MINUTE], now),
        1
    );
    assert!(matches!(
        policy.step(180_000, &[now - 61 * MINUTE, now - 10 * MINUTE], now),
        HealthStep::Restart { .. }
    ));
    assert_eq!(spoken(90_000), "1m30s");
    assert_eq!(spoken(3_600_000), "1h");
    assert_eq!(spoken(500), "500ms");
}

const BACKEND: &str = "019c527b6242-production-backend";

fn networks() -> Result<NodeNetworks, String> {
    let space: Ipv4Cidr = "10.64.0.0/12".parse()?;
    let mut table = NetworkTable::default();
    for holder in ["app-1", "app-2"] {
        table.assign(
            space,
            &NetworkNeed {
                key: BACKEND,
                node: holder,
                network: &format!("mm-{holder}-backend"),
                internal: false,
            },
        )?;
    }
    Ok(NodeNetworks {
        node: "app-1".to_owned(),
        space,
        mesh_interface: "mm0".to_owned(),
        mtu: 1420,
        table,
    })
}

fn web(id: &str, third: u8, health: Health) -> ContainerRecord {
    ContainerRecord {
        name: format!("web-{id}"),
        id: format!("{id:0<12}"),
        service: Some("web".to_owned()),
        health,
        endpoints: vec![Endpoint {
            network: BACKEND.to_owned(),
            address: Ipv4Addr::new(10, 64, third, 2),
            aliases: Vec::new(),
        }],
        restart_limit: false,
    }
}

/// What `web` resolves to for the caller on app-1.
fn answers(local: &NodeRecords, peers: &[NodeRecords], networks: &NodeNetworks) -> Resolution {
    ServiceMap {
        networks,
        local,
        peers,
    }
    .resolve(Ipv4Addr::new(10, 64, 0, 9), "tasks.web")
}

fn caller() -> ContainerRecord {
    ContainerRecord {
        name: "caller".to_owned(),
        service: Some("caller".to_owned()),
        endpoints: vec![Endpoint {
            network: BACKEND.to_owned(),
            address: Ipv4Addr::new(10, 64, 0, 9),
            aliases: Vec::new(),
        }],
        ..web("ccc", 0, Health::Healthy)
    }
}

#[test]
fn a_container_past_its_limit_is_answered_only_when_its_service_has_no_other() -> TestResult {
    let networks = networks()?;
    let mut local = NodeRecords {
        node: "app-1".to_owned(),
        containers: vec![web("aaa", 0, Health::Healthy), caller()],
        ..NodeRecords::default()
    };
    let peer = NodeRecords {
        node: "app-2".to_owned(),
        containers: vec![web("bbb", 1, Health::Healthy)],
        ..NodeRecords::default()
    };
    local.hold(&BTreeSet::from([format!("{:0<64}", "aaa")]));
    assert!(
        local.containers[0].restart_limit,
        "held by its whole engine id"
    );
    assert_eq!(
        answers(&local, std::slice::from_ref(&peer), &networks),
        Resolution::Addresses(vec![Ipv4Addr::new(10, 64, 1, 2)])
    );
    let map = ServiceMap {
        networks: &networks,
        local: &local,
        peers: std::slice::from_ref(&peer),
    };
    assert_eq!(
        map.service_addresses(BACKEND, "web", Reach::EveryNode).0,
        [Ipv4Addr::new(10, 64, 1, 2)],
        "an ingress proxy does not route to it either"
    );
    let unhealthy_peer = NodeRecords {
        containers: vec![web("bbb", 1, Health::Unhealthy)],
        ..peer
    };
    assert_eq!(
        answers(&local, &[unhealthy_peer], &networks),
        Resolution::Addresses(vec![
            Ipv4Addr::new(10, 64, 0, 2),
            Ipv4Addr::new(10, 64, 1, 2)
        ])
    );
    local.hold(&BTreeSet::new());
    assert!(
        !local.containers[0].restart_limit,
        "a hold that ends frees it"
    );
    Ok(())
}

#[test]
fn a_held_container_travels_to_the_other_nodes_and_old_records_read_as_before() -> TestResult {
    let mut held = web("aaa", 0, Health::Healthy);
    held.restart_limit = true;
    let text = serde_json::to_string(&held)?;
    assert!(text.contains(r#""restart-limit":true"#), "{text}");
    assert_eq!(serde_json::from_str::<ContainerRecord>(&text)?, held);
    let plain = serde_json::to_string(&web("aaa", 0, Health::Healthy))?;
    assert!(!plain.contains("restart-limit"), "{plain}");
    Ok(())
}

fn event(node: &str, at: i64, reason: &str) -> RestartEvent {
    RestartEvent {
        node: Some(node.to_owned()),
        container: "shop-production-app-web-1".to_owned(),
        at_unix_ms: at,
        reason: reason.to_owned(),
    }
}

#[test]
fn status_logs_and_doctor_say_what_was_restarted_and_what_was_given_up() {
    let app_1 = ServiceRestarts {
        restarts: vec![event("app-1", 20, "unhealthy for 3m")],
        limited: Vec::new(),
    };
    let app_2 = ServiceRestarts {
        restarts: vec![event("app-2", 10, "unhealthy for 3m")],
        limited: vec![event(
            "app-2",
            30,
            "unhealthy for 3m after 5 restarts within 1h",
        )],
    };
    let merged = app_1.merged(app_2);
    assert_eq!(
        merged
            .restarts
            .iter()
            .map(|event| event.at_unix_ms)
            .collect::<Vec<_>>(),
        [10, 20],
        "oldest first, from every node"
    );
    assert_eq!(merged.limited.len(), 1);
    assert_eq!(
        log_line(&merged.restarts[0], false),
        "mm: restarted shop-production-app-web-1 after it was unhealthy for 3m"
    );
    assert_eq!(
        log_line(&merged.limited[0], true),
        format!(
            "mm: stopped restarting shop-production-app-web-1, unhealthy for 3m after 5 restarts within 1h ({RESTART_LIMIT})"
        )
    );
    let diagnostics = restart_limit_diagnostics(&[("web", &merged)]);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, RESTART_LIMIT);
    assert!(
        diagnostics[0].message.contains("on app-2")
            && diagnostics[0].message.contains("`mm service logs web`")
            && diagnostics[0].message.contains("`mm service restart web`"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_node_keeps_what_it_watches_and_what_it_did_until_the_environment_goes() -> TestResult {
    use mm::adapters::healing::store::{
        ContainerHistory, History, WatchedEnvironment, WatchedService, history, keep_history,
        keep_watched, watched,
    };
    let root = temp_dirs::tempdir()?;
    let root = root.path();
    let scope: mm::domain::remote::RemoteScope = serde_json::from_value(serde_json::json!({
        "project_id": "019c527b-6242-7b43-bfd4-bd60e27c1de3",
        "environment": "production",
        "target": "production",
        "target_fingerprint": "",
        "runtime": "docker-compose",
        "node": "app-1",
    }))?;
    let mut environment = WatchedEnvironment {
        scope: scope.clone(),
        root: "/srv/mm/projects/p/production/abc".into(),
        services: vec![WatchedService {
            stack: "app".to_owned(),
            service: "web".to_owned(),
            restart: Some(RestartPolicy::default()),
        }],
    };
    keep_watched(root, &environment)?;
    assert_eq!(watched(root)?, [environment.clone()]);
    let mut kept = History::default();
    kept.containers.insert(
        "f".repeat(64),
        ContainerHistory {
            stack: "app".to_owned(),
            service: "web".to_owned(),
            restarts: vec![event("app-1", 10, "unhealthy for 3m")],
            limit: None,
        },
    );
    keep_history(root, &scope, &kept)?;
    assert_eq!(history(root, &scope)?, kept);
    keep_history(root, &scope, &History::default())?;
    assert_eq!(history(root, &scope)?, History::default());
    environment.services.clear();
    keep_watched(root, &environment)?;
    assert!(
        watched(root)?.is_empty(),
        "a node that runs nothing of the environment watches none of it"
    );
    Ok(())
}

fn project(service: &str, target: &str) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let root = temp_dirs::tempdir()?;
    std::fs::create_dir_all(root.path().join(".config"))?;
    std::fs::write(
        root.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    std::fs::write(
        root.path().join(".config/mm.yaml"),
        format!(
            "schema-version: 1
default-environment: production
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: healed
target: {target}
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  web:
    path: .
    services:
      web:
        stack: app
{service}
targets:
  production:
    nodes:
      - {{name: app-1, host: 203.0.113.11}}
      - {{name: app-2, host: 203.0.113.12}}
  single:
    host: 203.0.113.20
environments:
  production: {{}}
"
        ),
    )?;
    Ok(root)
}

fn findings(root: &tempfile::TempDir) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let document = mm::adapters::config::ConfigLoader.load_from(root.path())?;
    let report = mm::adapters::config::validate(&document, "production");
    Ok(report
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "MM_HEALTH")
        .map(|diagnostic| diagnostic.message)
        .collect())
}

#[test]
fn health_validates_on_a_cluster_and_names_the_fix_on_one_host() -> TestResult {
    let sample = "        health: {on-failure: restart, after: 3m, max-restarts: 5, window: 1h}";
    assert_eq!(
        findings(&project(sample, "production")?)?,
        Vec::<String>::new()
    );
    let broken = findings(&project("        health: {max-restarts: 0}", "production")?)?;
    assert_eq!(broken.len(), 1, "{broken:?}");
    assert!(
        broken[0].starts_with("modules.web.services.web.health.max-restarts is 0"),
        "{broken:?}"
    );
    let one_host = findings(&project(sample, "single")?)?;
    assert_eq!(one_host.len(), 1, "{one_host:?}");
    assert!(
        one_host[0].contains("Write targets.single as a cluster"),
        "{one_host:?}"
    );
    assert_eq!(
        findings(&project("        health: {on-failure: none}", "single")?)?,
        Vec::<String>::new(),
        "turning it off is fine anywhere"
    );
    Ok(())
}
