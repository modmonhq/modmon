//! `environments.<name>.healthchecks`: off by default for the development
//! environment on this machine, on everywhere else, and what the generated
//! overlay and a deployment's wait make of it.

use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm::adapters::config::ConfigLoader;
use mm::adapters::runtime::DockerComposeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{CommandOutput, CommandSpec, ProcessRunner, RuntimeDriver, TargetStore};
use mm::app::{StackManager, WorkspaceSelection};
use mm::domain::error::MmError;
use mm::domain::runtime::{
    ConvergenceCancellation, ConvergencePolicy, ConvergenceState, RuntimeKind, RuntimeTarget,
};
use mm::domain::target::{
    NamedTarget, TargetBootstrap, TargetDefinition, TargetId, TargetTransport,
};
use mm::domain::workspace::ProjectId;
use serde_yaml_ng::Value;
use uuid::Uuid;
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const COMPOSE: &str = "services:
  web:
    image: example/web
    healthcheck:
      test: [\"CMD-SHELL\", \"curl -f http://127.0.0.1:8080/health\"]
    depends_on:
      db:
        condition: service_healthy
      cache:
        condition: service_started
  db:
    image: example/db
    healthcheck:
      test: [\"CMD-SHELL\", \"pg_isready\"]
  cache:
    image: example/cache
";

fn write_project(root: &Path, development: &str, production: &str) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("app"))?;
    fs::write(root.join("compose.yaml"), COMPOSE)?;
    fs::write(
        root.join(".config/mm.yaml"),
        format!(
            "schema-version: 1
project:
  id: 019c527b-6242-7b43-bfd4-bd60e27c1de3
  name: shop
  development-environment: local
default-environment: local
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  app:
    path: app
    services:
      web: {{stack: app}}
      db: {{stack: app}}
      cache: {{stack: app}}
environments:
  local:
    bindings: {{}}
{development}  production:
    bindings: {{}}
{production}"
        ),
    )?;
    Ok(())
}

fn remote_target() -> Result<TargetDefinition, uuid::Error> {
    Ok(TargetDefinition {
        id: TargetId(Uuid::parse_str("419c527b-6242-7b43-bfd4-bd60e27c1de3")?),
        transport: TargetTransport::Ssh {
            host: "remote.example.test".to_owned(),
            port: 22,
            user: "mm".to_owned(),
            host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU".to_owned(),
            host_key: None,
            identity_file: None,
            remote_root: None,
            daemon_fingerprint: None,
        },
        bootstrap: TargetBootstrap::Ansible,
        ingress: None,
        default_runtime: Some(RuntimeKind::DockerCompose),
        available_runtimes: vec![RuntimeKind::DockerCompose],
        runtime: None,
        nodes: Vec::new(),
    })
}

/// The generated overlay of `environment` compiled for `target`.
fn overlay(
    project: &Path,
    environment: &str,
    target: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::parse_str(
            "319c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?)),
    )?;
    targets.insert_target("remote", remote_target()?)?;
    let manager = StackManager::new(
        Arc::new(ConfigLoader),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        targets,
    );
    let mut selection = WorkspaceSelection::new(project, Some(environment.to_owned()));
    selection.target = Some(target.to_owned());
    let report = manager.compile(&selection)?;
    Ok(serde_yaml_ng::from_str(&fs::read_to_string(
        &report.stacks[0].generated_overlay_path,
    )?)?)
}

fn service<'a>(overlay: &'a Value, name: &str) -> &'a Value {
    &overlay["services"][name]
}

fn checks_off(overlay: &Value) -> bool {
    ["web", "db", "cache"].iter().all(|name| {
        let service = service(overlay, name);
        service["healthcheck"]["disable"] == Value::Bool(true)
            && service["labels"]["io.mm.healthchecks"].as_str() == Some("off")
    })
}

fn checks_untouched(overlay: &Value) -> bool {
    ["web", "db", "cache"].iter().all(|name| {
        let service = service(overlay, name);
        service.get("healthcheck").is_none()
            && service.get("depends_on").is_none()
            && service["labels"].get("io.mm.healthchecks").is_none()
    })
}

#[test]
fn the_development_environment_on_this_machine_runs_without_health_checks() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let project = temporary.path().join("project");
    write_project(&project, "", "")?;

    let local = overlay(&project, "local", "local")?;
    assert!(checks_off(&local), "{local:#?}");
    assert_eq!(
        service(&local, "web")["depends_on"],
        serde_yaml_ng::from_str::<Value>("db: {condition: service_started}")?,
        "only the dependency that waited to be healthy changes"
    );
    assert!(service(&local, "db").get("depends_on").is_none());

    let production = overlay(&project, "production", "local")?;
    assert!(checks_untouched(&production), "{production:#?}");
    Ok(())
}

#[test]
fn the_development_environment_on_a_host_keeps_its_health_checks() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let project = temporary.path().join("project");
    write_project(&project, "", "")?;

    let remote = overlay(&project, "local", "remote")?;
    assert!(checks_untouched(&remote), "{remote:#?}");
    let production = overlay(&project, "production", "remote")?;
    assert!(checks_untouched(&production), "{production:#?}");
    Ok(())
}

#[test]
fn healthchecks_overrides_the_default_both_ways() -> TestResult {
    let temporary = temp_dirs::tempdir()?;
    let project = temporary.path().join("project");
    write_project(
        &project,
        "    healthchecks: true\n",
        "    healthchecks: false\n",
    )?;

    let local = overlay(&project, "local", "local")?;
    assert!(checks_untouched(&local), "{local:#?}");
    let production = overlay(&project, "production", "local")?;
    assert!(checks_off(&production), "{production:#?}");
    let remote = overlay(&project, "production", "remote")?;
    assert!(checks_off(&remote), "{remote:#?}");
    Ok(())
}

struct Answers(Mutex<VecDeque<String>>);

#[async_trait::async_trait]
impl ProcessRunner for Answers {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let stdout = self
            .0
            .lock()
            .map_err(|_| MmError::State {
                message: "answers lock was poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "no answer is left".to_owned(),
            })?;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

const RENDERED: &str = "services:
  api:
    image: example/api
    healthcheck:
      test: [\"CMD-SHELL\", \"curl -f http://127.0.0.1:8080/health\"]
      disable: true
    labels:
      io.mm.healthchecks: \"off\"
";

#[tokio::test]
async fn a_deployment_does_not_wait_for_a_check_that_is_off() -> TestResult {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-conformance");
    let listed = r#"[{"ID":"abc","Name":"shop-app-api-1","Image":"example/api","State":"running","Status":"Up 3 minutes","ExitCode":0,"Labels":"com.docker.compose.project=shop-app,com.docker.compose.service=api,io.mm.healthchecks=off"}]"#;
    let answers = Arc::new(Answers(Mutex::new(VecDeque::from([
        RENDERED.to_owned(),
        listed.to_owned(),
    ]))));
    let target = RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: Some(NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::from_u128(7))),
        )),
        project_id: ProjectId(Uuid::from_u128(8)),
        environment: "local".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop-app".to_owned(),
        artifact_hash: "fixture-hash".to_owned(),
        compose_files: vec![root.join("app.compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    };
    let outcome = DockerComposeRuntime::new(answers)
        .wait_for_services(
            &target,
            &["api".to_owned()],
            ConvergencePolicy {
                timeout: Duration::from_secs(30),
                initial_backoff: Duration::from_millis(5),
                maximum_backoff: Duration::from_millis(5),
            },
            ConvergenceCancellation::default(),
        )
        .await?;
    assert_eq!(outcome.state, ConvergenceState::Succeeded);
    assert_eq!(outcome.polls, 1);
    let container = &outcome.observation.containers[0];
    assert!(container.healthcheck_off(), "{container:#?}");
    assert_eq!(outcome.observation.services[0].healthy_count, 1);
    Ok(())
}
