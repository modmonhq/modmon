//! `mm service restart` on a cluster node: one container at a time, each
//! healthy before the next, and a running container that another one can
//! stand in for leaves first (its drain, then the restart), so a restart
//! drops no request. The service, its node's keeper of leaving containers
//! and its resolver are scripted.

use std::fs;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    ContainerLeaving, ExecAuditJournal, ExecStream, LogStream, RuntimeDriver,
    RuntimeOperationAuthorizer, RuntimeOperationDriver, SecretMaterialResolver, ServiceNames,
    TargetStore,
};
use mm::app::{
    Application, ApplicationDependencies, DrainedRestart, RuntimeOperationsDependencies,
    RuntimeOperationsService, StackManager, WorkspaceSelection,
};
use mm::domain::discovery::{Health, NameAddress, NameReport, ResolvedName};
use mm::domain::error::MmError;
use mm::domain::observability::{
    ApprovalContext, AuthorizationRequest, ExecAuditRecord, LogRequest, OperationCancellation,
    ResolvedExecRequest, RestartOutcome, RestartRequest, RuntimeOperation, ServiceSelector,
};
use mm::domain::runtime::{
    ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy, ConvergenceState,
    ObservedProject, RenderedProject, RuntimeKind, RuntimeProbe, RuntimeTarget,
};
use mm::domain::secret::{SecretReference, SecretResolution, SecretScope};
use mm::domain::target::{TargetDefinition, TargetId};

type TestResult = Result<(), Box<dyn std::error::Error>>;
/// What happened, in order.
type Journal = Arc<Mutex<Vec<String>>>;

fn note(journal: &Journal, entry: String) {
    journal
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry);
}

fn unexpected(what: &str) -> MmError {
    MmError::Environment {
        message: format!("{what} is not expected in a restart"),
    }
}

/// The engine: the service's containers, and restarts of single ones.
struct Engine {
    containers: Vec<(String, bool)>,
    journal: Journal,
}

#[async_trait]
impl RuntimeOperationDriver for Engine {
    async fn logs(
        &self,
        _target: &RuntimeTarget,
        _request: LogRequest,
        _redactions: Vec<zeroize::Zeroizing<String>>,
        _cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        Err(unexpected("logs"))
    }

    async fn exec(
        &self,
        _target: &RuntimeTarget,
        _request: ResolvedExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        Err(unexpected("exec"))
    }

    async fn restart(
        &self,
        target: &RuntimeTarget,
        selector: &ServiceSelector,
    ) -> Result<RestartOutcome, MmError> {
        let container = selector
            .container
            .clone()
            .ok_or_else(|| unexpected("a restart of every container at once"))?;
        note(&self.journal, format!("restart {container}"));
        Ok(RestartOutcome {
            runtime: RuntimeKind::DockerCompose,
            identity: mm::domain::runtime::ServiceIdentity {
                project_id: target.project_id,
                environment: target.environment.clone(),
                stack: target.stack_name.clone(),
                module: selector.module.clone(),
                service: selector.service.clone(),
            },
            containers: vec![container],
            rolling: false,
            nodes: Vec::new(),
        })
    }

    async fn restartable(
        &self,
        _target: &RuntimeTarget,
        _selector: &ServiceSelector,
    ) -> Result<Vec<(String, bool)>, MmError> {
        Ok(self.containers.clone())
    }
}

/// The node's keeper of leaving containers.
struct Keeper(Journal);

#[async_trait]
impl ContainerLeaving for Keeper {
    async fn leave(&self, _containers: &[String], _drain: Duration) -> Result<(), MmError> {
        Err(unexpected("a leave that removes a container"))
    }

    async fn recall(&self, _project: &str) -> Result<Vec<String>, MmError> {
        Err(unexpected("taking leaves back"))
    }

    async fn restart(&self, containers: &[String], drain: Option<Duration>) -> Result<(), MmError> {
        assert_eq!(drain, None, "the container's own drain applies");
        note(
            &self.0,
            format!("leave and restart {}", containers.join(" ")),
        );
        Ok(())
    }
}

/// The node's resolver: `answering` healthy containers answer for `web`.
struct Names {
    answering: usize,
}

impl ServiceNames for Names {
    fn report(&self, _prefix: &str) -> NameReport {
        NameReport {
            networks: vec!["backend".to_owned()],
            names: vec![ResolvedName {
                name: "web".to_owned(),
                network: "backend".to_owned(),
                addresses: (0..self.answering)
                    .map(|index| NameAddress {
                        address: std::net::Ipv4Addr::from(
                            0x0a40_0002_u32 + u32::try_from(index).unwrap_or(0),
                        ),
                        node: format!("app-{}", index + 1),
                        health: Health::Healthy,
                        unknown_to: Vec::new(),
                        restart_limit: false,
                    })
                    .collect(),
                lookups: Vec::new(),
                fail_open: false,
            }],
            problems: Vec::new(),
        }
    }
}

/// The node's engine as the health wait sees it: always healthy, and each
/// wait is noted.
struct HealthWait(Journal);

#[async_trait]
impl RuntimeDriver for HealthWait {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }

    async fn probe(&self) -> RuntimeProbe {
        RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: true,
            engine_version: None,
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    async fn render(&self, _target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        Err(unexpected("a render"))
    }

    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        Ok(ObservedProject {
            stack: target.stack_name.clone(),
            ..ObservedProject::default()
        })
    }

    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        _policy: ConvergencePolicy,
        _cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        note(&self.0, "healthy".to_owned());
        Ok(ConvergenceOutcome {
            state: ConvergenceState::Succeeded,
            polls: 1,
            observation: self.observe(target).await?,
            service: None,
            diagnostic_code: None,
            message: None,
        })
    }
}

struct NoSecrets;

#[async_trait]
impl SecretMaterialResolver for NoSecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        Err(unexpected("a secret"))
    }
}

struct RestartOnly;

impl RuntimeOperationAuthorizer for RestartOnly {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), MmError> {
        assert_eq!(request.operation, RuntimeOperation::Restart);
        Ok(())
    }
}

struct NoAudit;

impl ExecAuditJournal for NoAudit {
    fn append_exec_audit(&self, _record: &ExecAuditRecord) -> Result<(), MmError> {
        Err(unexpected("an exec audit"))
    }
}

/// A node with `containers` of web (id, running), `answering` of the
/// service's containers answering on the cluster; `scale` is what the
/// service sets next to its stack.
struct Node {
    _project: tempfile::TempDir,
    _home: tempfile::TempDir,
    selection: WorkspaceSelection,
    operations: RuntimeOperationsService,
    journal: Journal,
}

fn node(
    containers: &[(&str, bool)],
    answering: usize,
    scale: &str,
) -> Result<Node, Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    let home = crate::temp_dirs::tempdir()?;
    fs::create_dir(project.path().join(".config"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        format!(
            "schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000077
  name: drained-restart
default-environment: production
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  shop:
    path: .
    services:
      web:
        stack: app
{scale}environments:
  production: {{}}
"
        ),
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    let documents = Arc::new(ConfigLoader);
    let targets = Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::from_u128(7))),
    )?;
    let targets: Arc<dyn TargetStore> = targets;
    let stacks = StackManager::new(
        documents.clone(),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        targets,
    );
    let journal = Journal::default();
    let clock = Arc::new(FixedClock { unix_ms: 1_000 });
    let application = Application::new(ApplicationDependencies {
        documents,
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime: Arc::new(HealthWait(journal.clone())),
        state: Arc::new(SqliteStateStore::open(&home.path().join("state.db"))?),
        stacks,
        clock: clock.clone(),
    });
    let operations = RuntimeOperationsService::new(RuntimeOperationsDependencies {
        application,
        driver: Arc::new(Engine {
            containers: containers
                .iter()
                .map(|(id, running)| ((*id).to_owned(), *running))
                .collect(),
            journal: journal.clone(),
        }),
        secrets: Arc::new(NoSecrets),
        authorizer: Arc::new(RestartOnly),
        audit: Arc::new(NoAudit),
        clock,
    })
    .with_drained_restart(DrainedRestart {
        leaving: Arc::new(Keeper(journal.clone())),
        names: Arc::new(Names { answering }),
    });
    let selection = WorkspaceSelection::new(project.path(), Some("production".to_owned()));
    Ok(Node {
        _project: project,
        _home: home,
        selection,
        operations,
        journal,
    })
}

fn restart_web() -> RestartRequest {
    RestartRequest {
        selector: ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        },
        approval: ApprovalContext {
            principal: "carol".to_owned(),
            source: "test".to_owned(),
            approved_high_risk: false,
        },
    }
}

const AAA: &str = "aaaaaaaaaaaa";
const BBB: &str = "bbbbbbbbbbbb";

fn entries(node: &Node) -> Vec<String> {
    node.journal
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[tokio::test]
async fn each_container_leaves_and_restarts_while_another_answers() -> TestResult {
    // Two containers here, three answering over the cluster: each in turn
    // drains and restarts, healthy before the next.
    let node = node(&[(AAA, true), (BBB, true)], 3, "")?;
    let outcome = node
        .operations
        .restart(&node.selection, restart_web())
        .await?;
    assert_eq!(outcome.containers, [AAA, BBB]);
    assert_eq!(
        entries(&node),
        [
            format!("leave and restart {AAA}"),
            "healthy".to_owned(),
            format!("leave and restart {BBB}"),
            "healthy".to_owned(),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn the_only_container_and_a_stopped_one_restart_at_once() -> TestResult {
    // The service's only answering container: a drain would only make the
    // gap longer.
    let alone = node(&[(AAA, true)], 1, "")?;
    alone
        .operations
        .restart(&alone.selection, restart_web())
        .await?;
    assert_eq!(
        entries(&alone),
        [format!("restart {AAA}"), "healthy".to_owned()]
    );
    // A container that does not run serves nothing to drain.
    let stopped = node(&[(AAA, false), (BBB, true)], 2, "")?;
    stopped
        .operations
        .restart(&stopped.selection, restart_web())
        .await?;
    assert_eq!(
        entries(&stopped),
        [
            format!("restart {AAA}"),
            "healthy".to_owned(),
            format!("leave and restart {BBB}"),
            "healthy".to_owned(),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_node_that_runs_none_of_a_scaled_service_restarts_nothing() -> TestResult {
    let scaled = node(&[], 2, "        replicas: 1\n")?;
    let outcome = scaled
        .operations
        .restart(&scaled.selection, restart_web())
        .await?;
    assert!(outcome.containers.is_empty());
    assert!(entries(&scaled).is_empty(), "{:?}", entries(&scaled));
    // A service with one container on each of its nodes misses it here.
    let missing = node(&[], 2, "")?;
    let refused = missing
        .operations
        .restart(&missing.selection, restart_web())
        .await;
    assert!(
        matches!(&refused, Err(MmError::RuntimeResponse { message }) if message.contains("start the stack first")),
        "{refused:?}"
    );
    Ok(())
}
