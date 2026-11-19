use std::fs;

use super::*;
use crate::domain::config::MmConfig;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(update: &str, extra: &str) -> String {
    format!(
        "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
{extra}
environments:
  production: {{}}
modules:
  shop:
    path: .
    services:
      web: {{stack: app, update: {update}}}
"
    )
}

/// The rollout findings for a configuration and its Compose file.
fn findings(
    config: &str,
    compose: &str,
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
    validate_rollout(
        &ValidationContext::new(&document, "production"),
        &mut report,
    );
    Ok(report
        .diagnostics
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.message))
        .collect())
}

const PLAIN: &str = "services:\n  web:\n    image: nginx\n";

#[test]
fn every_update_key_is_optional_and_start_first_is_fine_for_a_plain_service() -> TestResult {
    assert_eq!(findings(&config("{}", ""), PLAIN)?, []);
    assert_eq!(
        findings(
            &config("{order: start-first, parallelism: 2, failure: pause}", ""),
            "services:\n  web:\n    image: nginx\n    ports: [\"8080\"]\n"
        )?,
        [],
        "a port the engine picks on the host never collides"
    );
    Ok(())
}

#[test]
fn start_first_is_refused_where_two_containers_cannot_run_side_by_side() -> TestResult {
    let start_first = config("{order: start-first}", "");
    for (compose, reason) in [
        (
            "services:\n  web:\n    image: nginx\n    ports: [\"80:8080\"]\n",
            "publishes host port 80",
        ),
        (
            "services:\n  web:\n    image: nginx\n    ports: [\"127.0.0.1:8443:443/tcp\"]\n",
            "publishes host port 8443",
        ),
        (
            "services:\n  web:\n    image: nginx\n    ports:\n      - {target: 80, published: 8080}\n",
            "publishes host port 8080",
        ),
        (
            "services:\n  web:\n    image: nginx\n    container_name: shop-web\n",
            "sets `container_name: shop-web`",
        ),
        (
            "services:\n  web:\n    image: nginx\n    scale: 2\n",
            "runs 2 containers",
        ),
    ] {
        let found = findings(&start_first, compose)?;
        assert_eq!(found.len(), 1, "{compose}: {found:?}");
        let (code, message) = &found[0];
        assert_eq!(code, "MM_UPDATE_ORDER");
        assert!(message.contains(reason), "{reason}: {message}");
        assert!(
            message.contains("modules.shop.services.web.update.order is start-first"),
            "{message}"
        );
        assert!(message.contains("remove `order`"), "the way out: {message}");
    }
    // The same Compose file is fine when the old container stops first.
    assert_eq!(
        findings(
            &config("{}", ""),
            "services:\n  web:\n    image: nginx\n    ports: [\"80:80\"]\n"
        )?,
        []
    );

    // Podman runs start-first too: both containers have the service's
    // fixed user ids. A Compose `userns_mode` other than `host` would give
    // the new one others.
    let podman = config("{order: start-first}", "runtime: {driver: podman-compose}");
    assert_eq!(findings(&podman, PLAIN)?, []);
    let own = "services:\n  web:\n    image: nginx\n    userns_mode: auto\n";
    let found = findings(&podman, own)?;
    assert!(
        found.len() == 1
            && found[0].1.contains("sets `userns_mode: auto`")
            && found[0]
                .1
                .contains("could not change what the old one wrote"),
        "{found:?}"
    );
    let host = own.replace("userns_mode: auto", "userns_mode: host");
    assert_eq!(findings(&podman, &host)?, []);
    assert_eq!(findings(&start_first, own)?, []);
    Ok(())
}

#[test]
fn stack_order_timeout_and_parallelism_say_what_to_write() -> TestResult {
    let found = findings(&config("{parallelism: 0}", ""), PLAIN)?;
    assert!(
        found
            .iter()
            .any(|(code, message)| code == "MM_UPDATE_ORDER" && message.contains("at least 1")),
        "{found:?}"
    );

    let looped = config("{}", "  db:\n    files: [compose.yaml]\n    after: [db]");
    let found = findings(&looped, PLAIN)?;
    assert!(
        found.iter().any(|(code, message)| code == "MM_STACK_ORDER"
            && message.contains("lists the stack itself")),
        "{found:?}"
    );

    let timeout = "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: shop
target: local
stacks:
  app:
    files: [compose.yaml]
environments:
  production: {deploy: {convergence-timeout: soon}}
";
    let found = findings(timeout, PLAIN)?;
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "MM_DEPLOY_TIMEOUT");
    assert!(
        found[0].1.contains("such as 90s, 10m, or 1h"),
        "{}",
        found[0].1
    );
    Ok(())
}
