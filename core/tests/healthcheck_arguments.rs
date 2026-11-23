//! `mm config validate` warns about a `CMD` health check argument that
//! Podman's Docker API would split at whitespace.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{IngressRouteRegistry, TargetStore};
use mm::app::{Application, ApplicationDependencies, StackManager, WorkspaceSelection};
use mm::domain::diagnostic::{Diagnostic, Severity};
use mm::domain::runtime::{RuntimeKind, RuntimeProbe};
use mm::domain::target::{TargetDefinition, TargetId};

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMPOSE: &str = r#"services:
  web:
    image: example/web
    healthcheck:
      test: ["CMD", "node", "-e", 'process.exit(require("fs").existsSync("/tmp/ready") ? 0 : 1)']
  api:
    image: example/api
    healthcheck:
      test: ["CMD", "curl", "-f", "http://127.0.0.1:8080/health"]
  worker:
    image: example/worker
    healthcheck:
      test: ["CMD-SHELL", "python3 -c 'import sys; sys.exit(0)'"]
  probe:
    image: example/probe
    healthcheck:
      test: ["CMD", "check", ""]
  db:
    image: example/db
    healthcheck:
      test: ["CMD", "sh", "-c", "pg_isready -U app"]
"#;

fn write_project(root: &Path) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(root.join("compose.yaml"), COMPOSE)?;
    fs::write(
        root.join(".config/mm.yaml"),
        "schema-version: 1
project: {id: 019c527b-6242-7b43-bfd4-bd60e27c1de3, name: shop, development-environment: dev}
default-environment: local
target: local
runtime: {driver: podman-compose}
stacks: {app: {files: [compose.yaml]}}
modules:
  app:
    path: app
    services:
      web: {stack: app}
      db: {stack: app, environments: [staging]}
environments:
  dev: {bindings: {}}
  local: {bindings: {}}
  staging: {bindings: {}}
  docker:
    runtime: {driver: docker-compose}
    bindings: {}
",
    )?;
    Ok(())
}

fn application(home: &Path) -> Result<Application, Box<dyn std::error::Error>> {
    fs::create_dir_all(home)?;
    let state = Arc::new(SqliteStateStore::open(&home.join("state.sqlite3"))?);
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    let documents = Arc::new(ConfigLoader);
    let route_registry: Arc<dyn IngressRouteRegistry> = state.clone();
    let stacks = StackManager::with_route_registry(
        documents.clone(),
        Arc::new(FilesystemStackArtifactStore::new(home.join("compose"))),
        route_registry,
        targets,
    );
    Ok(Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime: Arc::new(FakeRuntime {
            probe: RuntimeProbe {
                kind: RuntimeKind::PodmanCompose,
                available: false,
                engine_version: None,
                compose_provider: None,
                rootless: None,
                socket_path: None,
                capabilities: Vec::new(),
                diagnostics: Vec::new(),
            },
            user_rendered: None,
            rendered: BTreeMap::new(),
            observed: BTreeMap::new(),
        }),
        state,
        stacks,
        clock: Arc::new(FixedClock { unix_ms: 1_000 }),
    }))
}

fn warnings(
    application: &Application,
    project: &Path,
    environment: &str,
) -> Result<Vec<Diagnostic>, Box<dyn std::error::Error>> {
    let snapshot = application.validate(&WorkspaceSelection::new(
        project,
        Some(environment.to_owned()),
    ))?;
    Ok(snapshot
        .validation
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == "MM_HEALTHCHECK_ARGUMENTS")
        .collect())
}

#[test]
fn config_validate_names_each_podman_cmd_check_with_whitespace_arguments() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let project = temporary.path().join("project");
    write_project(&project)?;
    let application = application(&temporary.path().join("home"))?;

    let local = warnings(&application, &project, "local")?;
    assert_eq!(local.len(), 2, "{local:#?}");
    assert!(
        local[0].message.starts_with(
            "services.web.healthcheck.test in compose.yaml (stack 'app') is a `CMD` check"
        ),
        "{}",
        local[0].message
    );
    assert!(
        local[1]
            .message
            .starts_with("services.probe.healthcheck.test"),
        "{}",
        local[1].message
    );
    assert_eq!(local[0].severity, Severity::Warning);
    assert_eq!(
        local[0].path.as_deref(),
        Some(project.join("compose.yaml").as_path())
    );
    assert_eq!(
        local[0].help.as_deref(),
        Some(
            r#"write `test: ["CMD-SHELL", "node -e 'process.exit(require(\"fs\").existsSync(\"/tmp/ready\") ? 0 : 1)'"]` (it runs with /bin/sh in the container), or run Podman 5.8 or later"#
        )
    );
    assert_eq!(
        local[1].help.as_deref(),
        Some(
            r#"write `test: ["CMD-SHELL", "check ''"]` (it runs with /bin/sh in the container), or run Podman 5.8 or later"#
        )
    );

    let staging = warnings(&application, &project, "staging")?;
    assert_eq!(staging.len(), 3, "{staging:#?}");
    assert!(
        staging
            .iter()
            .any(|diagnostic| diagnostic.message.starts_with("services.db.")),
        "{staging:#?}"
    );

    assert_eq!(warnings(&application, &project, "docker")?, Vec::new());
    Ok(())
}
