//! Services with `role: job`, which only their jobs run.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use mm::adapters::config::{ConfigLoader, validate};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::TargetStore;
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::runtime::{ServiceState, state_across_nodes};
use mm::domain::target::{TargetDefinition, TargetId};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMPOSE: &str = "services:\n  web:\n    image: example/web\n  ops:\n    image: example/ops\n";

/// A project with a `web` service and an `ops` service with `role: job`
/// and the keys `ops`, `jobs` as its jobs, in two environments.
fn project(compose: &str, ops: &str, jobs: &str) -> Result<TempDir, Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let root = temporary.path();
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(root.join("compose.yaml"), compose)?;
    fs::write(root.join("app/.env.example"), "")?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1\nproject:\n  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3\n  name: jobs\ndefault-environment: staging\ntarget: local\nruntime:\n  driver: docker-compose\nstacks:\n  app:\n    files: [compose.yaml]\nmodules:\n  app:\n    path: app\n    env:\n      template: .env.example\n    services:\n      web:\n        stack: app\n      ops:\n        stack: app\n        role: job\n{ops}{jobs}environments:\n  staging:\n    bindings: {{}}\n  production:\n    bindings: {{}}\n"
        ),
    )?;
    Ok(temporary)
}

const BOOTSTRAP: &str = "jobs:\n  bootstrap:\n    module: app\n    service: ops\n    run: before-deploy\n    command: [\"/bin/true\"]\n";

/// The `MM_JOB_ONLY_SERVICE` findings of `environment`, as
/// `(is an error, message)`.
fn findings(
    root: &Path,
    environment: &str,
) -> Result<Vec<(bool, String)>, Box<dyn std::error::Error>> {
    let document = ConfigLoader.load_from(root)?;
    Ok(validate(&document, environment)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "MM_JOB_ONLY_SERVICE")
        .map(|diagnostic| {
            (
                diagnostic.severity == mm::domain::diagnostic::Severity::Error,
                diagnostic.message,
            )
        })
        .collect())
}

fn compiled_overlay(
    root: &Path,
    environment: &str,
) -> Result<serde_yaml_ng::Value, Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    store.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        store,
    );
    let report = manager.compile(&WorkspaceSelection::new(root, Some(environment.to_owned())))?;
    let overlay = fs::read_to_string(&report.stacks[0].generated_overlay_path)?;
    Ok(serde_yaml_ng::from_str(&overlay)?)
}

#[test]
fn a_job_service_with_a_job_validates_and_one_without_is_a_warning() -> TestResult {
    let with_job = project(COMPOSE, "", BOOTSTRAP)?;
    assert!(findings(with_job.path(), "staging")?.is_empty());
    let document = ConfigLoader.load_from(with_job.path())?;
    assert!(validate(&document, "staging").is_valid());

    let without = project(COMPOSE, "", "")?;
    let found = findings(without.path(), "staging")?;
    assert_eq!(found.len(), 1, "{found:?}");
    let (error, message) = &found[0];
    assert!(
        !error,
        "a job service without a job is a warning: {message}"
    );
    for expected in [
        "modules.app.services.ops has `role: job`",
        "no job runs in it",
        "`module: app` and `service: ops`",
        "or remove `role: job`",
    ] {
        assert!(message.contains(expected), "{expected}: {message}");
    }
    Ok(())
}

#[test]
fn what_only_a_running_container_uses_is_refused_with_what_to_change() -> TestResult {
    let cases = [
        (
            COMPOSE,
            "        replicas: 2\n",
            "sets `replicas`, but a deployment runs no container of it to count. Remove `replicas`",
        ),
        (
            COMPOSE,
            "        autoscale: {scale: {max: 3}}\n",
            "Remove `autoscale`",
        ),
        (
            COMPOSE,
            "        update: {order: start-first}\n",
            "sets `update.order: start-first`, but it has no running container to replace. Remove `update.order`",
        ),
        (
            "services:\n  web:\n    image: example/web\n  ops:\n    image: example/ops\n    ports: [\"8080:80\"]\n",
            "",
            "service 'ops' sets `ports`, and modules.app.services.ops has `role: job`",
        ),
        (
            "services:\n  web:\n    image: example/web\n    depends_on: [ops]\n  ops:\n    image: example/ops\n",
            "",
            "Remove 'ops' from the `depends_on` of 'web'",
        ),
        (
            "services:\n  web:\n    image: example/web\n    depends_on:\n      ops: {condition: service_healthy}\n  ops:\n    image: example/ops\n",
            "",
            "service 'web' depends on 'ops'",
        ),
    ];
    for (compose, ops, expected) in cases {
        let project = project(compose, ops, BOOTSTRAP)?;
        let found = findings(project.path(), "staging")?;
        assert!(
            found
                .iter()
                .any(|(error, message)| *error && message.contains(expected)),
            "{expected}: {found:?}"
        );
        let document = ConfigLoader.load_from(project.path())?;
        assert!(!validate(&document, "staging").is_valid(), "{expected}");
    }
    Ok(())
}

#[test]
fn a_route_to_a_job_service_is_refused() -> TestResult {
    let project = project(COMPOSE, "", BOOTSTRAP)?;
    let config_path = project.path().join(".config/mm.yaml");
    let config = fs::read_to_string(&config_path)?
        .replace(
            "    files: [compose.yaml]\n",
            "    files: [compose.yaml]\n    ingress:\n      domain: app\n      route: /\n      service: ops\n      port: 8080\n",
        )
        .replace(
            "  staging:\n    bindings: {}\n",
            "  staging:\n    domains: {app: app.example.test}\n    bindings: {}\n",
        );
    fs::write(&config_path, config)?;
    let found = findings(project.path(), "staging")?;
    assert!(
        found.iter().any(|(error, message)| *error
            && message.contains("sends requests to modules.app.services.ops")
            && message.contains("Route to another service")),
        "{found:?}"
    );
    Ok(())
}

#[test]
fn a_deployment_starts_no_container_of_a_job_service() -> TestResult {
    let project = project(COMPOSE, "", BOOTSTRAP)?;
    let overlay = compiled_overlay(project.path(), "staging")?;
    assert_eq!(
        overlay["services"]["ops"]["profiles"],
        serde_yaml_ng::Value::from(vec!["mm-job-only"]),
        "compose up leaves out a service in a profile no one enables: {overlay:?}"
    );
    assert!(
        overlay["services"]["web"].get("profiles").is_none(),
        "{overlay:?}"
    );
    Ok(())
}

#[test]
fn a_job_service_is_job_only_on_every_node_that_reports_it() {
    assert_eq!(
        state_across_nodes(&[ServiceState::JobOnly, ServiceState::Unknown]),
        ServiceState::JobOnly
    );
    assert_eq!(
        state_across_nodes(&[ServiceState::Running, ServiceState::Missing]),
        ServiceState::Degraded
    );
    assert!(ServiceState::JobOnly.is_up());
    assert!(!ServiceState::Missing.is_up());
}
