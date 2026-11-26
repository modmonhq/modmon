use super::*;
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{DeploymentJobs, JobLaunchTargets, PreparedJobTarget};
use mm::domain::scheduler::{JobRunRequest, JobRunsRequest};

const VALUES_CHANGED: &str = "the values or secrets of 'local' changed since its last deployment, and a job runs with what its service runs. Deploy the change first: `mm deploy plan --environment local`";

#[derive(Default)]
struct ValuesChanged {
    prepared: AtomicUsize,
}

#[async_trait]
impl JobLaunchTargets for ValuesChanged {
    async fn prepare(
        &self,
        _selection: &WorkspaceSelection,
        _stack: &str,
        _run: Uuid,
    ) -> Result<Option<PreparedJobTarget>, MmError> {
        self.prepared.fetch_add(1, Ordering::SeqCst);
        Err(state_error(VALUES_CHANGED))
    }
}

struct Stuck {
    _state: TempDir,
    setup: Setup,
    store: Arc<SqliteStateStore>,
    targets: Arc<ValuesChanged>,
    service: JobSchedulerService,
}

impl Stuck {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let state = crate::temp_dirs::tempdir()?;
        let store = Arc::new(SqliteStateStore::open(&state.path().join("state.sqlite3"))?);
        let setup = setup_with(
            true,
            Arc::new(FixedClock { unix_ms: NOW }),
            Some(store.clone()),
        )?;
        let plan = setup
            .service
            .plan_reconciliation(&setup.selection, &approval())?;
        setup
            .service
            .apply_reconciliation(&setup.selection, &approval(), &plan.plan_hash)?;
        let targets = Arc::new(ValuesChanged::default());
        let service = setup.service.clone().with_launch_targets(targets.clone());
        Ok(Self {
            _state: state,
            setup,
            store,
            targets,
            service,
        })
    }

    fn launching_left_behind(&self) -> Result<Uuid, Box<dyn std::error::Error>> {
        let run = self
            .setup
            .service
            .enqueue_job(&self.setup.selection, "task", &approval())?;
        self.store
            .claim_run(
                run.id,
                Uuid::new_v4(),
                NOW - 600_000,
                Duration::from_secs(30),
            )?
            .ok_or("the run was not claimed")?;
        assert_eq!(self.state(run.id)?, JobRunState::Launching);
        Ok(run.id)
    }

    fn run(&self, id: Uuid) -> Result<JobRun, Box<dyn std::error::Error>> {
        Ok(self.store.run(id)?.ok_or("the run is gone")?)
    }

    fn state(&self, id: Uuid) -> Result<JobRunState, Box<dyn std::error::Error>> {
        Ok(self.run(id)?.state)
    }

    fn prepared(&self) -> usize {
        self.targets.prepared.load(Ordering::SeqCst)
    }
}

fn assert_refused(run: &JobRun) {
    assert_eq!(run.state, JobRunState::Failed);
    assert_eq!(run.error_code.as_deref(), Some("MM_STATE"));
    assert!(
        run.output.contains("changed since its last deployment"),
        "{}",
        run.output
    );
}

fn stack_target() -> mm::domain::runtime::RuntimeTarget {
    mm::domain::runtime::RuntimeTarget {
        root: "/plan".into(),
        project_directory: "/plan".into(),
        target: None,
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        stack_name: "app".to_owned(),
        project_name: "scheduler-test-local-app".to_owned(),
        artifact_hash: "plan".to_owned(),
        compose_files: vec!["/plan/compose.yaml".into()],
        user_compose_file_count: 1,
        interpolation_environment: mm::domain::runtime::RuntimeEnvironment::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    }
}

#[tokio::test]
async fn a_read_of_the_runs_ends_a_run_that_can_no_longer_be_prepared()
-> Result<(), Box<dyn std::error::Error>> {
    let stuck = Stuck::new()?;
    let left = stuck.launching_left_behind()?;
    stuck.setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let runs = stuck
        .service
        .runs(
            &stuck.setup.selection,
            &JobRunsRequest {
                job: None,
                run: None,
                limit: 10,
            },
            &approval(),
        )
        .await?;
    let read = runs
        .iter()
        .find(|run| run.id == left)
        .ok_or("the run was not listed")?;
    assert_refused(read);
    assert_refused(&stuck.run(left)?);
    assert_eq!(stuck.prepared(), 1);
    Ok(())
}

#[tokio::test]
async fn a_deployment_sweep_ends_runs_that_cannot_be_prepared_and_goes_on()
-> Result<(), Box<dyn std::error::Error>> {
    let stuck = Stuck::new()?;
    stuck.setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let earlier = JobRunRequest {
        trigger: JobTrigger::BeforeDeploy,
        deployment: Some("earlier-plan".to_owned()),
        ..JobRunRequest::manual("task", false)
    };
    let cut_off = stuck
        .setup
        .service
        .run_job(&stuck.setup.selection, &earlier, &approval())
        .await?;
    assert_eq!(cut_off.state, JobRunState::Running);
    let left = stuck.launching_left_behind()?;
    stuck.setup.runtime.observe(Ok(RuntimeJobState::Missing))?;

    let removed = stuck.service.sweep(&stuck.setup.selection, &[]).await?;

    assert!(removed.is_empty());
    assert_refused(&stuck.run(left)?);
    let cancelled = stuck.run(cut_off.id)?;
    assert_eq!(cancelled.state, JobRunState::Cancelled);
    assert_eq!(cancelled.error_code.as_deref(), Some("MM_CANCELLED"));
    assert_eq!(
        stuck.prepared(),
        1,
        "only the run that launches is prepared"
    );
    Ok(())
}

#[tokio::test]
async fn cancelling_a_run_that_can_no_longer_be_prepared_ends_it_at_once()
-> Result<(), Box<dyn std::error::Error>> {
    let stuck = Stuck::new()?;
    let left = stuck.launching_left_behind()?;
    let cancelled = stuck
        .service
        .cancel_run(&stuck.setup.selection, left, &approval())
        .await?;
    assert_eq!(cancelled.state, JobRunState::Cancelled);
    assert_eq!(stuck.state(left)?, JobRunState::Cancelled);
    assert_eq!(stuck.prepared(), 0);
    assert!(
        ordered(&stuck.setup)?
            .iter()
            .any(|event| event == "runtime:cancel")
    );
    Ok(())
}

#[tokio::test]
async fn a_manual_run_refused_at_launch_ends_failed() -> Result<(), Box<dyn std::error::Error>> {
    let stuck = Stuck::new()?;
    stuck.setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let id = Uuid::from_u128(72);
    let request = JobRunRequest {
        run: Some(id),
        ..JobRunRequest::manual("task", false)
    };
    let error = stuck
        .service
        .run_job(&stuck.setup.selection, &request, &approval())
        .await
        .err()
        .ok_or("a run that cannot be prepared started")?;
    assert_eq!(error.code(), "MM_STATE");
    assert!(error.to_string().contains("Deploy the change first"));
    assert_refused(&stuck.run(id)?);
    assert!(
        !ordered(&stuck.setup)?
            .iter()
            .any(|event| event == "runtime:launch")
    );
    Ok(())
}

#[tokio::test]
async fn a_deployment_runs_its_job_with_the_plans_own_files_after_the_values_changed()
-> Result<(), Box<dyn std::error::Error>> {
    let stuck = Stuck::new()?;
    let left = stuck.launching_left_behind()?;
    stuck.setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    stuck.service.sweep(&stuck.setup.selection, &[]).await?;
    assert_refused(&stuck.run(left)?);

    stuck.setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    stuck.setup.runtime.observe(Ok(RuntimeJobState::Exited {
        exit_code: 0,
        output: "migrated\n".to_owned(),
        truncated: false,
    }))?;
    let run = DeploymentJobs::run(
        &stuck.service,
        &stuck.setup.selection,
        "task",
        stack_target(),
        "the-plan-being-applied",
    )
    .await?;
    assert_eq!(run.state, JobRunState::Succeeded);
    assert_eq!(run.trigger, JobTrigger::BeforeDeploy);
    assert_eq!(stuck.prepared(), 1, "the plan's run is not prepared again");
    Ok(())
}
