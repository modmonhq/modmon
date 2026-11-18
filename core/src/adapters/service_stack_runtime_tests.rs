use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tempfile::TempDir;

use super::*;
use crate::adapters::ca::FilesystemLocalCaStore;
use crate::adapters::clock::FixedClock;
use crate::adapters::services::FilesystemServiceStackStore;
use crate::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use crate::app::ports::{DashboardServiceInstaller, TargetOperationLock, TargetOperationLockGuard};
use crate::app::{
    ServicePlanRequest, ServiceStackManager, ServiceStackManagerConfig, TargetIngressManager,
};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::runtime::{
    ContainerObservation, ConvergenceOutcome, RenderedProject, ResourceEnsureResult, RuntimeKind,
    RuntimeProbe, ServiceIdentity, ServiceObservation,
};
use crate::domain::service_stack::{DashboardServiceConfig, ServiceStackAction, ServiceStackPlan};
use crate::domain::target::{NamedTarget, TargetDefinition, TargetId};

struct NoopPlanExecutor;

#[async_trait]
impl ServiceStackExecutor for NoopPlanExecutor {
    async fn preflight(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }

    async fn apply(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }

    async fn status(&self, _plan: &ServiceStackPlan) -> Result<ServiceExecutorStatus, MmError> {
        Ok(ServiceExecutorStatus {
            components: Vec::new(),
            diagnostics: Vec::new(),
        })
    }

    async fn remove(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }
}

struct NoopLock;
struct NoopGuard;

impl TargetOperationLockGuard for NoopGuard {}

impl TargetOperationLock for NoopLock {
    fn acquire(&self, _target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(NoopGuard))
    }
}

#[derive(Default)]
struct FakeRuntime {
    effects: Mutex<Vec<String>>,
    files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl FakeRuntime {
    fn record(&self, effect: impl Into<String>) -> Result<(), MmError> {
        self.effects
            .lock()
            .map_err(|_| MmError::State {
                message: "runtime effect lock was poisoned".to_owned(),
            })?
            .push(effect.into());
        Ok(())
    }
}

#[async_trait]
impl LocalDeploymentRuntime for FakeRuntime {
    async fn preflight(
        &self,
        _target: &NamedTarget,
        resources: &[RuntimeResource],
        projects: &[RuntimeTarget],
    ) -> Result<(), MmError> {
        self.record(format!("preflight:{}:{}", resources.len(), projects.len()))
    }

    async fn ensure_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.record(format!("ensure-network:{resource}"))?;
        Ok(ResourceEnsureResult::Created)
    }

    async fn inspect_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        _resource: &str,
    ) -> Result<bool, MmError> {
        Ok(true)
    }

    async fn remove_network(
        &self,
        _target: &NamedTarget,
        _name: &str,
        resource: &str,
    ) -> Result<(), MmError> {
        self.record(format!("remove-network:{resource}"))
    }

    async fn ensure_volume(
        &self,
        _target: &NamedTarget,
        _name: &str,
        resource: &str,
    ) -> Result<ResourceEnsureResult, MmError> {
        self.record(format!("ensure-volume:{resource}"))?;
        Ok(ResourceEnsureResult::Unchanged)
    }

    async fn remove_volume(
        &self,
        _target: &NamedTarget,
        _name: &str,
        resource: &str,
    ) -> Result<(), MmError> {
        self.record(format!("remove-volume:{resource}"))
    }

    async fn create_volume_file_if_absent(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        volume_resource: &str,
        file_name: &str,
        _mode: u32,
    ) -> Result<(), MmError> {
        self.record(format!("create:{volume_resource}:{file_name}"))
    }

    async fn replace_volume_file(
        &self,
        _target: &NamedTarget,
        _volume_name: &str,
        volume_resource: &str,
        file_name: &str,
        source: &Path,
        _mode: u32,
    ) -> Result<(), MmError> {
        self.files
            .lock()
            .map_err(|_| MmError::State {
                message: "file recorder poisoned".to_owned(),
            })?
            .insert(
                file_name.to_owned(),
                std::fs::read(source).map_err(|source| MmError::Io {
                    context: "reading test publication".to_owned(),
                    source,
                })?,
            );
        self.record(format!("replace:{volume_resource}:{file_name}"))
    }

    async fn apply(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.record(format!("apply:{}", target.stack_name))
    }

    async fn remove(&self, target: &RuntimeTarget) -> Result<(), MmError> {
        self.record(format!("remove:{}", target.stack_name))
    }
}

struct FakeInspector {
    convergence: ConvergenceState,
    degraded: bool,
}

fn service(name: &str, running: bool) -> ServiceObservation {
    ServiceObservation {
        identity: ServiceIdentity {
            project_id: ProjectId(uuid::Uuid::nil()),
            environment: "services".to_owned(),
            stack: "services".to_owned(),
            module: None,
            service: name.to_owned(),
        },
        desired_count: 1,
        running_count: u32::from(running),
        healthy_count: u32::from(running),
        state: if running {
            ServiceState::Running
        } else {
            ServiceState::Stopped
        },
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: None,
    }
}

fn container(name: &str, image: &str) -> ContainerObservation {
    ContainerObservation {
        id: format!("{name}-id"),
        name: name.to_owned(),
        service: name.to_owned(),
        image: image.to_owned(),
        state: "running".to_owned(),
        status: "up".to_owned(),
        health: Some("healthy".to_owned()),
        restart_count: None,
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        revision: None,
        digest: None,
        diagnostic_code: None,
        error: None,
        labels: BTreeMap::new(),
        node: None,
        history: false,
    }
}

impl FakeInspector {
    fn observation(&self, target: &RuntimeTarget) -> ObservedProject {
        if target.stack_name == "ingress" {
            return ObservedProject {
                stack: "ingress".to_owned(),
                services: vec![service("traefik", true)],
                containers: vec![container("traefik", "traefik:pinned")],
            };
        }
        let running = !self.degraded;
        ObservedProject {
            stack: "services".to_owned(),
            services: ["mailpit", "cap", "valkey"]
                .into_iter()
                .map(|name| service(name, running))
                .collect(),
            containers: vec![
                container(
                    "mailpit",
                    if self.degraded {
                        "mailpit:drift"
                    } else {
                        MAILPIT_IMAGE
                    },
                ),
                container(
                    "cap",
                    if self.degraded {
                        "cap:drift"
                    } else {
                        CAP_IMAGE
                    },
                ),
                container(
                    "valkey",
                    if self.degraded {
                        "valkey:drift"
                    } else {
                        VALKEY_IMAGE
                    },
                ),
            ],
        }
    }
}

#[async_trait]
impl RuntimeDriver for FakeInspector {
    fn kind(&self) -> RuntimeKind {
        RuntimeKind::DockerCompose
    }

    async fn probe(&self) -> RuntimeProbe {
        RuntimeProbe::unavailable(
            RuntimeKind::DockerCompose,
            Diagnostic::info("TEST", "fake runtime"),
        )
    }

    async fn render(&self, target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        Ok(RenderedProject {
            stack: target.stack_name.clone(),
            project_name: target.project_name.clone(),
            service_names: Vec::new(),
            services: BTreeMap::new(),
            resources: Vec::new(),
            config_hash: target.artifact_hash.clone(),
        })
    }

    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        Ok(self.observation(target))
    }

    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        _policy: ConvergencePolicy,
        _cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        Ok(ConvergenceOutcome {
            state: self.convergence,
            polls: 1,
            observation: self.observation(target),
            service: None,
            diagnostic_code: (self.convergence != ConvergenceState::Succeeded)
                .then(|| "TEST_NOT_CONVERGED".to_owned()),
            message: None,
        })
    }
}

struct FakeVerifier {
    fail_routes: bool,
}

#[async_trait]
impl IngressVerifier for FakeVerifier {
    async fn verify_listener(&self, _port: u16) -> Result<(), MmError> {
        Ok(())
    }

    async fn verify_route(&self, _port: u16, host: &str, _path: &str) -> Result<(), MmError> {
        if self.fail_routes {
            Err(MmError::RuntimeResponse {
                message: format!("injected route failure for {host}"),
            })
        } else {
            Ok(())
        }
    }
}

struct FakeIngress;

#[async_trait]
impl RootlessIngressInstaller for FakeIngress {
    async fn install(
        &self,
        _plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn uninstall(
        &self,
        _plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError> {
        Ok(())
    }
}

#[async_trait]
impl PrivilegedIngressInstaller for FakeIngress {
    async fn install(
        &self,
        _plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError> {
        Ok(())
    }

    async fn uninstall(
        &self,
        _plan: &crate::domain::target_ingress::TargetIngressPlan,
    ) -> Result<(), MmError> {
        Ok(())
    }
}

struct FakeDashboard {
    status: DashboardServiceStatus,
    fail_status: bool,
}

#[async_trait]
impl DashboardServiceInstaller for FakeDashboard {
    async fn preflight(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }

    async fn install(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }

    async fn status(&self, _plan: &ServiceStackPlan) -> Result<DashboardServiceStatus, MmError> {
        if self.fail_status {
            Err(MmError::RuntimeResponse {
                message: "injected dashboard status failure".to_owned(),
            })
        } else {
            Ok(self.status)
        }
    }

    async fn uninstall(&self, _plan: &ServiceStackPlan) -> Result<(), MmError> {
        Ok(())
    }
}

struct Fixture {
    _temp: TempDir,
    plan: ServiceStackPlan,
    targets: Arc<FilesystemTargetStore>,
    ca: Arc<FilesystemLocalCaStore>,
}

fn fixture() -> Result<Fixture, MmError> {
    let temp = crate::test_support::tempdir().map_err(|source| MmError::Io {
        context: "creating service runtime test root".to_owned(),
        source,
    })?;
    let targets = Arc::new(FilesystemTargetStore::new(temp.path().join("targets.yaml")));
    targets.insert_target(
        "local",
        TargetDefinition::local(TargetId(uuid::Uuid::from_u128(41))),
    )?;
    let ingress = TargetIngressManager::new(
        targets.clone(),
        Arc::new(FilesystemTargetIngressArtifactStore::new(
            temp.path().join("ingress"),
        )),
    );
    let store = Arc::new(FilesystemServiceStackStore::new(
        temp.path().join("services"),
    ));
    let manager = ServiceStackManager::new(ServiceStackManagerConfig {
        runtime: RuntimeKind::DockerCompose,
        targets: targets.clone(),
        ingress,
        store,
        executor: Arc::new(NoopPlanExecutor),
        locks: Arc::new(NoopLock),
        clock: Arc::new(FixedClock { unix_ms: 42 }),
        dashboard_ingress_token: "a".repeat(64),
        dashboard_service: DashboardServiceConfig {
            executable: temp.path().join("bin/mm"),
            mm_home: temp.path().join("home"),
        },
    });
    let plan = manager.plan(ServicePlanRequest {
        action: ServiceStackAction::Apply,
        host_smtp_port: None,
        purge_data: false,
        include_gate: true,
    })?;
    let ca = Arc::new(FilesystemLocalCaStore::new(temp.path().join("ca")));
    ca.install()?;
    Ok(Fixture {
        _temp: temp,
        plan,
        targets,
        ca,
    })
}

#[tokio::test]
async fn service_reconciliation_preserves_registered_project_routes_and_certificates()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::app::ports::IngressRouteRegistry;
    use crate::domain::ingress::{
        IngressCertificateReference, IngressRouteClaim, IngressRouteOwnership,
        IngressRouteProvenance,
    };
    let fixture = fixture()?;
    let state = crate::adapters::state::SqliteStateStore::open(
        &fixture._temp.path().join("state.sqlite3"),
    )?;
    let project_id = ProjectId(uuid::Uuid::from_u128(42));
    let claim = IngressRouteClaim {
        target: "local".to_owned(),
        target_id: fixture.plan.target_id,
        project_id,
        project_name: "store".to_owned(),
        environment: "local".to_owned(),
        stack: "app".to_owned(),
        route: "web".to_owned(),
        host: "web.store.test".to_owned(),
        path_prefix: "/".to_owned(),
        service: "web".to_owned(),
        port: 8080,
        tls: true,
        provenance: IngressRouteProvenance::AutomaticConvention,
        certificate_reference: IngressCertificateReference::LocalCa {
            host: "web.store.test".to_owned(),
        },
        ownership: IngressRouteOwnership::Project {
            project_id,
            environment: "local".to_owned(),
            stack: "app".to_owned(),
            service: "web".to_owned(),
        },
        priority: crate::domain::config_resolve::route_priority("/"),
        router_name: "mm-web".to_owned(),
        backend_alias: "mm-ingress-backend-mm-web".to_owned(),
        artifact_hash: "a".repeat(64),
        middlewares: Vec::new(),
    };
    state.replace_owner_claims(
        fixture.plan.target_id,
        project_id,
        "local",
        &[claim],
        &BTreeMap::new(),
        1,
    )?;
    let before = state.route_set(fixture.plan.target_id)?;
    let runtime = Arc::new(FakeRuntime::default());
    let executor = executor(
        &fixture,
        runtime.clone(),
        ConvergenceState::Succeeded,
        false,
        false,
        DashboardServiceStatus {
            installed: true,
            running: true,
        },
        false,
    );
    executor.apply(&fixture.plan).await?;
    let files = runtime.files.lock().map_err(|_| "files poisoned")?;
    let config: serde_yaml_ng::Value = serde_yaml_ng::from_slice(
        files
            .get("dynamic.yaml")
            .ok_or("project routes not published")?,
    )?;
    assert_eq!(
        config["http"]["routers"]["mm-web"]["rule"],
        "Host(`web.store.test`)"
    );
    assert!(files.contains_key("web.store.test.crt"));
    assert!(files.contains_key("web.store.test.key"));
    assert_eq!(before, state.route_set(fixture.plan.target_id)?);
    Ok(())
}

fn executor(
    fixture: &Fixture,
    runtime: Arc<FakeRuntime>,
    convergence: ConvergenceState,
    degraded: bool,
    fail_routes: bool,
    dashboard: DashboardServiceStatus,
    fail_dashboard_status: bool,
) -> LocalServiceStackExecutor {
    LocalServiceStackExecutor::new(LocalServiceStackExecutorConfig {
        routes: Arc::new(crate::adapters::state::SqliteIngressRouteReader {
            path: fixture._temp.path().join("state.sqlite3"),
        }),
        dynamic_configs: Arc::new(
            crate::adapters::target::FilesystemTraefikDynamicConfigStore::new(
                fixture._temp.path().join("dynamic-configs"),
            ),
        ),
        targets: fixture.targets.clone(),
        runtime,
        inspector: Arc::new(FakeInspector {
            convergence,
            degraded,
        }),
        ca: fixture.ca.clone(),
        verifier: Arc::new(FakeVerifier { fail_routes }),
        rootless: Arc::new(FakeIngress),
        privileged: Arc::new(FakeIngress),
        dashboard: Arc::new(FakeDashboard {
            status: dashboard,
            fail_status: fail_dashboard_status,
        }),
    })
}

#[tokio::test]
async fn executor_applies_observes_and_removes_complete_service_stack()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let runtime = Arc::new(FakeRuntime::default());
    let executor = executor(
        &fixture,
        runtime.clone(),
        ConvergenceState::Succeeded,
        false,
        false,
        DashboardServiceStatus {
            installed: true,
            running: true,
        },
        false,
    );
    assert!(format!("{executor:?}").contains("LocalServiceStackExecutor"));
    assert_eq!(LocalServiceStackExecutor::resources(&fixture.plan).len(), 6);
    assert_eq!(
        LocalServiceStackExecutor::ingress_target(&fixture.plan, &executor.target(&fixture.plan)?)
            .stack_name,
        "ingress"
    );
    assert_eq!(
        LocalServiceStackExecutor::services_target(&fixture.plan, &executor.target(&fixture.plan)?)
            .stack_name,
        "services"
    );

    executor.preflight(&fixture.plan).await?;
    executor.apply(&fixture.plan).await?;
    let status = executor.status(&fixture.plan).await?;
    assert_eq!(status.components.len(), 5);
    assert!(status.components.iter().all(|component| component.running));
    assert!(status.diagnostics.is_empty());

    let mut removal = fixture.plan.clone();
    removal.purge_data = true;
    executor.remove(&removal).await?;
    let effects = runtime.effects.lock().map_err(|_| "effect lock")?;
    assert!(effects.iter().any(|effect| effect == "apply:ingress"));
    assert!(
        effects
            .iter()
            .any(|effect| effect == "remove-volume:cap-valkey-data")
    );
    Ok(())
}

#[tokio::test]
async fn executor_reports_drift_and_stops_after_failed_convergence()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let runtime = Arc::new(FakeRuntime::default());
    let degraded = executor(
        &fixture,
        runtime,
        ConvergenceState::Succeeded,
        true,
        true,
        DashboardServiceStatus {
            installed: false,
            running: false,
        },
        true,
    );
    let status = degraded.status(&fixture.plan).await?;
    assert_eq!(status.components.len(), 5);
    assert!(status.diagnostics.len() >= 5);

    let failed = executor(
        &fixture,
        Arc::new(FakeRuntime::default()),
        ConvergenceState::Failed,
        false,
        false,
        DashboardServiceStatus {
            installed: true,
            running: true,
        },
        false,
    );
    assert!(failed.apply(&fixture.plan).await.is_err());

    let mut changed = fixture.plan.clone();
    changed.target_fingerprint = "0".repeat(64);
    assert!(failed.target(&changed).is_err());

    let mut without_cap = fixture.plan.clone();
    without_cap.cap_valkey_data_volume = None;
    assert!(
        failed
            .cap_components(&without_cap, &ObservedProject::default(), &mut Vec::new())
            .await
            .is_empty()
    );
    let component = service_component(ServiceComponentKind::Mailpit, MAILPIT_VERSION, None, None);
    assert!(!component.running);
    assert!(!component.healthy);
    assert!(component.image.is_none());
    Ok(())
}
