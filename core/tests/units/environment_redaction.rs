//! Which secret values a job's output and a service's logs are redacted
//! against: everything the environment hands out, read from the stores the
//! environment uses. Here `local` keeps its secrets in age and `staging` in
//! an OpenBao this machine has no connection to.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use reqwest::Method;
use uuid::Uuid;
use zeroize::Zeroizing;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::secrets::{
    OpenBaoConnection, OpenBaoConnectionStore, OpenBaoHttpResponse, OpenBaoHttpTransport,
    OpenBaoMode, OpenBaoSecretStore, ca_sha256,
};
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    DeploymentJobs, ExecAuditJournal, ExecStream, LogStream, RuntimeJobDriver,
    RuntimeOperationAuthorizer, RuntimeOperationDriver, SecretMaterialResolver, TargetStore,
};
use mm::app::{
    Application, JobSchedulerDependencies, JobSchedulerService, RuntimeOperationsDependencies,
    RuntimeOperationsService, StackManager, WorkspaceSelection,
};
use mm::domain::error::MmError;
use mm::domain::observability::{
    ApprovalContext, AuthorizationRequest, ExecAuditRecord, LogLimits, LogMode, LogOutputPolicy,
    LogRecord, LogRequest, LogSource, OperationCancellation, ResolvedExecRequest, ServiceSelector,
    redact_output,
};
use mm::domain::runtime::{
    RuntimeCapability, RuntimeEnvironment, RuntimeKind, RuntimeProbe, RuntimeTarget,
    ServiceIdentity,
};
use mm::domain::scheduler::{JobLaunch, JobRunState, RuntimeJobState};
use mm::domain::secret::{
    SecretMaterial, SecretMetadata, SecretReference, SecretResolution, SecretScope, SecretStatus,
};
use mm::domain::secret_backend::ProviderInstanceName;
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const PROJECT_ID: Uuid = Uuid::from_u128(0x16);
const CA_PEM: &str = "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n";
const DATABASE_PASSWORD: &str = "local-database-password-5f1c";
const API_TOKEN: &str = "local-api-token-8d2e";
const REGISTRY_TOKEN: &str = "local-registry-token-3b7a";
const PREVIOUS_TOKEN: &str = "previous-api-token-a4c9";
const LOCAL_VALUES: [&str; 4] = [DATABASE_PASSWORD, API_TOKEN, REGISTRY_TOKEN, PREVIOUS_TOKEN];

const CONFIG: &str = r#"schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000016
  name: redaction
default-environment: local
target: local
runtime:
  driver: docker-compose
providers:
  team-store:
    driver: openbao
    scope: project
    mode: external
    address: https://bao.example.test:8200
    storage-mount: mm-kv
    automation-auth-mount: mm-approle
    trust-bundle: bao-ca.pem
    expected-trust-sha256: TRUST_SHA256
registries:
  registry.example.test:
    username: ci
    password: {secret: registry.token}
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: api
    env:
      template: .env.example
    services:
      web:
        stack: app
        secret-files:
          api-token: {secret: api.token}
jobs:
  migrate:
    module: api
    service: web
    run: before-deploy
    command: [migrate]
secrets:
  database.password:
    storage: {provider: age, location: local/database-password}
  database.password.staging:
    storage: {provider: team-store, location: database/password}
  api.token:
    storage: {provider: age, location: local/api-token}
  api.previous-token:
    storage: {provider: age, location: local/previous-api-token}
  registry.token:
    storage: {provider: age, location: local/registry-token}
environments:
  local:
    bindings:
      DB.PASSWORD: {secret: database.password}
  staging:
    bindings:
      DB.PASSWORD: {secret: database.password.staging}
"#;

fn poisoned<T>(error: PoisonError<T>) -> MmError {
    MmError::State {
        message: format!("test lock was poisoned: {error}"),
    }
}

/// The OpenBao behind `staging`: every request it would get is noted.
#[derive(Default)]
struct Bao {
    requests: Mutex<Vec<String>>,
}

impl Bao {
    fn note(&self, path: &str) -> Result<OpenBaoHttpResponse, MmError> {
        self.requests
            .lock()
            .map_err(poisoned)?
            .push(path.to_owned());
        Err(MmError::State {
            message: format!("the store was asked for {path}"),
        })
    }

    fn requests(&self) -> Result<Vec<String>, MmError> {
        Ok(self.requests.lock().map_err(poisoned)?.clone())
    }
}

#[async_trait]
impl OpenBaoHttpTransport for Bao {
    async fn request(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        self.note(path)
    }

    async fn request_bytes(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        self.note(path)
    }
}

/// This machine's secrets: an age store holding `local`'s, and the real
/// OpenBao adapter with no connection stored for `staging`'s.
struct Stores {
    age: BTreeMap<(String, String), String>,
    team: OpenBaoSecretStore,
    asked: Mutex<Vec<String>>,
}

#[async_trait]
impl SecretMaterialResolver for Stores {
    async fn resolve(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        self.asked
            .lock()
            .map_err(poisoned)?
            .push(reference.canonical());
        if reference.provider().as_str() == "team-store" {
            return self.team.resolve(scope, reference).await;
        }
        let key = (
            scope.environment().to_owned(),
            reference.locator().to_owned(),
        );
        let material = self
            .age
            .get(&key)
            .map(|value| SecretMaterial::new(value.clone()))
            .transpose()
            .map_err(|message| MmError::Secret { message })?;
        Ok(SecretResolution {
            metadata: SecretMetadata {
                status: if material.is_some() {
                    SecretStatus::Set
                } else {
                    SecretStatus::Unset
                },
                revision: material.as_ref().map(|_| "1".to_owned()),
                created_at_unix_ms: None,
                candidate_revision: None,
            },
            material,
        })
    }
}

/// The engine: what each job and log read was handed to redact, and the
/// output it gives back redacted with it, as the real drivers do.
#[derive(Default)]
struct Engine {
    handed: Mutex<Vec<Vec<String>>>,
    launched: Mutex<usize>,
}

impl Engine {
    fn hand(&self, redactions: &[Zeroizing<String>]) -> Result<(), MmError> {
        self.handed.lock().map_err(poisoned)?.push(
            redactions
                .iter()
                .map(|value| value.as_str().to_owned())
                .collect(),
        );
        Ok(())
    }

    fn handed(&self) -> Result<Vec<Vec<String>>, MmError> {
        Ok(self.handed.lock().map_err(poisoned)?.clone())
    }

    fn launched(&self) -> Result<usize, MmError> {
        Ok(*self.launched.lock().map_err(poisoned)?)
    }
}

fn printed() -> String {
    format!(
        "connected with {DATABASE_PASSWORD}, token {API_TOKEN}, pulled with {REGISTRY_TOKEN}, old {PREVIOUS_TOKEN}\n"
    )
}

#[async_trait]
impl RuntimeJobDriver for Engine {
    async fn launch(&self, launch: JobLaunch) -> Result<String, MmError> {
        self.hand(&launch.redactions)?;
        *self.launched.lock().map_err(poisoned)? += 1;
        Ok("job-container".to_owned())
    }

    async fn inspect(
        &self,
        _target: &RuntimeTarget,
        _runtime_name: &str,
        runtime_id: Option<&str>,
        _output_limit_bytes: usize,
        redactions: Vec<Zeroizing<String>>,
    ) -> Result<RuntimeJobState, MmError> {
        self.hand(&redactions)?;
        Ok(match runtime_id {
            None => RuntimeJobState::Missing,
            Some(_) => RuntimeJobState::Exited {
                exit_code: 0,
                output: redact_output(&printed(), &redactions),
                truncated: false,
            },
        })
    }

    async fn cancel(
        &self,
        _target: &RuntimeTarget,
        _runtime_name: &str,
        _runtime_id: Option<&str>,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn cleanup(
        &self,
        _target: &RuntimeTarget,
        _runtime_name: &str,
        _runtime_id: Option<&str>,
    ) -> Result<(), MmError> {
        Ok(())
    }
}

#[async_trait]
impl RuntimeOperationDriver for Engine {
    async fn logs(
        &self,
        target: &RuntimeTarget,
        _request: LogRequest,
        redactions: Vec<Zeroizing<String>>,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        self.hand(&redactions)?;
        let record = LogRecord {
            identity: ServiceIdentity {
                project_id: target.project_id,
                environment: target.environment.clone(),
                stack: target.stack_name.clone(),
                module: Some("api".to_owned()),
                service: "web".to_owned(),
            },
            sequence: 1,
            source: LogSource::Stdout,
            node: None,
            container: None,
            timestamp: None,
            message: redact_output(&printed(), &redactions),
            lossy_utf8: false,
            truncated: false,
            reconnect: 0,
        };
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        sender.send(Ok(record)).await.map_err(|_| MmError::State {
            message: "the log reader went away".to_owned(),
        })?;
        Ok(LogStream::new(receiver, cancellation))
    }

    async fn exec(
        &self,
        _target: &RuntimeTarget,
        _request: ResolvedExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        Err(MmError::State {
            message: "exec is not part of this test".to_owned(),
        })
    }
}

struct Allow;

impl RuntimeOperationAuthorizer for Allow {
    fn authorize(&self, _request: &AuthorizationRequest) -> Result<(), MmError> {
        Ok(())
    }
}

struct NoAudit;

impl ExecAuditJournal for NoAudit {
    fn append_exec_audit(&self, _record: &ExecAuditRecord) -> Result<(), MmError> {
        Ok(())
    }
}

struct Setup {
    _project: tempfile::TempDir,
    _home: tempfile::TempDir,
    root: std::path::PathBuf,
    jobs: JobSchedulerService,
    operations: RuntimeOperationsService,
    engine: Arc<Engine>,
    bao: Arc<Bao>,
    stores: Arc<Stores>,
}

impl Setup {
    fn selection(&self, environment: &str) -> WorkspaceSelection {
        WorkspaceSelection::new(&self.root, Some(environment.to_owned()))
    }

    fn asked_team_store(&self) -> Result<bool, MmError> {
        Ok(self
            .stores
            .asked
            .lock()
            .map_err(poisoned)?
            .iter()
            .any(|reference| reference.starts_with("team-store/")))
    }
}

fn write_project(root: &Path) -> TestResult {
    fs::create_dir_all(root.join(".config"))?;
    fs::create_dir_all(root.join("api"))?;
    fs::write(
        root.join(".config/mm.yaml"),
        CONFIG.replace("TRUST_SHA256", &ca_sha256(CA_PEM)),
    )?;
    fs::write(root.join("bao-ca.pem"), CA_PEM)?;
    fs::write(
        root.join("compose.yaml"),
        "services:\n  web:\n    image: example/web\n",
    )?;
    fs::write(
        root.join("api/.env.example"),
        "DB_PASSWORD= # mm:from=DB.PASSWORD required\n",
    )?;
    Ok(())
}

fn setup() -> Result<Setup, Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    let home = crate::temp_dirs::tempdir()?;
    write_project(project.path())?;
    let documents = Arc::new(ConfigLoader);
    let targets: Arc<dyn TargetStore> =
        Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::from_u128(0x17))),
    )?;
    let stacks = StackManager::new(
        documents.clone(),
        Arc::new(FilesystemStackArtifactStore::new(
            home.path().join("compose"),
        )),
        targets,
    );
    let runtime = Arc::new(FakeRuntime {
        probe: RuntimeProbe {
            kind: RuntimeKind::DockerCompose,
            available: true,
            engine_version: Some("test".to_owned()),
            compose_provider: None,
            rootless: Some(true),
            socket_path: None,
            capabilities: vec![
                RuntimeCapability::RenderCompose,
                RuntimeCapability::OneShotJobs,
                RuntimeCapability::StreamLogs,
            ],
            diagnostics: Vec::new(),
        },
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let clock = Arc::new(FixedClock {
        unix_ms: 1_800_000_000_000,
    });
    let application = Application::runtime_inspection(
        runtime,
        clock.clone(),
        documents,
        Arc::new(EnvironmentIndexer),
        stacks,
    );
    let bao = Arc::new(Bao::default());
    let team = OpenBaoSecretStore::new(
        ProviderInstanceName::new("team-store")?,
        OpenBaoMode::External,
        ProjectId(PROJECT_ID),
        Arc::new(OpenBaoConnectionStore::new(
            home.path().join("openbao-connections.key"),
            home.path().join("openbao-connections.age"),
        )),
        bao.clone(),
    );
    let local =
        |location: &str, value: &str| (("local".to_owned(), location.to_owned()), value.to_owned());
    let stores = Arc::new(Stores {
        age: BTreeMap::from([
            local("local/database-password", DATABASE_PASSWORD),
            local("local/api-token", API_TOKEN),
            local("local/registry-token", REGISTRY_TOKEN),
            local("local/previous-api-token", PREVIOUS_TOKEN),
        ]),
        team,
        asked: Mutex::new(Vec::new()),
    });
    let engine = Arc::new(Engine::default());
    let jobs = JobSchedulerService::new(JobSchedulerDependencies {
        application: application.clone(),
        repository: Arc::new(SqliteStateStore::open(&home.path().join("state.db"))?),
        runtime: engine.clone(),
        secrets: stores.clone(),
        authorizer: Arc::new(Allow),
        clock: clock.clone(),
    });
    let operations = RuntimeOperationsService::new(RuntimeOperationsDependencies {
        application,
        driver: engine.clone(),
        secrets: stores.clone(),
        authorizer: Arc::new(Allow),
        audit: Arc::new(NoAudit),
        clock,
    });
    Ok(Setup {
        root: project.path().to_path_buf(),
        _project: project,
        _home: home,
        jobs,
        operations,
        engine,
        bao,
        stores,
    })
}

fn job_target(root: &Path, environment: &str) -> RuntimeTarget {
    RuntimeTarget {
        root: root.to_path_buf(),
        project_directory: root.to_path_buf(),
        target: None,
        project_id: ProjectId(PROJECT_ID),
        environment: environment.to_owned(),
        stack_name: "app".to_owned(),
        project_name: "redaction".to_owned(),
        artifact_hash: "a".repeat(64),
        compose_files: vec![root.join("compose.yaml")],
        user_compose_file_count: 1,
        interpolation_environment: RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

fn assert_every_local_secret(handed: &[String]) {
    for value in LOCAL_VALUES {
        assert!(
            handed.iter().any(|handed| handed == value),
            "{value} is not redacted"
        );
    }
}

#[tokio::test]
async fn a_local_deployment_job_runs_redacted_without_the_store_of_another_environment()
-> TestResult {
    let setup = setup()?;
    let run = setup
        .jobs
        .run(
            &setup.selection("local"),
            "migrate",
            job_target(&setup.root, "local"),
            &"b".repeat(64),
        )
        .await?;
    assert_eq!(run.state, JobRunState::Succeeded, "{run:?}");
    assert_eq!(setup.engine.launched()?, 1);
    let handed = setup.engine.handed()?;
    assert!(handed.len() >= 3, "launch and both looks get redactions");
    for redactions in &handed {
        assert_every_local_secret(redactions);
    }
    for value in LOCAL_VALUES {
        assert!(!run.output.contains(value), "{}", run.output);
    }
    assert_eq!(
        run.output.matches("[REDACTED]").count(),
        4,
        "{}",
        run.output
    );
    assert!(!setup.asked_team_store()?, "staging's store is not asked");
    assert_eq!(setup.bao.requests()?, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn a_staging_job_is_refused_when_a_secret_it_receives_cannot_be_read_here() -> TestResult {
    let setup = setup()?;
    let refused = setup
        .jobs
        .run(
            &setup.selection("staging"),
            "migrate",
            job_target(&setup.root, "staging"),
            &"c".repeat(64),
        )
        .await
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        refused.contains("no connection to secret provider 'team-store' for environment 'staging'"),
        "{refused}"
    );
    assert_eq!(setup.engine.launched()?, 0, "nothing runs unredacted");
    assert_eq!(setup.bao.requests()?, Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn local_service_logs_are_redacted_without_the_store_of_another_environment() -> TestResult {
    let setup = setup()?;
    let request = LogRequest {
        selector: Some(ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        }),
        nodes: Vec::new(),
        tail: 10,
        since_unix_ms: None,
        until_unix_ms: None,
        follow: false,
        timestamps: false,
        output: LogOutputPolicy::Both,
        mode: LogMode::Redacted,
        limits: LogLimits::default(),
        approval: ApprovalContext {
            principal: "operator".to_owned(),
            source: "test".to_owned(),
            approved_high_risk: false,
        },
    };
    let mut stream = setup
        .operations
        .logs(
            &setup.selection("local"),
            request,
            OperationCancellation::default(),
        )
        .await?;
    let line = stream.recv().await.ok_or("no log line")??;
    for value in LOCAL_VALUES {
        assert!(!line.message.contains(value), "{}", line.message);
    }
    assert_eq!(
        line.message.matches("[REDACTED]").count(),
        4,
        "{}",
        line.message
    );
    let handed = setup.engine.handed()?;
    assert_eq!(handed.len(), 1);
    assert_every_local_secret(&handed[0]);
    assert!(!setup.asked_team_store()?, "staging's store is not asked");
    assert_eq!(setup.bao.requests()?, Vec::<String>::new());
    Ok(())
}
