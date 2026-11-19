use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;

use super::*;
use crate::adapters::clock::FixedClock;
use crate::adapters::config::ConfigLoader;
use crate::adapters::environment::EnvironmentIndexer;
use crate::adapters::runtime::FakeRuntime;
use crate::adapters::stack::FilesystemStackArtifactStore;
use crate::adapters::target::FilesystemTargetStore;
use crate::app::ports::{LogStream, StackArtifactStore, TargetStore};
use crate::app::{StackManager, WorkspaceSelection};
use crate::domain::config::{ConfigDocument, MmConfig};
use crate::domain::observability::{
    ApprovalContext, ExecEnvironmentReference, ExecOutcome, ExecOutputSource, ExecStdinMode,
    ExecTtyMode, LogLimits, LogMode, LogOutputPolicy, LogRecord, LogRequest, LogSource,
    ServiceSelector,
};
use crate::domain::runtime::{
    RuntimeEnvironment, RuntimeKind, RuntimeProbe, RuntimeTarget, ServiceIdentity,
};
use crate::domain::secret::{
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretScope, SecretStatus,
};
use crate::domain::target::{TargetDefinition, TargetId};
use crate::domain::workspace::ProjectId;

const NOW: i64 = 1_900_000_000_000;
const SENTINEL: &str = "observability-secret-sentinel";

fn state(message: impl Into<String>) -> MmError {
    MmError::State {
        message: message.into(),
    }
}

fn lock<T>(value: &Mutex<T>) -> Result<MutexGuard<'_, T>, MmError> {
    value.lock().map_err(|_| state("test lock was poisoned"))
}

#[derive(Debug)]
struct LogCall {
    target: String,
    request: LogRequest,
    redactions: Vec<String>,
}

#[derive(Debug)]
struct ExecCall {
    target: String,
    request: ExecRequest,
    environment_debug: String,
}

#[derive(Default)]
struct FakeOperations {
    logs: Mutex<Vec<LogCall>>,
    execs: Mutex<Vec<ExecCall>>,
    log_events: Mutex<VecDeque<Vec<Result<LogRecord, MmError>>>>,
    exec_events: Mutex<VecDeque<Vec<Result<ExecEvent, MmError>>>>,
    log_error: Mutex<Option<MmError>>,
    exec_error: Mutex<Option<MmError>>,
    exec_gate: Mutex<Option<Arc<tokio::sync::Notify>>>,
}

impl FakeOperations {
    fn queue_log(&self, events: Vec<Result<LogRecord, MmError>>) -> Result<(), MmError> {
        lock(&self.log_events)?.push_back(events);
        Ok(())
    }

    fn queue_exec(&self, events: Vec<Result<ExecEvent, MmError>>) -> Result<(), MmError> {
        lock(&self.exec_events)?.push_back(events);
        Ok(())
    }

    fn gate_exec(&self) -> Result<Arc<tokio::sync::Notify>, MmError> {
        let gate = Arc::new(tokio::sync::Notify::new());
        *lock(&self.exec_gate)? = Some(gate.clone());
        Ok(gate)
    }

    fn log_calls(&self) -> Result<Vec<LogCall>, MmError> {
        Ok(lock(&self.logs)?.drain(..).collect())
    }

    fn exec_calls(&self) -> Result<Vec<ExecCall>, MmError> {
        Ok(lock(&self.execs)?.drain(..).collect())
    }
}

#[async_trait]
impl RuntimeOperationDriver for FakeOperations {
    async fn logs(
        &self,
        target: &RuntimeTarget,
        request: LogRequest,
        redactions: Vec<zeroize::Zeroizing<String>>,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        lock(&self.logs)?.push(LogCall {
            target: target.stack_name.clone(),
            request,
            redactions: redactions
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
        });
        if let Some(error) = lock(&self.log_error)?.take() {
            return Err(error);
        }
        let events = lock(&self.log_events)?.pop_front().unwrap_or_default();
        let (sender, receiver) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            for event in events {
                if sender.send(event).await.is_err() {
                    break;
                }
            }
        });
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        target: &RuntimeTarget,
        request: ResolvedExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        let environment_debug = format!("{:?}", request.environment);
        lock(&self.execs)?.push(ExecCall {
            target: target.stack_name.clone(),
            request: request.request,
            environment_debug,
        });
        if let Some(error) = lock(&self.exec_error)?.take() {
            return Err(error);
        }
        let events = lock(&self.exec_events)?.pop_front().unwrap_or_default();
        let gate = lock(&self.exec_gate)?.take();
        let (sender, receiver) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            if let Some(gate) = gate {
                gate.notified().await;
            }
            for event in events {
                if sender.send(event).await.is_err() {
                    break;
                }
            }
        });
        Ok(ExecStream::new(receiver, cancellation))
    }
}

#[derive(Default)]
struct FakeSecrets {
    resolutions: Mutex<VecDeque<Result<SecretResolution, MmError>>>,
    calls: AtomicUsize,
}

#[async_trait]
impl SecretMaterialResolver for FakeSecrets {
    async fn resolve(
        &self,
        scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        assert_eq!(scope.environment(), "local");
        self.calls.fetch_add(1, Ordering::SeqCst);
        lock(&self.resolutions)?
            .pop_front()
            .ok_or_else(|| state("test secret resolution is absent"))?
    }
}

#[derive(Default)]
struct FakeAuthorizer {
    denied: AtomicBool,
    requests: Mutex<Vec<AuthorizationRequest>>,
}

impl RuntimeOperationAuthorizer for FakeAuthorizer {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), MmError> {
        lock(&self.requests)?.push(request.clone());
        if self.denied.load(Ordering::SeqCst) {
            Err(MmError::Security {
                message: "injected authorization denial".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct FakeAudit {
    records: Mutex<Vec<ExecAuditRecord>>,
    calls: AtomicUsize,
    fail_at: AtomicUsize,
}

impl ExecAuditJournal for FakeAudit {
    fn append_exec_audit(&self, record: &ExecAuditRecord) -> Result<(), MmError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at.load(Ordering::SeqCst) == call {
            return Err(state(format!("injected audit failure at call {call}")));
        }
        lock(&self.records)?.push(record.clone());
        Ok(())
    }
}

struct Setup {
    _project: TempDir,
    _home: TempDir,
    service: RuntimeOperationsService,
    operations: Arc<FakeOperations>,
    secrets: Arc<FakeSecrets>,
    authorizer: Arc<FakeAuthorizer>,
    audit: Arc<FakeAudit>,
    selection: WorkspaceSelection,
}

fn setup(capabilities: Vec<RuntimeCapability>) -> Result<Setup, Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
    let home = crate::test_support::tempdir()?;
    fs::create_dir(project.path().join(".config"))?;
    fs::create_dir(project.path().join("api"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: observability-service
default-environment: local
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    services:
      web:
        stack: app
        secret-files:
          token:
            secret: token
secrets:
  token:
    storage:
      provider: env
      location: TOKEN
environments:
  local:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    let documents = Arc::new(ConfigLoader);
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::from_u128(2))),
    )?;
    let artifacts: Arc<dyn StackArtifactStore> = Arc::new(FilesystemStackArtifactStore::new(
        home.path().join("compose"),
    ));
    let stacks = StackManager::new(documents.clone(), artifacts, targets);
    let runtime = Arc::new(FakeRuntime {
        probe: RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: None,
            rootless: Some(true),
            socket_path: None,
            capabilities,
            diagnostics: Vec::new(),
        },
        user_rendered: None,
        rendered: Default::default(),
        observed: Default::default(),
    });
    let clock = Arc::new(FixedClock { unix_ms: NOW });
    let application = Application::runtime_inspection(
        runtime,
        clock.clone(),
        documents,
        Arc::new(EnvironmentIndexer),
        stacks,
    );
    let operations = Arc::new(FakeOperations::default());
    let secrets = Arc::new(FakeSecrets::default());
    let authorizer = Arc::new(FakeAuthorizer::default());
    let audit = Arc::new(FakeAudit::default());
    let service = RuntimeOperationsService::new(RuntimeOperationsDependencies {
        application,
        driver: operations.clone(),
        secrets: secrets.clone(),
        authorizer: authorizer.clone(),
        audit: audit.clone(),
        clock,
    });
    let selection = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    Ok(Setup {
        _project: project,
        _home: home,
        service,
        operations,
        secrets,
        authorizer,
        audit,
        selection,
    })
}

fn secret(status: SecretStatus, material: Option<&str>) -> Result<SecretResolution, String> {
    Ok(SecretResolution {
        metadata: SecretMetadata {
            status,
            revision: Some("revision-1".to_owned()),
            created_at_unix_ms: Some(NOW),
            candidate_revision: None,
        },
        material: material
            .map(|value| SecretMaterial::new(value.to_owned()))
            .transpose()?,
    })
}

fn selector() -> ServiceSelector {
    ServiceSelector {
        stack: Some("app".to_owned()),
        module: None,
        service: "web".to_owned(),
        container: None,
    }
}

fn approval(high_risk: bool) -> ApprovalContext {
    ApprovalContext {
        principal: "operator".to_owned(),
        source: "unit-test".to_owned(),
        approved_high_risk: high_risk,
    }
}

fn log_request(mode: LogMode) -> LogRequest {
    LogRequest {
        selector: Some(selector()),
        nodes: Vec::new(),
        tail: 10,
        since_unix_ms: None,
        until_unix_ms: None,
        follow: false,
        timestamps: true,
        output: LogOutputPolicy::Both,
        mode,
        limits: LogLimits::default(),
        approval: approval(mode == LogMode::Raw),
    }
}

fn exec_request() -> Result<ExecRequest, String> {
    Ok(ExecRequest {
        selector: selector(),
        argv: vec!["printf".to_owned(), "safe".to_owned()],
        user: None,
        workdir: None,
        environment: vec![ExecEnvironmentReference {
            name: "TOKEN".to_owned(),
            reference: "env://TOKEN".parse()?,
        }],
        tty: ExecTtyMode::Disabled,
        stdin: ExecStdinMode::Closed,
        timeout: Duration::from_secs(30),
        approval: approval(false),
        shell_wrapper: false,
    })
}

fn identity() -> ServiceIdentity {
    ServiceIdentity {
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        stack: "app".to_owned(),
        module: Some("api".to_owned()),
        service: "web".to_owned(),
    }
}

fn log_record() -> LogRecord {
    LogRecord {
        identity: identity(),
        sequence: 1,
        source: LogSource::Stdout,
        node: None,
        container: None,
        timestamp: None,
        message: "public".to_owned(),
        lossy_utf8: false,
        truncated: false,
        reconnect: 0,
    }
}

fn finished() -> ExecEvent {
    ExecEvent::Finished(ExecOutcome {
        runtime: RuntimeKind::DockerCompose,
        identity: identity(),
        container_id: "container-1".to_owned(),
        exit_code: 7,
        started_at_unix_ms: NOW - 10,
        finished_at_unix_ms: NOW + 10,
    })
}

fn document(two_modules: bool) -> Result<ConfigDocument, String> {
    let second = if two_modules {
        "  worker:\n    path: worker\n    services:\n      web:\n        stack: app\n"
    } else {
        ""
    };
    let source = format!(
        "schema-version: 1\nproject:\n  id: 00000000-0000-0000-0000-000000000001\n  name: observability\ntarget: local\nstacks: {{}}\nmodules:\n  api:\n    path: api\n    services:\n      web:\n        stack: app\n{second}environments:\n  local:\n    bindings: {{}}\n"
    );
    let config = serde_yaml_ng::from_str::<MmConfig>(&source).map_err(|error| error.to_string())?;
    Ok(ConfigDocument {
        config,
        root: PathBuf::from("/workspace"),
        path: PathBuf::from("/workspace/mm.yaml"),
        sources: Default::default(),
        source_hash: "a".repeat(64),
    })
}

fn target(stack: &str) -> RuntimeTarget {
    RuntimeTarget {
        root: PathBuf::from("/workspace"),
        project_directory: PathBuf::from("/workspace"),
        target: None,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        stack_name: stack.to_owned(),
        project_name: "observability".to_owned(),
        artifact_hash: "b".repeat(64),
        compose_files: Vec::new(),
        user_compose_file_count: 0,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

fn authorization(operation: RuntimeOperation) -> AuthorizationRequest {
    AuthorizationRequest {
        principal: "operator".to_owned(),
        source: "cli".to_owned(),
        operation,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        selector: None,
        approved_high_risk: false,
    }
}

#[test]
fn runtime_service_selection_requires_one_exact_stack_and_module_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let single = document(false)?;
    let app = target("app");
    assert_eq!(
        select_target(&single, std::slice::from_ref(&app), &selector())?.stack_name,
        "app"
    );
    assert_eq!(select_module(&single, &selector(), "app")?, "api");
    let mut explicit = selector();
    explicit.module = Some("api".to_owned());
    assert_eq!(select_module(&single, &explicit, "app")?, "api");
    assert!(select_target(&single, &[target("other")], &selector()).is_err());
    assert!(select_target(&single, &[target("app"), target("app")], &selector()).is_err());
    let mut missing = selector();
    missing.service = "missing".to_owned();
    assert!(select_module(&single, &missing, "app").is_err());
    let ambiguous = document(true)?;
    assert!(select_module(&ambiguous, &selector(), "app").is_err());
    explicit.module = Some("worker".to_owned());
    assert_eq!(select_module(&ambiguous, &explicit, "app")?, "worker");
    Ok(())
}

#[test]
fn local_authorization_requires_identity_and_explicit_high_risk_approval() {
    let authorizer = LocalRuntimeOperationAuthorizer;
    assert!(
        authorizer
            .authorize(&authorization(RuntimeOperation::Logs))
            .is_ok()
    );
    for operation in [RuntimeOperation::RawLogs, RuntimeOperation::Shell] {
        let mut request = authorization(operation);
        assert!(authorizer.authorize(&request).is_err());
        request.approved_high_risk = true;
        assert!(authorizer.authorize(&request).is_ok());
    }
    let mut invalid = authorization(RuntimeOperation::Status);
    for (principal, source) in [
        ("  ", "cli"),
        ("operator", ""),
        ("operator", &"x".repeat(256)),
        ("operator", "terminal\nspoof"),
    ] {
        invalid.principal = principal.to_owned();
        invalid.source = source.to_owned();
        assert!(authorizer.authorize(&invalid).is_err());
    }
}

mod service_contract;
