use std::fs;

use super::*;
use crate::domain::config::MmConfig;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const CONFIG: &str = "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
environments:
  local: {}
  production: {target: production}
targets:
  production:
    nodes:
      - {name: app-1, host: 203.0.113.11}
      - {name: app-2, host: 203.0.113.12}
modules:
  shop:
    path: .
    services:
      web: {stack: app}
      preview: {stack: app, environments: [local]}
";

/// The findings for `compose` in `environment`, as `(code, message)`.
fn findings(
    compose: &str,
    environment: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    findings_with(CONFIG, compose, environment)
}

fn findings_with(
    config: &str,
    compose: &str,
    environment: &str,
) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    fs::write(root.path().join("compose.yaml"), compose)?;
    let document = ConfigDocument {
        config: serde_yaml_ng::from_str::<MmConfig>(config)?,
        root: root.path().to_path_buf(),
        path: root.path().join(".config/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    };
    let mut report = ValidationReport::default();
    validate_cluster_compose(&ValidationContext::new(&document, environment), &mut report);
    Ok(report
        .diagnostics
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message))
        .collect())
}

#[test]
fn every_swarm_only_key_says_what_to_write_in_mm_yaml() -> TestResult {
    let found = findings(
        "services:
  web:
    image: example/web
    deploy:
      mode: global
      replicas: 3
      endpoint_mode: dnsrr
      placement:
        constraints: [node.role == worker]
        max_replicas_per_node: 1
",
        "production",
    )?;
    assert_eq!(found.len(), 4, "{found:?}");
    assert!(
        found
            .iter()
            .all(|(code, _)| code == "MM_COMPOSE_DEPLOY_KEY")
    );
    let all = found
        .iter()
        .map(|(_, message)| message.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "`deploy.mode: global`, which only Docker Swarm reads. To run it on every node, write `placement: all` under modules.shop.services.web",
        "`deploy.replicas: 3`",
        "write `replicas: 3` under modules.shop.services.web in mm.yaml",
        "`deploy.endpoint_mode`",
        "mm has no virtual IPs; remove `deploy.endpoint_mode`",
        "`deploy.placement`, which only Docker Swarm reads. Write `placement:` under modules.shop.services.web",
        "a group from targets.production.cluster.groups",
    ] {
        assert!(all.contains(expected), "missing `{expected}` in:\n{all}");
    }
    Ok(())
}

#[test]
fn keys_compose_itself_honours_are_left_alone() -> TestResult {
    let compose = "services:
  web:
    image: example/web
    deploy:
      replicas: 1
      resources: {limits: {memory: 256M}}
      restart_policy: {condition: on-failure}
";
    assert!(findings(compose, "production")?.is_empty());
    Ok(())
}

#[test]
fn a_single_host_environment_keeps_reading_the_same_compose_file() -> TestResult {
    let swarm = "services:\n  web: {image: example/web, deploy: {mode: global, replicas: 4}}\n  cache: {image: example/cache}\n";
    assert!(
        findings(swarm, "local")?.is_empty(),
        "one host: Compose reads these keys and placement is not involved"
    );
    Ok(())
}

#[test]
fn a_compose_service_mm_yaml_does_not_list_cannot_be_placed() -> TestResult {
    let found = findings(
        "services:\n  web: {image: example/web}\n  cache: {image: example/cache}\n",
        "production",
    )?;
    assert_eq!(found.len(), 1, "{found:?}");
    let (code, message) = &found[0];
    assert_eq!(code, "MM_PLACEMENT");
    assert!(
        message.contains("Compose service 'cache' of stack 'app' has no entry under modules"),
        "{message}"
    );
    assert!(
        message.contains("modules.<module>.services.cache: {stack: app}"),
        "{message}"
    );
    assert!(message.contains("first node (app-1)"), "{message}");
    Ok(())
}

#[test]
fn a_service_the_environment_leaves_out_is_not_checked() -> TestResult {
    let compose = "services:\n  web: {image: example/web}\n  preview: {image: example/web, deploy: {mode: global}}\n";
    assert!(findings(compose, "production")?.is_empty());
    Ok(())
}

#[test]
fn a_resolver_key_is_refused_where_the_node_answers_names() -> TestResult {
    let compose =
        "services:\n  web: {image: example/web, dns: [192.0.2.53], dns_search: corp.example}\n";
    // No mm network: the engine's own resolver stays in charge.
    assert!(findings(compose, "production")?.is_empty());
    assert!(findings(compose, "local")?.is_empty());

    let on_network = CONFIG.replace(
        "      web: {stack: app}",
        "      web: {stack: app, networks: [backend]}",
    ) + "networks:\n  backend: {}\n";
    let found = findings_with(&on_network, compose, "production")?;
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|(code, _)| code == "MM_COMPOSE_DNS"));
    assert!(
        found[0].1.contains("service 'web' sets `dns`, which has no effect on target 'production'")
            && found[0].1.contains("Remove `dns`; to resolve other names differently, change /etc/resolv.conf on the nodes"),
        "{}",
        found[0].1
    );
    assert!(found[1].1.contains("`dns_search`"), "{}", found[1].1);
    assert!(findings_with(&on_network, compose, "local")?.is_empty());

    // An existing private interface: networks stay on each host.
    let own_interface = on_network.replace(
        "    nodes:\n",
        "    cluster: {mesh: {interface: eth1}}\n    nodes:\n",
    );
    assert!(findings_with(&own_interface, compose, "production")?.is_empty());
    Ok(())
}
