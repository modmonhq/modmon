use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mm::adapters::clock::FixedClock;
use mm::adapters::services::FilesystemServiceStackStore;
use mm::adapters::target::{FilesystemTargetIngressArtifactStore, FilesystemTargetStore};
use mm::app::ports::{
    DashboardHostSource, GatePolicySource, ServiceExecutorStatus, ServiceStackArtifactInput,
    ServiceStackExecutor, ServiceStackStore, TargetOperationLock, TargetOperationLockGuard,
    TargetStore,
};
use mm::app::{
    ServicePlanRequest, ServiceStackManager, ServiceStackManagerConfig, TargetIngressManager,
};
use mm::domain::diagnostic::Diagnostic;
use mm::domain::error::MmError;
use mm::domain::runtime::RuntimeKind;
use mm::domain::service_stack::DashboardServiceConfig;
use mm::domain::service_stack::{
    CAP_IMAGE, MAILPIT_IMAGE, PreparedServiceStackArtifact, ServiceComponentKind,
    ServiceComponentStatus, ServiceStackAction, ServiceStackJournalState, ServiceStackState,
    VALKEY_IMAGE,
};
use mm::domain::target::{TargetDefinition, TargetId};
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

const TARGET_ID: u128 = 0x319c_527b_6242_7b43_bfd4_bd60_e27c_1de3;

#[test]
fn generated_user_units_use_valid_cli_commands_and_the_selected_runtime()
-> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser;
    for (runtime, engine) in [
        (RuntimeKind::DockerCompose, "docker"),
        (RuntimeKind::PodmanCompose, "podman"),
    ] {
        let fixture = Fixture::new_with_runtime(false, runtime)?;
        let plan = fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Apply))?;
        let dashboard = std::fs::read_to_string(
            plan.artifact
                .dashboard_unit_path
                .as_ref()
                .ok_or("dashboard unit")?,
        )?;
        let executable = dashboard
            .lines()
            .find(|line| line.starts_with("ExecStart="))
            .ok_or("ExecStart")?;
        let (_, suffix) = executable
            .split_once(" --runtime ")
            .ok_or("runtime selection")?;
        mm::presentation::cli::Cli::try_parse_from(
            std::iter::once("mm")
                .chain(["--runtime"])
                .chain(suffix.split_ascii_whitespace()),
        )?;
        assert!(dashboard.contains("NoNewPrivileges=true"));
        for directive in [
            "ProtectHome=",
            "ProtectSystem=",
            "PrivateTmp=",
            "PrivateUsers=",
        ] {
            assert!(
                !dashboard.contains(directive),
                "user-namespace remapping breaks filesystem ownership checks: {directive}"
            );
        }
        let ingress_path = plan
            .ingress
            .support_paths
            .iter()
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name == "mm-ingress.service")
            })
            .ok_or("ingress unit")?;
        let ingress = std::fs::read_to_string(ingress_path)?;
        assert!(ingress.contains(&format!("ExecStart=/usr/bin/{engine} compose")));
        assert!(ingress.contains(&format!("ExecStop=/usr/bin/{engine} compose")));
    }
    Ok(())
}

#[derive(Default)]
struct FakeExecutor {
    applied: Mutex<Vec<String>>,
    removed: Mutex<Vec<(String, bool)>>,
    fail_preflight: Mutex<bool>,
    fail_next_apply: Mutex<bool>,
    fail_next_remove: Mutex<bool>,
}

impl FakeExecutor {
    fn set_preflight_failure(&self, fail: bool) -> Result<(), MmError> {
        *self.fail_preflight.lock().map_err(lock_error)? = fail;
        Ok(())
    }

    fn fail_next_apply(&self) -> Result<(), MmError> {
        *self.fail_next_apply.lock().map_err(lock_error)? = true;
        Ok(())
    }

    fn fail_next_remove(&self) -> Result<(), MmError> {
        *self.fail_next_remove.lock().map_err(lock_error)? = true;
        Ok(())
    }

    fn applied_count(&self) -> Result<usize, MmError> {
        self.applied
            .lock()
            .map(|items| items.len())
            .map_err(lock_error)
    }
}

#[async_trait::async_trait]
impl ServiceStackExecutor for FakeExecutor {
    async fn preflight(
        &self,
        _plan: &mm::domain::service_stack::ServiceStackPlan,
    ) -> Result<(), MmError> {
        if *self.fail_preflight.lock().map_err(lock_error)? {
            return Err(MmError::Security {
                message: "foreign same-name resource".to_owned(),
            });
        }
        Ok(())
    }

    async fn apply(
        &self,
        plan: &mm::domain::service_stack::ServiceStackPlan,
    ) -> Result<(), MmError> {
        let mut fail = self.fail_next_apply.lock().map_err(lock_error)?;
        if *fail {
            *fail = false;
            return Err(MmError::RuntimeResponse {
                message: "injected apply failure".to_owned(),
            });
        }
        self.applied
            .lock()
            .map_err(lock_error)?
            .push(plan.plan_hash.clone());
        Ok(())
    }

    async fn status(
        &self,
        _plan: &mm::domain::service_stack::ServiceStackPlan,
    ) -> Result<ServiceExecutorStatus, MmError> {
        Ok(ServiceExecutorStatus {
            components: vec![ServiceComponentStatus {
                kind: ServiceComponentKind::Mailpit,
                version: "test".to_owned(),
                running: true,
                healthy: true,
                image: Some(MAILPIT_IMAGE.to_owned()),
            }],
            diagnostics: vec![Diagnostic::info("TEST_OBSERVED", "executor was queried")],
        })
    }

    async fn remove(
        &self,
        plan: &mm::domain::service_stack::ServiceStackPlan,
    ) -> Result<(), MmError> {
        let mut fail = self.fail_next_remove.lock().map_err(lock_error)?;
        if *fail {
            *fail = false;
            return Err(MmError::RuntimeResponse {
                message: "injected remove failure".to_owned(),
            });
        }
        self.removed
            .lock()
            .map_err(lock_error)?
            .push((plan.plan_hash.clone(), plan.purge_data));
        Ok(())
    }
}

struct Fixture {
    _root: TempDir,
    manager: ServiceStackManager,
    store: Arc<TestStore>,
    executor: Arc<FakeExecutor>,
}

struct TestStore {
    inner: FilesystemServiceStackStore,
    override_state: Mutex<Option<ServiceStackState>>,
}

impl TestStore {
    fn new(root: PathBuf) -> Self {
        Self {
            inner: FilesystemServiceStackStore::new(root),
            override_state: Mutex::new(None),
        }
    }

    fn override_state(&self, state: ServiceStackState) -> Result<(), MmError> {
        *self.override_state.lock().map_err(lock_error)? = Some(state);
        Ok(())
    }
}

impl ServiceStackStore for TestStore {
    fn load_state(&self) -> Result<Option<ServiceStackState>, MmError> {
        if let Some(state) = self.override_state.lock().map_err(lock_error)?.clone() {
            Ok(Some(state))
        } else {
            self.inner.load_state()
        }
    }

    fn save_state(&self, state: &ServiceStackState) -> Result<(), MmError> {
        self.inner.save_state(state)
    }

    fn remove_state(&self) -> Result<(), MmError> {
        self.inner.remove_state()
    }

    fn ensure_cap_admin_key_file(&self) -> Result<PathBuf, MmError> {
        self.inner.ensure_cap_admin_key_file()
    }

    fn publish(
        &self,
        artifact: ServiceStackArtifactInput<'_>,
    ) -> Result<PreparedServiceStackArtifact, MmError> {
        self.inner.publish(artifact)
    }
}

struct NoopLock;
struct NoopGuard;
struct FixedGatePolicies(bool);
struct FixedDashboardHosts;

impl GatePolicySource for FixedGatePolicies {
    fn gate_components_required(&self) -> Result<bool, MmError> {
        Ok(self.0)
    }
}

impl DashboardHostSource for FixedDashboardHosts {
    fn project_dashboard_hosts(&self) -> Result<BTreeSet<String>, MmError> {
        Ok(BTreeSet::from(["mm.project.test".to_owned()]))
    }
}

impl TargetOperationLockGuard for NoopGuard {}

impl TargetOperationLock for NoopLock {
    fn acquire(&self, _target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(NoopGuard))
    }
}

impl Fixture {
    fn new() -> Result<Self, MmError> {
        Self::new_with_gate(false)
    }

    fn new_with_gate(gate_enabled: bool) -> Result<Self, MmError> {
        Self::new_with_runtime(gate_enabled, RuntimeKind::DockerCompose)
    }

    fn new_with_runtime(gate_enabled: bool, runtime: RuntimeKind) -> Result<Self, MmError> {
        let root = temp_dirs::tempdir().map_err(|source| MmError::Io {
            context: "creating services test root".to_owned(),
            source,
        })?;
        let targets = Arc::new(FilesystemTargetStore::new(root.path().join("targets.yaml")));
        targets.insert_target(
            "local",
            TargetDefinition::local(TargetId(Uuid::from_u128(TARGET_ID))),
        )?;
        let ingress = TargetIngressManager::new(
            targets.clone(),
            Arc::new(FilesystemTargetIngressArtifactStore::new(
                root.path().join("ingress"),
            )),
        );
        let store = Arc::new(TestStore::new(root.path().join("services")));
        let executor = Arc::new(FakeExecutor::default());
        let manager = ServiceStackManager::new(ServiceStackManagerConfig {
            runtime,
            targets,
            ingress,
            store: store.clone(),
            executor: executor.clone(),
            locks: Arc::new(NoopLock),
            clock: Arc::new(FixedClock {
                unix_ms: 1_700_000_000_000,
            }),
            dashboard_ingress_token: "a".repeat(64),
            dashboard_service: DashboardServiceConfig {
                executable: root.path().join("bin/mm"),
                mm_home: root.path().join("mm-home"),
            },
        })
        .with_gate_policies(Arc::new(FixedGatePolicies(gate_enabled)));
        Ok(Self {
            _root: root,
            manager,
            store,
            executor,
        })
    }

    fn request(action: ServiceStackAction) -> ServicePlanRequest {
        ServicePlanRequest {
            action,
            host_smtp_port: None,
            purge_data: false,
            include_gate: false,
        }
    }

    async fn install(&self) -> Result<String, MmError> {
        let request = Self::request(ServiceStackAction::Apply);
        let plan = self.manager.plan(request)?;
        self.manager
            .execute_reviewed(request, &plan.plan_hash)
            .await?;
        Ok(plan.plan_hash)
    }
}

#[test]
fn cap_and_valkey_are_conditional_pinned_private_and_admin_routes_are_not_exposed()
-> Result<(), Box<dyn std::error::Error>> {
    let absent = Fixture::new()?;
    let absent_plan = absent
        .manager
        .plan(Fixture::request(ServiceStackAction::Apply))?;
    let absent_compose = std::fs::read_to_string(&absent_plan.artifact.compose_path)?;
    assert!(!absent_compose.contains(CAP_IMAGE));
    assert!(!absent_compose.contains(VALKEY_IMAGE));
    assert!(absent_plan.cap_admin_key_file.is_none());

    let mut preparation = Fixture::request(ServiceStackAction::Apply);
    preparation.include_gate = true;
    let preparation_plan = absent.manager.plan(preparation)?;
    let preparation_compose = std::fs::read_to_string(&preparation_plan.artifact.compose_path)?;
    assert!(preparation_compose.contains(CAP_IMAGE));
    assert!(preparation_compose.contains(VALKEY_IMAGE));
    assert_ne!(preparation_plan.plan_hash, absent_plan.plan_hash);

    let present = Fixture::new_with_gate(true)?;
    let present_plan = present
        .manager
        .plan(Fixture::request(ServiceStackAction::Apply))?;
    let compose = std::fs::read_to_string(&present_plan.artifact.compose_path)?;
    let route = std::fs::read_to_string(&present_plan.artifact.route_path)?;
    let key_path = present_plan
        .cap_admin_key_file
        .as_ref()
        .ok_or("Cap admin key path is missing")?;
    let key = std::fs::read_to_string(key_path)?;

    assert!(compose.contains(CAP_IMAGE));
    assert!(compose.contains(VALKEY_IMAGE));
    assert!(compose.contains("CORS_ORIGIN: '*'") || compose.contains("CORS_ORIGIN: \"*\""));
    assert!(!compose.contains("ENABLE_ASSETS_SERVER"));
    assert!(!compose.contains("CACHE_HOST"));
    assert!(compose.contains("user: 999:1000"));
    assert!(compose.contains("cap_drop:\n    - ALL"));
    assert!(!compose.contains(&key));
    assert!(!route.contains(&key));
    assert!(!compose.contains("docker.sock"));
    assert!(!compose.contains("podman.sock"));
    assert!(!route.contains("PathPrefix(`/assets/`)"));
    assert!(route.contains("challenge|redeem"));
    assert!(!route.contains("challenge|redeem|siteverify"));
    assert!(route.contains("mm-cap-admin:\n      rule: Host(`cap.mm.test`)"));
    assert!(!route.contains("mm-cap-admin-secure"));
    assert!(route.contains("mm-gate-ingress-auth:"));
    assert!(route.contains("mm-gate-ingress-clear:"));
    assert!(present_plan.components.iter().any(|component| {
        component.kind == ServiceComponentKind::Cap && component.image.as_deref() == Some(CAP_IMAGE)
    }));
    assert!(present_plan.components.iter().any(|component| {
        component.kind == ServiceComponentKind::CapValkey
            && component.image.as_deref() == Some(VALKEY_IMAGE)
    }));
    Ok(())
}

#[tokio::test]
async fn initial_install_and_reapply_are_transactional_and_idempotent()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let request = Fixture::request(ServiceStackAction::Apply);
    let plan = fixture.manager.plan(request)?;
    fixture
        .manager
        .execute_reviewed(request, &plan.plan_hash)
        .await?;
    let state = fixture
        .store
        .load_state()?
        .ok_or_else(|| std::io::Error::other("service state is missing"))?;
    assert_eq!(state.journal_state, ServiceStackJournalState::Applied);
    assert_eq!(fixture.executor.applied_count()?, 2);
    let status = fixture.manager.status().await?;
    assert!(status.installed);
    assert_eq!(status.components.len(), 1);
    assert!(
        status
            .diagnostics
            .iter()
            .any(|item| item.code == "TEST_OBSERVED")
    );
    Ok(())
}

#[tokio::test]
async fn reviewed_execution_rejects_stale_hashes_without_effects_and_rolls_back_initial_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let request = Fixture::request(ServiceStackAction::Apply);
    assert!(matches!(
        fixture.manager.execute_reviewed(request, "stale").await,
        Err(MmError::State { .. })
    ));
    assert_eq!(fixture.executor.applied_count()?, 0);
    assert!(fixture.store.load_state()?.is_none());

    let plan = fixture.manager.plan(request)?;
    fixture.executor.fail_next_apply()?;
    assert!(matches!(
        fixture
            .manager
            .execute_reviewed(request, &plan.plan_hash)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));
    assert!(fixture.store.load_state()?.is_none());
    assert_eq!(
        fixture.executor.removed.lock().map_err(lock_error)?.len(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn rollback_failure_is_journaled_for_reviewed_repair()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let request = Fixture::request(ServiceStackAction::Apply);
    let plan = fixture.manager.plan(request)?;
    fixture.executor.fail_next_apply()?;
    fixture.executor.fail_next_remove()?;
    let error = fixture
        .manager
        .execute_reviewed(request, &plan.plan_hash)
        .await
        .err()
        .ok_or("operation and rollback should fail")?;
    assert!(matches!(error, MmError::Security { .. }));
    let failed = fixture
        .store
        .load_state()?
        .ok_or("failed journal is missing")?;
    assert_eq!(failed.journal_state, ServiceStackJournalState::Failed);
    assert_eq!(failed.failure_code.as_deref(), Some("MM_RUNTIME_RESPONSE"));
    Ok(())
}

#[tokio::test]
async fn failed_reapply_restores_the_current_release_and_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let previous = fixture
        .store
        .load_state()?
        .ok_or_else(|| std::io::Error::other("service state is missing"))?;
    let request = Fixture::request(ServiceStackAction::Apply);
    let plan = fixture.manager.plan(request)?;
    fixture.executor.fail_next_apply()?;
    assert!(
        fixture
            .manager
            .execute_reviewed(request, &plan.plan_hash)
            .await
            .is_err()
    );
    let restored = fixture
        .store
        .load_state()?
        .ok_or_else(|| std::io::Error::other("restored state is missing"))?;
    assert_eq!(restored.plan_hash, previous.plan_hash);
    assert_eq!(restored.stack_version, "1.0.0");
    assert_eq!(restored.journal_state, ServiceStackJournalState::Applied);
    Ok(())
}

#[tokio::test]
async fn repair_recovers_an_interrupted_transaction_from_its_journal()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    assert_eq!(fixture.manager.setup_action()?, ServiceStackAction::Apply);
    fixture.install().await?;
    let previous = fixture
        .store
        .load_state()?
        .ok_or_else(|| std::io::Error::other("service state is missing"))?;
    let mut interrupted = previous.clone();
    interrupted.journal_state = ServiceStackJournalState::Applying;
    interrupted.previous = Some(Box::new(previous.clone()));
    fixture.store.save_state(&interrupted)?;
    assert_eq!(fixture.manager.setup_action()?, ServiceStackAction::Repair);
    let request = Fixture::request(ServiceStackAction::Repair);
    let plan = fixture.manager.plan(request)?;
    let result = fixture
        .manager
        .execute_reviewed(request, &plan.plan_hash)
        .await?;
    assert!(result.rolled_back);
    let recovered = fixture
        .store
        .load_state()?
        .ok_or_else(|| std::io::Error::other("recovered state is missing"))?;
    assert_eq!(recovered.journal_state, ServiceStackJournalState::Applied);
    assert!(recovered.previous.is_none());
    assert_eq!(fixture.manager.setup_action()?, ServiceStackAction::Apply);
    Ok(())
}

#[tokio::test]
async fn repair_without_a_previous_release_converges_without_removing_shared_networks()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let mut interrupted = fixture.store.load_state()?.ok_or("state is missing")?;
    interrupted.journal_state = ServiceStackJournalState::Applying;
    assert!(interrupted.previous.is_none());
    fixture.store.save_state(&interrupted)?;
    // Model Docker rejecting network removal because project endpoints exist.
    fixture.executor.fail_next_remove()?;

    let request = Fixture::request(ServiceStackAction::Repair);
    let plan = fixture.manager.plan(request)?;
    let result = fixture
        .manager
        .execute_reviewed(request, &plan.plan_hash)
        .await?;
    assert!(!result.rolled_back);
    let recovered = fixture
        .store
        .load_state()?
        .ok_or("recovered state missing")?;
    assert_eq!(recovered.journal_state, ServiceStackJournalState::Applied);
    assert_eq!(recovered.applied_plan.plan_hash, plan.plan_hash);
    assert!(!result.data_purged);
    assert_eq!(
        fixture.executor.removed.lock().map_err(lock_error)?.len(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn failed_forward_repair_retains_resources_and_can_be_retried()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let mut interrupted = fixture.store.load_state()?.ok_or("state missing")?;
    interrupted.journal_state = ServiceStackJournalState::Failed;
    interrupted.previous = None;
    fixture.store.save_state(&interrupted)?;
    fixture.executor.fail_next_apply()?;
    fixture.executor.fail_next_remove()?;
    let request = Fixture::request(ServiceStackAction::Repair);
    let plan = fixture.manager.plan(request)?;
    assert!(matches!(
        fixture
            .manager
            .execute_reviewed(request, &plan.plan_hash)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));
    let retained = fixture
        .store
        .load_state()?
        .ok_or("failed repair lost its journal")?;
    assert_eq!(retained.journal_state, ServiceStackJournalState::Failed);
    assert!(retained.previous.is_none());
    assert!(
        fixture
            .executor
            .removed
            .lock()
            .map_err(lock_error)?
            .is_empty()
    );
    let retry = fixture.manager.plan(request)?;
    let result = fixture
        .manager
        .execute_reviewed(request, &retry.plan_hash)
        .await?;
    assert_eq!(result.state, ServiceStackJournalState::Applied);
    assert!(!result.data_purged);
    assert!(
        fixture
            .executor
            .removed
            .lock()
            .map_err(lock_error)?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn foreign_collision_fails_before_any_mutation() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.executor.set_preflight_failure(true)?;
    let request = Fixture::request(ServiceStackAction::Apply);
    let plan = fixture.manager.plan(request)?;
    assert!(matches!(
        fixture
            .manager
            .execute_reviewed(request, &plan.plan_hash)
            .await,
        Err(MmError::Security { .. })
    ));
    assert_eq!(fixture.executor.applied_count()?, 0);
    assert!(fixture.store.load_state()?.is_none());
    Ok(())
}

#[tokio::test]
async fn remove_retains_data_unless_the_reviewed_plan_purges_it()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let request = Fixture::request(ServiceStackAction::Remove);
    let plan = fixture.manager.plan(request)?;
    let result = fixture
        .manager
        .execute_reviewed(request, &plan.plan_hash)
        .await?;
    assert!(!result.data_purged);
    assert!(fixture.store.load_state()?.is_none());

    fixture.install().await?;
    let request = ServicePlanRequest {
        action: ServiceStackAction::Remove,
        host_smtp_port: None,
        purge_data: true,
        include_gate: false,
    };
    let plan = fixture.manager.plan(request)?;
    let result = fixture
        .manager
        .execute_reviewed(request, &plan.plan_hash)
        .await?;
    assert!(result.data_purged);
    assert_eq!(
        fixture.executor.removed.lock().map_err(lock_error)?.len(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn failed_remove_restores_the_installed_release() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let previous = fixture.store.load_state()?.ok_or("state is missing")?;
    let request = Fixture::request(ServiceStackAction::Remove);
    let plan = fixture.manager.plan(request)?;
    fixture.executor.fail_next_remove()?;
    assert!(
        fixture
            .manager
            .execute_reviewed(request, &plan.plan_hash)
            .await
            .is_err()
    );
    let restored = fixture.store.load_state()?.ok_or("state is missing")?;
    assert_eq!(restored.plan_hash, previous.plan_hash);
    assert_eq!(restored.journal_state, ServiceStackJournalState::Applied);
    Ok(())
}

#[tokio::test]
async fn status_reports_absence_ownership_version_and_journal_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let absent = fixture.manager.status().await?;
    assert!(!absent.installed);
    assert_eq!(absent.diagnostics[0].code, "MM_SERVICES_NOT_INSTALLED");

    fixture.install().await?;
    let mut state = fixture.store.load_state()?.ok_or("state is missing")?;
    state.runtime = RuntimeKind::PodmanCompose;
    fixture.store.override_state(state.clone())?;
    let mismatch = fixture.manager.status().await?;
    assert_eq!(mismatch.diagnostics[0].code, "MM_SERVICES_RUNTIME_MISMATCH");
    assert!(mismatch.components.is_empty());

    state.runtime = RuntimeKind::DockerCompose;
    state.stack_version = "0.9.0".to_owned();
    state.journal_state = ServiceStackJournalState::Removing;
    fixture.store.override_state(state)?;
    let drift = fixture.manager.status().await?;
    assert!(
        drift
            .diagnostics
            .iter()
            .any(|item| item.code == "MM_SERVICES_VERSION_DRIFT")
    );
    assert!(
        drift
            .diagnostics
            .iter()
            .any(|item| item.code == "MM_SERVICES_RECOVERY_REQUIRED")
    );
    assert_eq!(drift.components.len(), 1);
    Ok(())
}

#[test]
fn planning_rejects_invalid_action_state_runtime_and_port_combinations()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    for action in [ServiceStackAction::Repair, ServiceStackAction::Remove] {
        assert!(fixture.manager.plan(Fixture::request(action)).is_err());
    }
    assert!(
        fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Upgrade))
            .is_err()
    );

    let mut invalid = Fixture::request(ServiceStackAction::Apply);
    invalid.purge_data = true;
    assert!(matches!(
        fixture.manager.plan(invalid),
        Err(MmError::Security { .. })
    ));
    for port in [80, 443, 8025] {
        let mut invalid = Fixture::request(ServiceStackAction::Apply);
        invalid.host_smtp_port = Some(port);
        assert!(fixture.manager.plan(invalid).is_err());
    }
    let mut invalid = Fixture::request(ServiceStackAction::Repair);
    invalid.include_gate = true;
    assert!(fixture.manager.plan(invalid).is_err());
    Ok(())
}

#[tokio::test]
async fn installed_state_rejects_foreign_interrupted_and_invalid_upgrade_records()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture.install().await?;
    let original = fixture.store.load_state()?.ok_or("state is missing")?;

    let mut state = original.clone();
    state.target_id = TargetId(Uuid::from_u128(TARGET_ID + 1));
    fixture.store.override_state(state)?;
    assert!(matches!(
        fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Apply)),
        Err(MmError::Security { .. })
    ));

    let mut state = original.clone();
    state.journal_state = ServiceStackJournalState::Failed;
    fixture.store.override_state(state)?;
    assert!(matches!(
        fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Apply)),
        Err(MmError::State { .. })
    ));

    fixture.store.override_state(original.clone())?;
    assert!(
        fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Upgrade))
            .is_err()
    );
    let mut state = original.clone();
    state.stack_version = "invalid".to_owned();
    fixture.store.override_state(state.clone())?;
    assert!(
        fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Upgrade))
            .is_err()
    );
    state.stack_version = "0.9.0".to_owned();
    fixture.store.override_state(state)?;
    assert!(
        fixture
            .manager
            .plan(Fixture::request(ServiceStackAction::Upgrade))
            .is_ok()
    );
    Ok(())
}

#[test]
fn dashboard_hosts_are_deduplicated_and_manager_debug_redacts_dependencies()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let manager = fixture
        .manager
        .clone()
        .with_dashboard_hosts(Arc::new(FixedDashboardHosts));
    let plan = manager.plan(Fixture::request(ServiceStackAction::Apply))?;
    assert_eq!(
        plan.dashboard_hosts,
        vec!["mm.project.test".to_owned(), "mm.test".to_owned()]
    );
    assert_eq!(format!("{manager:?}"), "ServiceStackManager { .. }");
    Ok(())
}

#[test]
fn service_planning_skips_registered_projects_whose_folder_is_gone()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::adapters::config::ConfigLoader;
    use mm::adapters::state::SqliteStateStore;
    use mm::app::RegisteredDashboardHostSource;
    use mm::app::ports::StateStore;
    use mm::domain::workspace::{ProjectId, WorkspaceRegistration};

    let fixture = Fixture::new()?;
    let home = temp_dirs::tempdir()?;
    let state = Arc::new(SqliteStateStore::open(&home.path().join("state.sqlite3"))?);
    let mut roots = Vec::new();
    for (id, name) in [(0x51_u128, "kept"), (0x52, "gone")] {
        let root = home.path().join(name);
        std::fs::create_dir_all(root.join(".config"))?;
        std::fs::write(
            root.join(".config/mm.yaml"),
            format!(
                "schema-version: 1\nproject:\n  id: {}\n  name: {name}\ntarget: local\nstacks: {{}}\nenvironments:\n  local:\n    bindings: {{}}\n",
                Uuid::from_u128(id)
            ),
        )?;
        let document = ConfigLoader.load_from(&root)?;
        state.register_workspace(
            &WorkspaceRegistration {
                project_id: ProjectId(Uuid::from_u128(id)),
                root: document.root,
                config_path: document.path,
            },
            1_700_000_000_000,
        )?;
        roots.push(root);
    }
    std::fs::remove_dir_all(&roots[1])?;

    let manager =
        fixture
            .manager
            .clone()
            .with_dashboard_hosts(Arc::new(RegisteredDashboardHostSource::new(
                Arc::new(ConfigLoader),
                state,
            )));
    let plan = manager.plan(Fixture::request(ServiceStackAction::Apply))?;
    let kept = mm::domain::local_ingress::project_dashboard_slug(
        "kept",
        ProjectId(Uuid::from_u128(0x51)),
    )?;
    assert_eq!(
        plan.dashboard_hosts,
        vec![format!("mm.{kept}.test"), "mm.test".to_owned()]
    );
    Ok(())
}

#[test]
fn generated_topology_is_pinned_limited_socketless_and_smtp_private_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let plan = fixture
        .manager
        .plan(Fixture::request(ServiceStackAction::Apply))?;
    let compose = std::fs::read_to_string(&plan.artifact.compose_path)?;
    let route = std::fs::read_to_string(&plan.artifact.route_path)?;
    assert!(compose.contains(MAILPIT_IMAGE));
    assert!(compose.contains("cpus: 1.0"));
    assert!(compose.contains("mem_limit: 256m"));
    assert!(compose.contains("pids_limit: 128"));
    assert!(!compose.contains("docker.sock"));
    assert!(!compose.contains("podman.sock"));
    assert!(!compose.contains("ports:"));
    assert!(compose.contains("mailpit-data:/data"));
    assert!(route.contains("entryPoints: [websecure]"));
    assert!(route.contains("entryPoints: [mm-verify]"));
    assert!(route.contains(r"HostRegexp(`^mm\.[a-z0-9-]+\.test$`)"));
    assert!(route.contains("X-MM-Route-Verified: \"1\""));
    assert!(
        plan.components
            .iter()
            .all(|component| !component.engine_socket_mounted)
    );
    assert!(
        plan.components
            .iter()
            .flat_map(|component| &component.volumes)
            .all(|volume| volume.retained_by_default)
    );
    Ok(())
}

#[test]
fn pre_release_service_plans_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut plan = fixture
        .manager
        .plan(Fixture::request(ServiceStackAction::Apply))?;
    plan.stack_version = "2.0.0".to_owned();
    plan.plan_hash = plan.canonical_hash()?;
    assert!(plan.validate().is_err());
    Ok(())
}

#[test]
fn service_plan_validation_rejects_each_noncanonical_dashboard_and_component_shape()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let plan = fixture
        .manager
        .plan(Fixture::request(ServiceStackAction::Apply))?;
    assert!(plan.validate().is_ok());
    assert_eq!(
        [
            ServiceStackAction::Apply,
            ServiceStackAction::Upgrade,
            ServiceStackAction::Repair,
            ServiceStackAction::Remove,
        ]
        .map(ServiceStackAction::as_str),
        ["apply", "upgrade", "repair", "remove"]
    );

    macro_rules! rejects {
        ($change:expr) => {{
            let mut candidate = plan.clone();
            $change(&mut candidate);
            candidate.plan_hash = candidate.canonical_hash()?;
            assert!(candidate.validate().is_err());
        }};
    }

    rejects!(|value: &mut mm::domain::service_stack::ServiceStackPlan| value.schema = 2);
    let mut stale = plan.clone();
    stale.plan_hash = "stale".to_owned();
    assert!(stale.validate().is_err());
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.components[0]
            .engine_socket_mounted = true
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.host_smtp_port = Some(0)
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.previous_host_smtp_port =
            Some(0)
    );

    for hosts in [
        Vec::new(),
        vec!["mm.test".to_owned(), "mm.test".to_owned()],
        vec!["invalid.example".to_owned()],
        vec!["mm..test".to_owned()],
        vec!["mm.-project.test".to_owned()],
        vec!["mm.project-.test".to_owned()],
        vec!["mm.Project.test".to_owned()],
        vec![format!("mm.{}.test", "x".repeat(64))],
    ] {
        rejects!(
            |value: &mut mm::domain::service_stack::ServiceStackPlan| value.dashboard_hosts =
                hosts.clone()
        );
    }
    rejects!(|value: &mut mm::domain::service_stack::ServiceStackPlan| {
        if let Some(service) = value.dashboard_service.as_mut() {
            service.executable = PathBuf::from("relative");
        }
    });
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value
            .artifact
            .dashboard_unit_path = None
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value
            .artifact
            .dashboard_unit_digest = None
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value
            .artifact
            .dashboard_unit_path =
            Some(PathBuf::from("/outside/mm.service"))
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value
            .artifact
            .dashboard_unit_digest =
            Some("g".repeat(64))
    );
    rejects!(|value: &mut mm::domain::service_stack::ServiceStackPlan| {
        value.dashboard_service = None;
        value.artifact.dashboard_unit_path = None;
        value.artifact.dashboard_unit_digest = None;
    });
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.dashboard_service = None
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.components[0].image =
            Some("mailpit:latest".to_owned())
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.cap_valkey_data_volume =
            Some("unexpected".to_owned())
    );
    rejects!(
        |value: &mut mm::domain::service_stack::ServiceStackPlan| value.cap_admin_key_file =
            Some(PathBuf::from("relative"))
    );
    Ok(())
}

#[test]
fn host_smtp_requires_an_explicit_reviewed_loopback_port() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let plan = fixture.manager.plan(ServicePlanRequest {
        action: ServiceStackAction::Apply,
        host_smtp_port: Some(10_252),
        purge_data: false,
        include_gate: false,
    })?;
    let compose = std::fs::read_to_string(&plan.artifact.compose_path)?;
    assert!(compose.contains("127.0.0.1:10252:1025"));
    assert!(!compose.contains("0.0.0.0:10252"));
    assert!(
        fixture
            .manager
            .plan(ServicePlanRequest {
                action: ServiceStackAction::Apply,
                host_smtp_port: Some(80),
                purge_data: false,
                include_gate: false,
            })
            .is_err()
    );
    Ok(())
}

fn lock_error<T>(_error: std::sync::PoisonError<T>) -> MmError {
    MmError::State {
        message: "services test lock was poisoned".to_owned(),
    }
}
