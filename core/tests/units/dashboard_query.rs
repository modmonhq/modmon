//! The dashboard's query service over real stores and a scripted runtime:
//! every scope is bounded, lists only registered projects, and never carries
//! a secret.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    DashboardHostSource, ExecAuditJournal, ExecStream, InstallationStatusQuery, LogStream,
    ProjectDocumentStore, RuntimeOperationAuthorizer, RuntimeOperationDriver,
    SecretMaterialResolver, StateStore, TargetStore,
};
use mm::app::{
    Application, ApplicationDependencies, DashboardQueryService, RegisteredDashboardHostSource,
    RuntimeOperationsDependencies, RuntimeOperationsService, StackManager,
};
use mm::domain::dashboard::{
    DashboardScope, DashboardScopeDto, MAX_DASHBOARD_DIAGNOSTICS, MAX_DASHBOARD_PROJECTS,
};
use mm::domain::diagnostic::Diagnostic;
use mm::domain::local_ingress::project_dashboard_slug;
use mm::domain::observability::{
    AuthorizationRequest, ExecAuditRecord, LogRequest, OperationCancellation, ResolvedExecRequest,
};
use mm::domain::runtime::{ObservedProject, RuntimeKind, RuntimeProbe, RuntimeTarget};
use mm::domain::secret::{SecretReference, SecretResolution, SecretScope};
use mm::domain::service_stack::{ServiceComponentKind, ServiceComponentStatus, ServiceStackStatus};
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::workspace::{ProjectId, WorkspaceRegistration};

const NOW: i64 = 1_900_000_000_000;
const SENTINEL: &str = "mm-dashboard-secret-sentinel-9f3c2a1b";

struct NoRuntimeOperations;

#[async_trait]
impl RuntimeOperationDriver for NoRuntimeOperations {
    async fn logs(
        &self,
        _target: &RuntimeTarget,
        _request: LogRequest,
        _redactions: Vec<zeroize::Zeroizing<String>>,
        _cancellation: OperationCancellation,
    ) -> Result<LogStream, mm::domain::error::MmError> {
        Err(mm::domain::error::MmError::Environment {
            message: "logs are not expected in a dashboard query".to_owned(),
        })
    }

    async fn exec(
        &self,
        _target: &RuntimeTarget,
        _request: ResolvedExecRequest,
        _cancellation: OperationCancellation,
    ) -> Result<ExecStream, mm::domain::error::MmError> {
        Err(mm::domain::error::MmError::Environment {
            message: "exec is not expected in a dashboard query".to_owned(),
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
    ) -> Result<SecretResolution, mm::domain::error::MmError> {
        Err(mm::domain::error::MmError::Secret {
            message: "secret resolution is not expected in a dashboard query".to_owned(),
        })
    }
}

struct AllowStatus;

impl RuntimeOperationAuthorizer for AllowStatus {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), mm::domain::error::MmError> {
        assert_eq!(
            request.operation,
            mm::domain::observability::RuntimeOperation::Status
        );
        Ok(())
    }
}

struct NoAudit;

impl ExecAuditJournal for NoAudit {
    fn append_exec_audit(
        &self,
        _record: &ExecAuditRecord,
    ) -> Result<(), mm::domain::error::MmError> {
        Err(mm::domain::error::MmError::State {
            message: "exec audit is not expected in a dashboard query".to_owned(),
        })
    }
}

struct FixedInstallation(ServiceStackStatus);

#[async_trait]
impl InstallationStatusQuery for FixedInstallation {
    async fn installation_status(&self) -> Result<ServiceStackStatus, mm::domain::error::MmError> {
        Ok(self.0.clone())
    }
}

struct QuerySetup {
    _project: TempDir,
    _home: TempDir,
    service: DashboardQueryService,
    /// The stores the service was built over.
    documents: Arc<dyn ProjectDocumentStore>,
    state: Arc<dyn StateStore>,
}

fn setup() -> Result<QuerySetup, Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    let home = crate::temp_dirs::tempdir()?;
    fs::create_dir(project.path().join(".config"))?;
    fs::create_dir(project.path().join("app"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000042
  name: dashboard-query
default-environment: local
target: local
runtime:
  driver: docker-compose
stacks:
  app:
    files: [compose.yaml]
modules:
  api:
    path: app
    services:
      web:
        stack: app
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
    let targets = Arc::new(FilesystemTargetStore::new(home.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(Uuid::from_u128(7))),
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
            available: false,
            engine_version: None,
            compose_provider: None,
            rootless: None,
            socket_path: None,
            capabilities: Vec::new(),
            diagnostics: vec![Diagnostic::warning("MM_RUNTIME_UNAVAILABLE", SENTINEL)],
        },
        user_rendered: None,
        rendered: BTreeMap::new(),
        observed: BTreeMap::<String, ObservedProject>::new(),
    });
    let state = Arc::new(SqliteStateStore::open(&home.path().join("state.db"))?);
    let clock = Arc::new(FixedClock { unix_ms: NOW });
    let document = documents.load_from(project.path())?;
    state.register_workspace(
        &WorkspaceRegistration {
            project_id: document.config.project.id,
            root: document.root,
            config_path: document.path,
        },
        NOW,
    )?;
    let application = Application::new(ApplicationDependencies {
        documents: documents.clone(),
        environment_sources: Arc::new(EnvironmentIndexer),
        runtime,
        state: state.clone(),
        stacks,
        clock: clock.clone(),
    });
    let operations = RuntimeOperationsService::new(RuntimeOperationsDependencies {
        application,
        driver: Arc::new(NoRuntimeOperations),
        secrets: Arc::new(NoSecrets),
        authorizer: Arc::new(AllowStatus),
        audit: Arc::new(NoAudit),
        clock: clock.clone(),
    });
    let installation = ServiceStackStatus {
        installed: true,
        stack_version: Some("1.2.3".to_owned()),
        runtime: Some(RuntimeKind::DockerCompose),
        target_id: Some(TargetId(Uuid::from_u128(7))),
        route: format!("https://user:{SENTINEL}@private.test"),
        internal_smtp: SENTINEL.to_owned(),
        host_smtp_port: Some(1025),
        components: vec![
            ServiceComponentStatus {
                kind: ServiceComponentKind::LocalIngress,
                version: "1".to_owned(),
                running: true,
                healthy: true,
                image: Some(SENTINEL.to_owned()),
            },
            ServiceComponentStatus {
                kind: ServiceComponentKind::Mailpit,
                version: "1".to_owned(),
                running: true,
                healthy: false,
                image: None,
            },
            ServiceComponentStatus {
                kind: ServiceComponentKind::Cap,
                version: "1".to_owned(),
                running: false,
                healthy: false,
                image: None,
            },
        ],
        diagnostics: (0..=MAX_DASHBOARD_DIAGNOSTICS)
            .map(|_| Diagnostic::warning("MM_SERVICES_NOT_INSTALLED", SENTINEL))
            .collect(),
    };
    let service = DashboardQueryService::new(
        operations,
        documents.clone(),
        state.clone(),
        state.clone(),
        Arc::new(FixedInstallation(installation)),
        clock,
    );
    Ok(QuerySetup {
        _project: project,
        _home: home,
        service,
        documents,
        state,
    })
}

#[tokio::test]
async fn all_dashboard_scopes_are_bounded_registered_and_secret_safe()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    assert!(format!("{:?}", setup.service).contains("DashboardQueryService"));
    let slug = project_dashboard_slug("dashboard-query", ProjectId(Uuid::from_u128(0x42)))?;
    setup
        .service
        .authorize_scope(&DashboardScope::Installation)?;
    setup
        .service
        .authorize_scope(&DashboardScope::Project(slug.clone()))?;
    assert!(
        setup
            .service
            .authorize_scope(&DashboardScope::Project("unknown".to_owned()))
            .is_err()
    );
    assert!(
        setup
            .service
            .query(DashboardScope::Project("unknown".to_owned()))
            .await
            .is_err()
    );

    let project = setup
        .service
        .query(DashboardScope::Project(slug.clone()))
        .await?;
    assert!(matches!(project.scope, DashboardScopeDto::Project { .. }));
    assert_eq!(project.projects.len(), 1);
    assert!(
        project
            .project
            .as_ref()
            .is_some_and(|detail| !detail.runtime_available)
    );

    let installation = setup.service.query(DashboardScope::Installation).await?;
    assert!(matches!(
        installation.scope,
        DashboardScopeDto::Installation
    ));
    assert_eq!(installation.projects.len(), 1);
    assert_eq!(installation.installation.route, "https://mm.test");
    assert_eq!(installation.installation.components.len(), 3);
    assert_eq!(
        installation.installation.diagnostics.len(),
        MAX_DASHBOARD_DIAGNOSTICS
    );

    let hosts = RegisteredDashboardHostSource::new(setup.documents.clone(), setup.state.clone())
        .project_dashboard_hosts()?;
    assert_eq!(hosts, [format!("mm.{slug}.test")].into_iter().collect());
    let encoded = serde_json::to_string(&(project, installation))?;
    assert!(!encoded.contains(SENTINEL));

    let foreign = crate::temp_dirs::tempdir()?;
    fs::create_dir(foreign.path().join(".config"))?;
    fs::copy(
        setup._project.path().join(".config/mm.yaml"),
        foreign.path().join(".config/mm.yaml"),
    )?;
    fs::copy(
        setup._project.path().join("compose.yaml"),
        foreign.path().join("compose.yaml"),
    )?;
    setup.state.register_workspace(
        &WorkspaceRegistration {
            project_id: ProjectId(Uuid::from_u128(0x43)),
            root: foreign.path().to_path_buf(),
            config_path: foreign.path().join(".config/mm.yaml"),
        },
        NOW,
    )?;
    assert!(
        RegisteredDashboardHostSource::new(setup.documents.clone(), setup.state.clone(),)
            .project_dashboard_hosts()
            .is_err()
    );
    Ok(())
}

#[test]
fn registered_project_listing_rejects_duplicate_project_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let duplicate = crate::temp_dirs::tempdir()?;
    fs::create_dir(duplicate.path().join(".config"))?;
    fs::copy(
        setup._project.path().join(".config/mm.yaml"),
        duplicate.path().join(".config/mm.yaml"),
    )?;
    fs::copy(
        setup._project.path().join("compose.yaml"),
        duplicate.path().join("compose.yaml"),
    )?;
    setup.state.register_workspace(
        &WorkspaceRegistration {
            project_id: ProjectId(Uuid::from_u128(0x42)),
            root: duplicate.path().to_path_buf(),
            config_path: duplicate.path().join(".config/mm.yaml"),
        },
        NOW,
    )?;

    let source = RegisteredDashboardHostSource::new(setup.documents.clone(), setup.state.clone());
    assert!(matches!(
        source.project_dashboard_hosts(),
        Err(mm::domain::error::MmError::Security { message })
            if message.contains("multiple registered roots claim dashboard identity")
    ));
    Ok(())
}

#[test]
fn registered_project_listing_rejects_an_oversized_response()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    for index in 0..MAX_DASHBOARD_PROJECTS {
        let root = setup._home.path().join(format!("project-{index}"));
        setup.state.register_workspace(
            &WorkspaceRegistration {
                project_id: ProjectId(Uuid::from_u128(index as u128 + 100)),
                config_path: root.join(".config/mm.yaml"),
                root,
            },
            NOW,
        )?;
    }

    let source = RegisteredDashboardHostSource::new(setup.documents.clone(), setup.state.clone());
    assert!(matches!(
        source.project_dashboard_hosts(),
        Err(mm::domain::error::MmError::State { message })
            if message.contains("dashboard project response exceeds")
    ));
    Ok(())
}
