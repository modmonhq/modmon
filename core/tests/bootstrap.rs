use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::state::SqliteStateStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    StateStore, TargetBootstrapExecutor, TargetOperationLock, TargetOperationLockGuard, TargetStore,
};
use mm::app::{
    TargetBootstrapExecutionPorts, TargetBootstrapService, TargetManager, WorkspaceSelection,
};
use mm::domain::error::MmError;
use mm::domain::operation::{NewOperation, OperationId, OperationKind, OperationState};
use mm::domain::target::{BootstrapCapability, BootstrapExecution, TargetBootstrapPlan};
use mm::domain::workspace::WorkspaceRegistration;
use tempfile::TempDir;

mod temp_dirs;

#[derive(Debug)]
struct FakeExecutor {
    calls: Mutex<usize>,
    behavior: ExecutorBehavior,
}

#[derive(Debug, Clone, Copy)]
enum ExecutorBehavior {
    Succeed,
    Fail,
    Incomplete,
}

#[async_trait]
impl TargetBootstrapExecutor for FakeExecutor {
    async fn execute(
        &self,
        _project_root: &Path,
        plan: &TargetBootstrapPlan,
    ) -> Result<BootstrapExecution, MmError> {
        *self.calls.lock().map_err(|_| MmError::State {
            message: "fake bootstrap executor lock was poisoned".to_owned(),
        })? += 1;
        let capabilities = match self.behavior {
            ExecutorBehavior::Succeed => plan.required_capabilities.clone(),
            ExecutorBehavior::Fail => {
                return Err(MmError::Target {
                    message: "injected bootstrap failure".to_owned(),
                });
            }
            ExecutorBehavior::Incomplete => vec![BootstrapCapability::AnsiblePlaybook],
        };
        Ok(BootstrapExecution {
            capabilities,
            access: None,
        })
    }
}

#[derive(Debug)]
struct FakeLock;

#[derive(Debug)]
struct FakeGuard;

impl TargetOperationLockGuard for FakeGuard {}

impl TargetOperationLock for FakeLock {
    fn acquire(
        &self,
        _target_id: mm::domain::target::TargetId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(FakeGuard))
    }
}

struct Fixture {
    project: TempDir,
    _target_directory: TempDir,
    state_directory: TempDir,
    state: Arc<SqliteStateStore>,
    manager: TargetManager,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let project = temp_dirs::tempdir()?;
        fs::write(project.path().join("compose.yaml"), "services: {}\n")?;
        ConfigLoader.initialize(project.path(), Some("bootstrap-service"))?;

        let target_directory = temp_dirs::tempdir()?;
        let targets: Arc<dyn TargetStore> = Arc::new(FilesystemTargetStore::new(
            target_directory.path().join("targets.yaml"),
        ));
        let manager = TargetManager::new(Arc::new(ConfigLoader), targets);
        manager.ensure_local()?;

        let state_directory = temp_dirs::tempdir()?;
        let state = Arc::new(SqliteStateStore::open(
            &state_directory.path().join("state.sqlite3"),
        )?);
        Ok(Self {
            project,
            _target_directory: target_directory,
            state_directory,
            state,
            manager,
        })
    }

    fn selection(&self) -> WorkspaceSelection {
        WorkspaceSelection::new(self.project.path(), None)
    }

    fn service(&self, behavior: ExecutorBehavior) -> (TargetBootstrapService, Arc<FakeExecutor>) {
        let state: Arc<dyn StateStore> = self.state.clone();
        let executor = Arc::new(FakeExecutor {
            calls: Mutex::new(0),
            behavior,
        });
        let service = TargetBootstrapService::new(
            self.manager.clone(),
            TargetBootstrapExecutionPorts {
                documents: Arc::new(ConfigLoader),
                state,
                locks: Arc::new(FakeLock),
                executor: executor.clone(),
                clock: Arc::new(FixedClock { unix_ms: 1000 }),
            },
        );
        (service, executor)
    }

    fn workspace(
        &self,
    ) -> Result<mm::domain::workspace::WorkspaceRecord, Box<dyn std::error::Error>> {
        let document = ConfigLoader.load_from(self.project.path())?;
        Ok(self.state.register_workspace(
            &WorkspaceRegistration {
                project_id: document.config.project.id,
                root: document.root,
                config_path: document.path,
            },
            1000,
        )?)
    }
}

#[tokio::test]
async fn reviewed_bootstrap_is_journaled_with_its_exact_plan_and_result()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let (service, executor) = fixture.service(ExecutorBehavior::Succeed);
    let selection = fixture.selection();
    let plan = service.plan(&selection, None).await?;

    let result = service
        .apply_reviewed(&selection, &plan.plan_hash, None)
        .await?;

    assert_eq!(result.plan_hash, plan.plan_hash);
    assert_eq!(result.target_id, plan.target.definition.id);
    assert_eq!(result.verified_capabilities, plan.required_capabilities);
    assert_eq!(*executor.calls.lock().map_err(|_| "lock poisoned")?, 1);
    let workspace = fixture.workspace()?;
    let operations = fixture.state.recent_operations(workspace.id, 10)?;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind, OperationKind::Bootstrap);
    assert_eq!(operations[0].state, OperationState::Succeeded);
    assert_eq!(
        fixture.state.target_bootstrap_plan(operations[0].id)?,
        Some(plan)
    );

    let connection =
        rusqlite::Connection::open(fixture.state_directory.path().join("state.sqlite3"))?;
    let (result_json, event_count): (String, i64) = connection.query_row(
        "SELECT snapshots.result_json, COUNT(events.sequence)
         FROM target_bootstrap_snapshots AS snapshots
         JOIN operation_events AS events ON events.operation_id = snapshots.operation_id
         WHERE snapshots.operation_id = ?1
         GROUP BY snapshots.operation_id",
        [operations[0].id.0.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert!(result_json.contains(&result.plan_hash));
    assert_eq!(event_count, 5);
    Ok(())
}

#[tokio::test]
async fn changed_inputs_and_executor_failures_cannot_succeed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let selection = fixture.selection();
    let (service, executor) = fixture.service(ExecutorBehavior::Succeed);
    let plan = service.plan(&selection, None).await?;
    fs::write(
        fixture
            .project
            .path()
            .join(".config/ansible/reviewed-note.txt"),
        "changed\n",
    )?;

    assert!(matches!(
        service
            .apply_reviewed(&selection, &plan.plan_hash, None)
            .await,
        Err(MmError::State { .. })
    ));
    assert_eq!(*executor.calls.lock().map_err(|_| "lock poisoned")?, 0);
    assert!(
        fixture
            .state
            .recent_operations(fixture.workspace()?.id, 10)?
            .is_empty()
    );

    let (failing, executor) = fixture.service(ExecutorBehavior::Fail);
    let current = failing.plan(&selection, None).await?;
    assert!(matches!(
        failing
            .apply_reviewed(&selection, &current.plan_hash, None)
            .await,
        Err(MmError::Target { .. })
    ));
    assert_eq!(*executor.calls.lock().map_err(|_| "lock poisoned")?, 1);
    let operations = fixture
        .state
        .recent_operations(fixture.workspace()?.id, 10)?;
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].state, OperationState::Failed);
    assert_eq!(operations[0].error_code.as_deref(), Some("MM_TARGET"));

    let (incomplete, _) = fixture.service(ExecutorBehavior::Incomplete);
    assert!(matches!(
        incomplete
            .apply_reviewed(&selection, &current.plan_hash, None)
            .await,
        Err(MmError::Security { .. })
    ));
    let operations = fixture
        .state
        .recent_operations(fixture.workspace()?.id, 10)?;
    assert!(operations.iter().any(|operation| {
        operation.state == OperationState::Failed
            && operation.error_code.as_deref() == Some("MM_SECURITY")
    }));
    Ok(())
}

#[tokio::test]
async fn retry_interrupts_an_unfinished_bootstrap_for_the_same_target()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let selection = fixture.selection();
    let (service, _) = fixture.service(ExecutorBehavior::Succeed);
    let plan = service.plan(&selection, None).await?;
    let workspace = fixture.workspace()?;
    let abandoned = OperationId::new();
    fixture.state.begin_operation(&NewOperation {
        id: abandoned,
        workspace_id: workspace.id,
        kind: OperationKind::Bootstrap,
        environment: plan.environment.clone(),
        requested_at_unix_ms: 900,
        plan_hash: Some(plan.plan_hash.clone()),
    })?;
    fixture.state.save_target_bootstrap_plan(abandoned, &plan)?;

    service
        .apply_reviewed(&selection, &plan.plan_hash, None)
        .await?;

    let operations = fixture.state.recent_operations(workspace.id, 10)?;
    assert_eq!(operations.len(), 2);
    let previous = operations
        .iter()
        .find(|operation| operation.id == abandoned)
        .ok_or("abandoned operation was not retained")?;
    assert_eq!(previous.state, OperationState::Interrupted);
    assert_eq!(
        previous.error_code.as_deref(),
        Some("MM_BOOTSTRAP_INTERRUPTED")
    );
    assert!(
        operations
            .iter()
            .any(|operation| operation.state == OperationState::Succeeded)
    );
    Ok(())
}

/// Setup stages its files under `MM_HOME/run/bootstrap`. Cluster setup used
/// to create `run` with the usual mode, and the one-host setup's operation
/// lock under `run` then refused to work ("must be symlink-free, private,
/// and owned by the current user").
#[cfg(unix)]
#[test]
fn the_setup_staging_root_and_every_directory_it_makes_above_it_are_private()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let temporary = temp_dirs::tempdir()?;
    let home = temporary.path().join(".mm");
    fs::create_dir(&home)?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    let staging = home.join("run").join("bootstrap");
    mm::adapters::target::prepare_staging_root(&staging)?;
    for directory in [home.join("run"), staging] {
        let mode = fs::metadata(&directory)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{} is {mode:o}", directory.display());
    }
    Ok(())
}
