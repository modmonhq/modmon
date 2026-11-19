//! The job scheduler service against a scripted repository, runtime,
//! authorizer, and secret resolver: reconciliation, ticks, runs, retries,
//! timeouts, and cancellation.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use tempfile::TempDir;
use uuid::Uuid;

use mm::adapters::clock::FixedClock;
use mm::adapters::config::ConfigLoader;
use mm::adapters::environment::EnvironmentIndexer;
use mm::adapters::runtime::FakeRuntime;
use mm::adapters::stack::FilesystemStackArtifactStore;
use mm::adapters::target::FilesystemTargetStore;
use mm::app::ports::{
    RuntimeJobDriver, RuntimeOperationAuthorizer, SchedulerRepository, SecretMaterialResolver,
    StackArtifactStore, TargetStore,
};
use mm::app::{
    Application, JobSchedulerDependencies, JobSchedulerService, StackManager, WorkspaceSelection,
};
use mm::domain::config::{
    DurationSpec, JobConfig, RetryPolicyConfig, ScheduleConcurrency, ScheduleConfig,
};
use mm::domain::error::MmError;
use mm::domain::observability::{ApprovalContext, AuthorizationRequest};
use mm::domain::runtime::{RuntimeCapability, RuntimeKind, RuntimeProbe};
use mm::domain::scheduler::{
    ClaimedJobRun, ClaimedSchedule, DesiredDefinitions, JobAuditRecord, JobLaunch, JobRun,
    JobRunCompletion, JobRunState, JobTrigger, NewJobRun, ReconciliationCommit, RuntimeJobState,
    ScheduleRecord, ScheduleTickCommit, StoredDefinitionHashes, runtime_job_name,
};
use mm::domain::secret::{
    SecretMaterial, SecretMetadata, SecretResolution, SecretScope, SecretStatus,
};
use mm::domain::target::{TargetDefinition, TargetId};
use mm::domain::workspace::ProjectId;

const NOW: i64 = 1_800_000_000_000;

fn state_error(message: impl Into<String>) -> MmError {
    MmError::State {
        message: message.into(),
    }
}

#[derive(Default)]
struct RepositoryState {
    events: Vec<String>,
    hashes: StoredDefinitionHashes,
    runs: BTreeMap<Uuid, JobRun>,
    schedule_claims: VecDeque<ClaimedSchedule>,
    run_claims: VecDeque<ClaimedJobRun>,
}

#[derive(Default)]
struct FakeRepository {
    state: Mutex<RepositoryState>,
    ordered: Arc<Mutex<Vec<String>>>,
}

impl FakeRepository {
    fn with_log(ordered: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            state: Mutex::new(RepositoryState::default()),
            ordered,
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, RepositoryState>, MmError> {
        self.state
            .lock()
            .map_err(|_| state_error("scheduler repository test lock was poisoned"))
    }

    fn record(&self, event: impl Into<String>) -> Result<(), MmError> {
        let event = event.into();
        self.lock()?.events.push(event.clone());
        self.ordered
            .lock()
            .map_err(|_| state_error("scheduler event test lock was poisoned"))?
            .push(event);
        Ok(())
    }

    fn events(&self) -> Result<Vec<String>, MmError> {
        Ok(self.lock()?.events.clone())
    }

    fn clear_events(&self) -> Result<(), MmError> {
        self.lock()?.events.clear();
        self.ordered
            .lock()
            .map_err(|_| state_error("scheduler event test lock was poisoned"))?
            .clear();
        Ok(())
    }

    fn put_run(&self, run: JobRun) -> Result<(), MmError> {
        self.lock()?.runs.insert(run.id, run);
        Ok(())
    }

    fn queue_schedule(&self, claim: ClaimedSchedule) -> Result<(), MmError> {
        self.lock()?.schedule_claims.push_back(claim);
        Ok(())
    }

    fn queue_run(&self, claim: ClaimedJobRun) -> Result<(), MmError> {
        self.lock()?.runs.insert(claim.run.id, claim.run.clone());
        self.lock()?.run_claims.push_back(claim);
        Ok(())
    }

    fn updated_run(&self, id: Uuid, update: impl FnOnce(&mut JobRun)) -> Result<JobRun, MmError> {
        let mut state = self.lock()?;
        let run = state
            .runs
            .get_mut(&id)
            .ok_or_else(|| state_error(format!("test run '{id}' is absent")))?;
        update(run);
        Ok(run.clone())
    }
}

impl SchedulerRepository for FakeRepository {
    fn definition_hashes(
        &self,
        _project_id: ProjectId,
        _environment: &str,
    ) -> Result<StoredDefinitionHashes, MmError> {
        self.record("repo:definition_hashes")?;
        Ok(self.lock()?.hashes.clone())
    }

    fn reconcile(&self, _commit: &ReconciliationCommit) -> Result<(), MmError> {
        self.record("repo:reconcile")
    }

    fn list_jobs(
        &self,
        _project_id: ProjectId,
        _environment: &str,
    ) -> Result<DesiredDefinitions, MmError> {
        self.record("repo:list_jobs")?;
        Ok(DesiredDefinitions {
            jobs: BTreeMap::from([("task".to_owned(), job())]),
            schedules: BTreeMap::new(),
        })
    }

    fn list_schedules(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Vec<ScheduleRecord>, MmError> {
        self.record("repo:list_schedules")?;
        Ok(vec![schedule_record(project_id, environment, true)])
    }

    fn set_schedule_enabled(
        &self,
        project_id: ProjectId,
        environment: &str,
        _name: &str,
        enabled: bool,
        next_due_at_unix_ms: Option<i64>,
        _audit: &JobAuditRecord,
    ) -> Result<ScheduleRecord, MmError> {
        self.record(format!(
            "repo:set_schedule:{enabled}:{next_due_at_unix_ms:?}"
        ))?;
        let mut record = schedule_record(project_id, environment, enabled);
        record.next_due_at_unix_ms = next_due_at_unix_ms;
        Ok(record)
    }

    fn insert_run(&self, run: &NewJobRun, _audit: &JobAuditRecord) -> Result<JobRun, MmError> {
        self.record(format!("repo:insert:{:?}", run.trigger))?;
        let inserted = run_from_new(run);
        self.lock()?.runs.insert(inserted.id, inserted.clone());
        Ok(inserted)
    }

    fn run(&self, run_id: Uuid) -> Result<Option<JobRun>, MmError> {
        self.record("repo:run")?;
        Ok(self.lock()?.runs.get(&run_id).cloned())
    }

    fn history(
        &self,
        _project_id: ProjectId,
        _environment: &str,
        _job: Option<&str>,
        _schedule: Option<&str>,
        limit: usize,
    ) -> Result<Vec<JobRun>, MmError> {
        self.record(format!("repo:history:{limit}"))?;
        Ok(self.lock()?.runs.values().take(limit).cloned().collect())
    }

    fn request_cancel(&self, run_id: Uuid, _audit: &JobAuditRecord) -> Result<JobRun, MmError> {
        self.record("repo:request_cancel")?;
        self.updated_run(run_id, |run| {
            run.cancellation_requested = true;
            run.state = JobRunState::Cancelling;
        })
    }

    fn claim_due_schedules(
        &self,
        _project_id: ProjectId,
        _environment: &str,
        _owner: Uuid,
        _now_unix_ms: i64,
        _lease: Duration,
        _limit: usize,
    ) -> Result<Vec<ClaimedSchedule>, MmError> {
        self.record("repo:claim_schedules")?;
        Ok(self.lock()?.schedule_claims.drain(..).collect())
    }

    fn commit_schedule_tick(&self, commit: &ScheduleTickCommit) -> Result<Vec<JobRun>, MmError> {
        self.record("repo:commit_tick")?;
        Ok(commit
            .due_at_unix_ms
            .iter()
            .enumerate()
            .map(|(index, due)| scheduled_run(&commit.claim, *due, index as u128))
            .collect())
    }

    fn claim_runnable_run(
        &self,
        _project_id: ProjectId,
        _environment: &str,
        _owner: Uuid,
        _now_unix_ms: i64,
        _lease: Duration,
    ) -> Result<Option<ClaimedJobRun>, MmError> {
        self.record("repo:claim_run")?;
        Ok(self.lock()?.run_claims.pop_front())
    }

    fn claim_run(
        &self,
        run_id: Uuid,
        _owner: Uuid,
        _now_unix_ms: i64,
        _lease: Duration,
    ) -> Result<Option<ClaimedJobRun>, MmError> {
        self.record("repo:claim_run_by_id")?;
        let mut state = self.lock()?;
        let position = state
            .run_claims
            .iter()
            .position(|claim| claim.run.id == run_id);
        let claim = position.and_then(|position| state.run_claims.remove(position));
        Ok(claim.map(|mut claim| {
            if let Some(run) = state.runs.get(&run_id) {
                claim.run = run.clone();
            }
            claim
        }))
    }

    fn mark_run_running(
        &self,
        claim: &ClaimedJobRun,
        runtime_id: &str,
        started_at_unix_ms: i64,
    ) -> Result<JobRun, MmError> {
        self.record("repo:mark_running")?;
        self.updated_run(claim.run.id, |run| {
            run.state = JobRunState::Running;
            run.runtime_id = Some(runtime_id.to_owned());
            run.started_at_unix_ms = Some(started_at_unix_ms);
        })
    }

    fn complete_run(&self, completion: &JobRunCompletion) -> Result<JobRun, MmError> {
        self.record(format!("repo:complete:{:?}", completion.state))?;
        self.updated_run(completion.run_id, |run| {
            run.state = completion.state;
            run.finished_at_unix_ms = Some(completion.finished_at_unix_ms);
            run.exit_code = completion.exit_code;
            run.error_code.clone_from(&completion.error_code);
            run.output.clone_from(&completion.output);
            run.output_truncated = completion.output_truncated;
        })
    }

    fn schedule_retry(
        &self,
        claim: &ClaimedJobRun,
        next_attempt_at_unix_ms: i64,
        error_code: &str,
    ) -> Result<JobRun, MmError> {
        self.record(format!("repo:retry:{error_code}"))?;
        self.updated_run(claim.run.id, |run| {
            run.state = JobRunState::RetryWait;
            run.next_attempt_at_unix_ms = Some(next_attempt_at_unix_ms);
            run.error_code = Some(error_code.to_owned());
        })
    }

    fn mark_run_unknown(
        &self,
        claim: &ClaimedJobRun,
        diagnostic_code: &str,
    ) -> Result<JobRun, MmError> {
        self.record(format!("repo:unknown:{diagnostic_code}"))?;
        self.updated_run(claim.run.id, |run| run.state = JobRunState::Unknown)
    }

    fn append_job_audit(&self, _audit: &JobAuditRecord) -> Result<(), MmError> {
        self.record("repo:audit")
    }

    fn prune_history(
        &self,
        _project_id: ProjectId,
        _environment: &str,
        _job: &str,
        retain: usize,
    ) -> Result<usize, MmError> {
        self.record(format!("repo:prune:{retain}"))?;
        Ok(0)
    }
}

struct FakeJobRuntime {
    observations: Mutex<VecDeque<Result<RuntimeJobState, MmError>>>,
    refusals: Mutex<VecDeque<MmError>>,
    fail_cancel: AtomicBool,
    ordered: Arc<Mutex<Vec<String>>>,
}

impl FakeJobRuntime {
    fn new(ordered: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            observations: Mutex::new(VecDeque::new()),
            refusals: Mutex::new(VecDeque::new()),
            fail_cancel: AtomicBool::new(false),
            ordered,
        }
    }

    fn record(&self, event: &str) -> Result<(), MmError> {
        self.ordered
            .lock()
            .map_err(|_| state_error("runtime event test lock was poisoned"))?
            .push(event.to_owned());
        Ok(())
    }

    fn refuse(&self, error: MmError) -> Result<(), MmError> {
        self.refusals
            .lock()
            .map_err(|_| state_error("runtime refusal test lock was poisoned"))?
            .push_back(error);
        Ok(())
    }

    fn observe(&self, state: Result<RuntimeJobState, MmError>) -> Result<(), MmError> {
        self.observations
            .lock()
            .map_err(|_| state_error("runtime observation test lock was poisoned"))?
            .push_back(state);
        Ok(())
    }
}

#[async_trait]
impl RuntimeJobDriver for FakeJobRuntime {
    async fn launch(&self, launch: JobLaunch) -> Result<String, MmError> {
        self.record("runtime:launch")?;
        assert!(
            launch
                .environment
                .keys()
                .any(|key| key == "MM_JOB_FENCE_TOKEN")
        );
        assert_eq!(launch.job, "task");
        let refused = self
            .refusals
            .lock()
            .map_err(|_| state_error("runtime refusal test lock was poisoned"))?
            .pop_front();
        refused.map_or_else(|| Ok("runtime-id".to_owned()), Err)
    }

    async fn inspect(
        &self,
        _target: &mm::domain::runtime::RuntimeTarget,
        _runtime_name: &str,
        _runtime_id: Option<&str>,
        _output_limit_bytes: usize,
        _redactions: Vec<zeroize::Zeroizing<String>>,
    ) -> Result<RuntimeJobState, MmError> {
        self.record("runtime:inspect")?;
        self.observations
            .lock()
            .map_err(|_| state_error("runtime observation test lock was poisoned"))?
            .pop_front()
            .ok_or_else(|| state_error("test runtime has no queued observation"))?
    }

    async fn cancel(
        &self,
        _target: &mm::domain::runtime::RuntimeTarget,
        _runtime_name: &str,
        _runtime_id: Option<&str>,
    ) -> Result<(), MmError> {
        self.record("runtime:cancel")?;
        if self.fail_cancel.load(Ordering::SeqCst) {
            Err(state_error("injected cancel failure"))
        } else {
            Ok(())
        }
    }

    async fn cleanup(
        &self,
        _target: &mm::domain::runtime::RuntimeTarget,
        _runtime_name: &str,
        _runtime_id: Option<&str>,
    ) -> Result<(), MmError> {
        self.record("runtime:cleanup")
    }
}

struct FakeSecrets {
    resolution: Mutex<Option<SecretResolution>>,
}

#[async_trait]
impl SecretMaterialResolver for FakeSecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        _reference: &mm::domain::secret::SecretReference,
    ) -> Result<SecretResolution, MmError> {
        self.resolution
            .lock()
            .map_err(|_| state_error("secret resolution test lock was poisoned"))?
            .take()
            .ok_or_else(|| state_error("test secret resolution is absent"))
    }
}

struct FakeAuthorizer {
    denied: AtomicBool,
    ordered: Arc<Mutex<Vec<String>>>,
}

impl RuntimeOperationAuthorizer for FakeAuthorizer {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), MmError> {
        self.ordered
            .lock()
            .map_err(|_| state_error("authorization event test lock was poisoned"))?
            .push(format!("authorize:{:?}", request.operation));
        if self.denied.load(Ordering::SeqCst) {
            Err(MmError::Security {
                message: "injected authorization denial".to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

struct Setup {
    _project: TempDir,
    _home: TempDir,
    service: JobSchedulerService,
    repository: Arc<FakeRepository>,
    runtime: Arc<FakeJobRuntime>,
    authorizer: Arc<FakeAuthorizer>,
    ordered: Arc<Mutex<Vec<String>>>,
    selection: WorkspaceSelection,
}

fn setup(one_shot_jobs: bool) -> Result<Setup, Box<dyn std::error::Error>> {
    let project = crate::temp_dirs::tempdir()?;
    let home = crate::temp_dirs::tempdir()?;
    fs::create_dir(project.path().join(".config"))?;
    fs::create_dir(project.path().join("app"))?;
    fs::write(
        project.path().join(".config/mm.yaml"),
        r#"schema-version: 1
project:
  id: 00000000-0000-0000-0000-000000000001
  name: scheduler-test
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
      worker:
        stack: app
        role: job
jobs:
  task:
    module: app
    service: worker
    command: [run, task]
    timeout: 10s
    retry:
      attempts: 2
      backoff: 1s
      max-backoff: 2s
    output-limit-bytes: 1024
    history-limit: 5
  production-only:
    module: app
    service: worker
    command: [run, production]
    environment: production
schedules:
  every-minute:
    job: task
    cron: "* * * * *"
    timezone: UTC
    jitter: 0s
environments:
  local:
    bindings: {}
  production:
    bindings: {}
"#,
    )?;
    fs::write(
        project.path().join("compose.yaml"),
        "services:\n  worker:\n    image: example/worker\n",
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
    let mut capabilities = vec![RuntimeCapability::RenderCompose];
    if one_shot_jobs {
        capabilities.push(RuntimeCapability::OneShotJobs);
    }
    let runtime_driver = Arc::new(FakeRuntime {
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
        rendered: BTreeMap::new(),
        observed: BTreeMap::new(),
    });
    let clock = Arc::new(FixedClock { unix_ms: NOW });
    let application = Application::runtime_inspection(
        runtime_driver,
        clock.clone(),
        documents,
        Arc::new(EnvironmentIndexer),
        stacks,
    );
    let ordered = Arc::new(Mutex::new(Vec::new()));
    let repository = Arc::new(FakeRepository::with_log(ordered.clone()));
    let runtime = Arc::new(FakeJobRuntime::new(ordered.clone()));
    let authorizer = Arc::new(FakeAuthorizer {
        denied: AtomicBool::new(false),
        ordered: ordered.clone(),
    });
    let secrets = Arc::new(FakeSecrets {
        resolution: Mutex::new(Some(SecretResolution {
            metadata: SecretMetadata {
                status: SecretStatus::Set,
                revision: Some("1".to_owned()),
                created_at_unix_ms: Some(NOW),
                candidate_revision: None,
            },
            material: Some(SecretMaterial::new("scheduler-secret-sentinel".to_owned())?),
        })),
    });
    let service = JobSchedulerService::new(JobSchedulerDependencies {
        application,
        repository: repository.clone(),
        runtime: runtime.clone(),
        secrets,
        authorizer: authorizer.clone(),
        clock,
    });
    let selection = WorkspaceSelection::new(project.path(), Some("local".to_owned()));
    Ok(Setup {
        _project: project,
        _home: home,
        service,
        repository,
        runtime,
        authorizer,
        ordered,
        selection,
    })
}

fn approval() -> ApprovalContext {
    ApprovalContext {
        principal: "scheduler-test".to_owned(),
        source: "unit-test".to_owned(),
        approved_high_risk: true,
    }
}

fn job() -> JobConfig {
    JobConfig {
        module: "app".to_owned(),
        service: "worker".to_owned(),
        command: vec!["run".to_owned(), "task".to_owned()],
        environments: None,
        run: None,
        timeout: DurationSpec("10s".to_owned()),
        retry: RetryPolicyConfig {
            attempts: 2,
            backoff: DurationSpec("1s".to_owned()),
            max_backoff: DurationSpec("2s".to_owned()),
        },
        output_limit_bytes: 1024,
        history_limit: 5,
        placement: None,
        user: None,
        placed_elsewhere: None,
        development: None,
        off_locally: None,
    }
}

fn schedule() -> ScheduleConfig {
    ScheduleConfig {
        job: "task".to_owned(),
        cron: "* * * * *".to_owned(),
        timezone: "UTC".to_owned(),
        enabled: true,
        starts_at: None,
        ends_at: None,
        jitter: DurationSpec("0s".to_owned()),
        concurrency: ScheduleConcurrency::Forbid,
        misfire: mm::domain::config::MisfirePolicy::RunOnce,
        retry: None,
        timeout: None,
    }
}

fn schedule_record(project_id: ProjectId, environment: &str, enabled: bool) -> ScheduleRecord {
    ScheduleRecord {
        name: "every-minute".to_owned(),
        project_id,
        environment: environment.to_owned(),
        job: "task".to_owned(),
        enabled,
        retired: false,
        next_due_at_unix_ms: None,
        definition_hash: "definition".to_owned(),
        generation: 1,
    }
}

fn run_from_new(run: &NewJobRun) -> JobRun {
    JobRun {
        id: run.id,
        project_id: run.project_id,
        environment: run.environment.clone(),
        job: run.job.clone(),
        schedule: run.schedule.clone(),
        trigger: run.trigger,
        state: JobRunState::Queued,
        attempt: 1,
        maximum_attempts: run.job_definition.retry.attempts,
        fence_token: 1,
        runtime_name: runtime_job_name(run.project_id, run.id),
        runtime_id: None,
        requested_at_unix_ms: run.requested_at_unix_ms,
        scheduled_for_unix_ms: run.scheduled_for_unix_ms,
        started_at_unix_ms: None,
        finished_at_unix_ms: None,
        next_attempt_at_unix_ms: None,
        exit_code: None,
        error_code: None,
        output: String::new(),
        output_truncated: false,
        cancellation_requested: false,
        principal: run.principal.clone(),
        source: run.source.clone(),
        node: None,
    }
}

fn claimed_run(sequence: u128, state: JobRunState, attempt: u16) -> ClaimedJobRun {
    let id = Uuid::from_u128(100 + sequence);
    ClaimedJobRun {
        run: JobRun {
            id,
            project_id: ProjectId(Uuid::from_u128(1)),
            environment: "local".to_owned(),
            job: "task".to_owned(),
            schedule: Some("every-minute".to_owned()),
            trigger: JobTrigger::Schedule,
            state,
            attempt,
            maximum_attempts: 2,
            fence_token: sequence as u64 + 1,
            runtime_name: runtime_job_name(ProjectId(Uuid::from_u128(1)), id),
            runtime_id: Some(format!("runtime-{sequence}")),
            requested_at_unix_ms: NOW - 20_000,
            scheduled_for_unix_ms: Some(NOW - 20_000),
            started_at_unix_ms: Some(NOW - 11_000),
            finished_at_unix_ms: None,
            next_attempt_at_unix_ms: None,
            exit_code: None,
            error_code: None,
            output: String::new(),
            output_truncated: false,
            cancellation_requested: false,
            principal: "controller".to_owned(),
            source: "test".to_owned(),
            node: None,
        },
        job_definition: job(),
        schedule_definition: Some(schedule()),
        lease_owner: Uuid::from_u128(500 + sequence),
    }
}

fn scheduled_run(claim: &ClaimedSchedule, due: i64, sequence: u128) -> JobRun {
    let id = Uuid::from_u128(1_000 + sequence);
    JobRun {
        id,
        project_id: claim.project_id,
        environment: claim.environment.clone(),
        job: claim.definition.job.clone(),
        schedule: Some(claim.name.clone()),
        trigger: JobTrigger::Schedule,
        state: JobRunState::Queued,
        attempt: 1,
        maximum_attempts: claim.job_definition.retry.attempts,
        fence_token: claim.fence_token,
        runtime_name: runtime_job_name(claim.project_id, id),
        runtime_id: None,
        requested_at_unix_ms: due,
        scheduled_for_unix_ms: Some(due),
        started_at_unix_ms: None,
        finished_at_unix_ms: None,
        next_attempt_at_unix_ms: None,
        exit_code: None,
        error_code: None,
        output: String::new(),
        output_truncated: false,
        cancellation_requested: false,
        principal: "controller".to_owned(),
        source: "test".to_owned(),
        node: None,
    }
}

fn ordered(setup: &Setup) -> Result<Vec<String>, MmError> {
    setup
        .ordered
        .lock()
        .map(|events| events.clone())
        .map_err(|_| state_error("scheduler event test lock was poisoned"))
}

#[path = "job_scheduler/cases.rs"]
mod cases;
