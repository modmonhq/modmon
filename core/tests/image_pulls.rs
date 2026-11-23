//! Before a stack or a job starts, the images it lacks are pulled in a step
//! of their own, with the registry logins and under the deployment's pull
//! limit, so that `compose up` and `compose run` pull nothing.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm::adapters::runtime::{DockerComposeRuntime, PodmanComposeRuntime, RuntimeOperationsAdapter};
use mm::app::ports::{
    CommandOutput, CommandSpec, LocalDeploymentRuntime, ProcessRunner, ProcessStream,
    RedactedCommandRecord, RuntimeJobDriver, StreamCommandSpec, StreamingProcessRunner,
};
use mm::domain::error::MmError;
use mm::domain::observability::OperationCancellation;
use mm::domain::runtime::{RegistryCredential, RuntimeKind, RuntimeTarget};
use mm::domain::target::{NamedTarget, TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;
use zeroize::Zeroizing;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const CONFIG: &str = "name: shop
services:
  web:
    image: registry.example.com/shop/web:2
  api:
    image: registry.example.com/shop/api:1.4
  worker:
    image: registry.example.com/shop/api:1.4
  cache:
    image: redis:7
    pull_policy: always
  proxy:
    image: library-mirror/proxy:3
  db:
    image: postgres:17
    pull_policy: never
  built:
    image: registry.example.com/shop/built:1
    build: {context: .}
  local:
    image: mm.invalid/shop/local:production-0a1b2c3d4e5f6a7b8c9d
  feed:
    image: registry.example.com/shop/feed:3
    pull_policy: daily
  search:
    image: registry.example.com/shop/search:5
    pull_policy: if_not_present
";

enum Pull {
    Arrives,
    TooSlow,
    Denied,
}

struct Engine {
    present: BTreeSet<&'static str>,
    pull: Pull,
    records: Mutex<Vec<RedactedCommandRecord>>,
    limits: Mutex<Vec<Duration>>,
}

impl Engine {
    fn new(present: &[&'static str], pull: Pull) -> Arc<Self> {
        Arc::new(Self {
            present: present.iter().copied().collect(),
            pull,
            records: Mutex::new(Vec::new()),
            limits: Mutex::new(Vec::new()),
        })
    }

    fn limits(&self) -> Vec<Duration> {
        self.limits
            .lock()
            .map(|limits| limits.clone())
            .unwrap_or_default()
    }

    fn records(&self) -> Vec<RedactedCommandRecord> {
        self.records
            .lock()
            .map(|records| records.clone())
            .unwrap_or_default()
    }

    fn pulled(&self) -> Vec<String> {
        self.records()
            .into_iter()
            .filter(|record| record.arguments.first().map(String::as_str) == Some("pull"))
            .filter_map(|record| record.arguments.last().cloned())
            .collect()
    }
}

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        code: Some(code),
        stdout: Zeroizing::new(stdout.to_owned()),
        stderr: Zeroizing::new(stderr.to_owned()),
        truncated: false,
    }
}

#[async_trait::async_trait]
impl ProcessRunner for Engine {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let record = specification.redacted_record();
        self.limits
            .lock()
            .map_err(|_| MmError::State {
                message: "the fake engine's lock was poisoned".to_owned(),
            })?
            .push(specification.timeout_value());
        self.records
            .lock()
            .map_err(|_| MmError::State {
                message: "the fake engine's lock was poisoned".to_owned(),
            })?
            .push(record.clone());
        let arguments = record.arguments;
        match arguments.first().map(String::as_str) {
            Some("compose") if arguments.last().map(String::as_str) == Some("config") => {
                Ok(output(0, CONFIG, ""))
            }
            Some("image") => {
                let image = arguments.last().map(String::as_str).unwrap_or_default();
                Ok(output(i32::from(!self.present.contains(image)), "", ""))
            }
            Some("pull") => match self.pull {
                Pull::Arrives => Ok(output(0, "", "")),
                Pull::TooSlow => Err(MmError::ProcessTimeout {
                    program: record.program,
                    seconds: 2700,
                }),
                Pull::Denied => Ok(output(
                    1,
                    "",
                    "Trying to pull registry.example.com/shop/web:2...\nError: initializing source docker://registry.example.com/shop/web:2: reading manifest 2 in registry.example.com/shop/web: unauthorized: authentication required\n",
                )),
            },
            _ => Err(MmError::State {
                message: format!("the fake engine was not asked for {arguments:?}"),
            }),
        }
    }
}

struct NoStreams;

#[async_trait::async_trait]
impl StreamingProcessRunner for NoStreams {
    async fn spawn(
        &self,
        _specification: StreamCommandSpec,
        _cancellation: OperationCancellation,
    ) -> Result<ProcessStream, MmError> {
        Err(MmError::State {
            message: "a pull started a streaming process".to_owned(),
        })
    }
}

fn target(credentials: Vec<RegistryCredential>) -> RuntimeTarget {
    let root = PathBuf::from("/srv/shop");
    RuntimeTarget {
        root: root.clone(),
        project_directory: root.clone(),
        target: Some(NamedTarget::new(
            "local".to_owned(),
            TargetDefinition::local(TargetId(Uuid::from_u128(7))),
        )),
        project_id: ProjectId(Uuid::from_u128(8)),
        environment: "production".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "shop".to_owned(),
        artifact_hash: "fixture-hash".to_owned(),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: credentials,
    }
}

fn login() -> RegistryCredential {
    RegistryCredential {
        registry: "registry.example.com".to_owned(),
        username: "deploy".to_owned(),
        password: Zeroizing::new("fixture-password".to_owned()),
    }
}

#[tokio::test]
async fn podman_pulls_only_the_missing_images_by_their_full_names_with_the_logins() -> TestResult {
    let engine = Engine::new(
        &[
            "registry.example.com/shop/web:2",
            "redis:7",
            "registry.example.com/shop/feed:3",
            "registry.example.com/shop/search:5",
        ],
        Pull::Arrives,
    );
    PodmanComposeRuntime::with_executable(engine.clone(), "podman")
        .pull_images(&target(vec![login()]), None, Duration::from_secs(60))
        .await?;
    assert_eq!(
        engine.pulled(),
        [
            "registry.example.com/shop/api:1.4",
            "docker.io/library/redis:7",
            "docker.io/library-mirror/proxy:3",
            "registry.example.com/shop/feed:3",
        ],
        "web and search are there, api is pulled once for two services, cache and feed always, and nothing is pulled for a built, local, or never-pulled image"
    );
    for record in engine.records() {
        let pull = record.arguments.first().map(String::as_str) == Some("pull");
        assert_eq!(
            record
                .environment_keys
                .contains(&"REGISTRY_AUTH_FILE".to_owned()),
            pull,
            "only a pull gets the logins: {:?}",
            record.arguments
        );
    }
    Ok(())
}

#[tokio::test]
async fn named_services_pull_only_their_own_images_and_present_ones_pull_nothing() -> TestResult {
    let engine = Engine::new(&[], Pull::Arrives);
    PodmanComposeRuntime::with_executable(engine.clone(), "podman")
        .pull_images(
            &target(Vec::new()),
            Some(&["web".to_owned(), "db".to_owned()]),
            Duration::from_secs(60),
        )
        .await?;
    assert_eq!(engine.pulled(), ["registry.example.com/shop/web:2"]);

    let engine = Engine::new(&["registry.example.com/shop/web:2"], Pull::Arrives);
    DockerComposeRuntime::new(engine.clone())
        .pull_images(
            &target(Vec::new()),
            Some(&["web".to_owned()]),
            Duration::from_secs(60),
        )
        .await?;
    assert!(engine.pulled().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_pull_past_its_limit_names_the_key_that_raises_it() -> TestResult {
    let engine = Engine::new(&[], Pull::TooSlow);
    let error = DockerComposeRuntime::new(engine.clone())
        .pull_images(
            &target(Vec::new()),
            Some(&["api".to_owned(), "worker".to_owned()]),
            Duration::from_secs(45 * 60),
        )
        .await
        .err()
        .ok_or("a pull past its limit succeeded")?;
    assert_eq!(error.code(), "MM_TIMEOUT");
    let message = error.to_string();
    for part in [
        "registry.example.com/shop/api:1.4",
        "services 'api', 'worker' of stack 'app'",
        "within 45m (deploy.pull-timeout)",
        "nothing of the stack was started",
        "`environments.production.deploy.pull-timeout`",
    ] {
        assert!(message.contains(part), "missing {part:?} in: {message}");
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_pull_says_which_image_and_why() -> TestResult {
    let engine = Engine::new(&[], Pull::Denied);
    let error = PodmanComposeRuntime::with_executable(engine, "podman")
        .pull_images(
            &target(Vec::new()),
            Some(&["web".to_owned()]),
            Duration::from_secs(60),
        )
        .await
        .err()
        .ok_or("a refused pull succeeded")?;
    assert_eq!(error.code(), "MM_RUNTIME_LAUNCH");
    let message = error.to_string();
    for part in [
        "could not pull image registry.example.com/shop/web:2 for service 'web' of stack 'app'",
        "unauthorized: authentication required",
        "`registries`",
    ] {
        assert!(message.contains(part), "missing {part:?} in: {message}");
    }
    Ok(())
}

#[tokio::test]
async fn a_job_pulls_its_service_image_with_the_registry_logins() -> TestResult {
    for kind in [RuntimeKind::PodmanCompose, RuntimeKind::DockerCompose] {
        let engine = Engine::new(&[], Pull::Arrives);
        RuntimeOperationsAdapter::new(kind, engine.clone(), Arc::new(NoStreams))
            .pull_image(&target(vec![login()]), "api", Duration::from_secs(60))
            .await?;
        assert_eq!(engine.pulled(), ["registry.example.com/shop/api:1.4"]);
        let auth = match kind {
            RuntimeKind::PodmanCompose => "REGISTRY_AUTH_FILE",
            RuntimeKind::DockerCompose => "DOCKER_CONFIG",
        };
        assert!(
            engine
                .records()
                .iter()
                .filter(|record| record.arguments.first().map(String::as_str) == Some("pull"))
                .all(|record| record.environment_keys.contains(&auth.to_owned())),
            "{kind:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn looking_for_images_counts_against_the_pull_limit() -> TestResult {
    let engine = Engine::new(&[], Pull::Arrives);
    DockerComposeRuntime::new(engine.clone())
        .pull_images(
            &target(Vec::new()),
            Some(&["web".to_owned()]),
            Duration::from_secs(10),
        )
        .await?;
    let limits = engine.limits();
    assert_eq!(limits.len(), 3, "config, inspect, pull");
    assert!(
        limits.iter().all(|limit| *limit <= Duration::from_secs(10)),
        "{limits:?}"
    );

    let engine = Engine::new(&[], Pull::Arrives);
    let error = PodmanComposeRuntime::with_executable(engine.clone(), "podman")
        .pull_images(&target(Vec::new()), None, Duration::ZERO)
        .await
        .err()
        .ok_or("a pull with no time left succeeded")?;
    assert_eq!(error.code(), "MM_TIMEOUT");
    let message = error.to_string();
    for part in [
        "finding which images stack 'app' lacks",
        "(deploy.pull-timeout)",
        "`environments.production.deploy.pull-timeout`",
    ] {
        assert!(message.contains(part), "missing {part:?} in: {message}");
    }
    assert!(engine.records().is_empty());
    Ok(())
}
