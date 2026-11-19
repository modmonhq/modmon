use super::*;
use crate::domain::config::MmConfig;
use std::path::PathBuf;

fn document(root: PathBuf) -> Result<ConfigDocument, serde_yaml_ng::Error> {
    Ok(ConfigDocument {
        config: serde_yaml_ng::from_str::<MmConfig>(
            r#"
schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: routes
target: local
networks:
  frontend: {}
stacks:
  first:
    project-name: shared
    files: [compose.yaml, compose.yaml]
    networks: [frontend, frontend, missing]
    ingress:
      - domain: app
        route: invalid/
        service: absent
        port: 0
        https: false
        environments: [absent]
      - domain: missing
        route: /
  second:
    project-name: shared
    files: []
    ingress:
      - domain: app
        route: /
      - domain: app
        route: /
      - domain: invalid-host
        route: /invalid
environments:
  local:
    domains:
      app: mm.test
      invalid-host: Upper.test
    bindings: {}
"#,
        )?,
        root: root.clone(),
        path: root.join("mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
        unknown_keys: Vec::new(),
    })
}

#[test]
fn malformed_stacks_report_file_network_route_and_owner_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    std::fs::write(temporary.path().join("compose.yaml"), "services: {}\n")?;
    let document = document(temporary.path().to_path_buf())?;
    let mut report = ValidationReport::default();
    validate_stacks(&ValidationContext::new(&document, "local"), &mut report);
    let codes = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.as_str())
        .collect::<BTreeSet<_>>();
    for expected in [
        "MM_COMPOSE_FILE_DUPLICATE",
        "MM_COMPOSE_FILES_EMPTY",
        "MM_COMPOSE_PROJECT_NAME_DUPLICATE",
        "MM_NETWORK_DUPLICATE",
        "MM_NETWORK_UNKNOWN",
        "MM_INGRESS_ENVIRONMENT_UNKNOWN",
        "MM_INGRESS_PATH_INVALID",
        "MM_INGRESS_PORT_INVALID",
        "MM_INGRESS_HTTP_OPT_OUT",
        "MM_INGRESS_SERVICE_UNKNOWN",
        "MM_INGRESS_DOMAIN_UNKNOWN",
        "MM_INGRESS_HOST_INVALID",
        "MM_INGRESS_HOST_RESERVED",
        "MM_INGRESS_ROUTE_CONFLICT",
    ] {
        assert!(codes.contains(expected), "missing {expected}: {codes:?}");
    }
    Ok(())
}

#[test]
fn route_lexical_contracts_and_hash_boundaries_are_exact() {
    for host in ["app.test", "a-b.example", "1.example"] {
        assert!(is_route_host(host), "{host}");
    }
    for host in ["", "Upper.test", "-bad.test", "bad-.test", "bad..test"] {
        assert!(!is_route_host(host), "{host}");
    }
    for path in ["/", "/api", "/api/v1-resource_1~"] {
        assert!(is_route_path(path), "{path}");
    }
    for path in ["", "api", "/api/", "/api//v1", "/../api", "/api?q=1"] {
        assert!(!is_route_path(path), "{path}");
    }
    let first = ingress_route_name("app.test", "/api");
    assert_eq!(first, ingress_route_name("app.test", "/api"));
    assert_ne!(first, ingress_route_name("app.test", "/api/v2"));
    assert_ne!(first, ingress_route_name("api.test", "/api"));
}
