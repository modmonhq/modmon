use super::*;
use mm::adapters::state::SqliteStateStore;
use mm::domain::scheduler::JobRunRequest;

fn launching(sequence: u128) -> ClaimedJobRun {
    let mut claim = claimed_run(sequence, JobRunState::Launching, 1);
    claim.run.runtime_id = None;
    claim.run.started_at_unix_ms = None;
    claim
}

fn stored_run(setup: &Setup, id: u128) -> Result<JobRun, Box<dyn std::error::Error>> {
    Ok(setup
        .repository
        .lock()?
        .runs
        .get(&Uuid::from_u128(id))
        .cloned()
        .ok_or("the run is gone")?)
}

#[tokio::test]
async fn a_job_pulls_its_service_image_before_its_container_launches()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    setup.repository.queue_run(launching(20))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    setup.service.cycle(&setup.selection, &approval()).await?;
    let events = ordered(&setup)?;
    let pull = events
        .iter()
        .position(|event| event == "runtime:pull:worker")
        .ok_or("the image was not pulled")?;
    let launch = events
        .iter()
        .position(|event| event == "runtime:launch")
        .ok_or("the container was not launched")?;
    assert!(pull < launch, "{events:?}");
    assert_eq!(stored_run(&setup, 120)?.state, JobRunState::Running);
    Ok(())
}

#[tokio::test]
async fn a_pull_past_its_limit_ends_the_attempt_and_the_job_is_retried()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    setup.repository.queue_run(launching(21))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    setup.runtime.refuse_pull(MmError::Timeout {
        message: "pulling image example/worker for service 'worker' of stack 'app' did not finish within 20m (deploy.pull-timeout)".to_owned(),
    })?;
    setup.service.cycle(&setup.selection, &approval()).await?;
    let run = stored_run(&setup, 121)?;
    assert_eq!(run.state, JobRunState::RetryWait);
    assert_eq!(run.error_code.as_deref(), Some("MM_TIMEOUT"));
    let events = ordered(&setup)?;
    assert!(
        events.iter().any(|event| event == "repo:retry:MM_TIMEOUT"),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|event| event == "runtime:launch"),
        "{events:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_controller_moves_other_runs_on_while_one_pulls() -> Result<(), Box<dyn std::error::Error>>
{
    let setup = setup(true)?;
    let gate = setup.runtime.gate_pulls()?;
    setup.repository.queue_run(launching(22))?;
    setup
        .repository
        .queue_run(claimed_run(23, JobRunState::Running, 1))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    setup.runtime.observe(Ok(RuntimeJobState::Running))?;
    let controller = setup.service.clone().with_background_launches();
    tokio::time::timeout(
        Duration::from_secs(10),
        controller.cycle(&setup.selection, &approval()),
    )
    .await
    .map_err(|_| "the look waited for the pull")??;
    let events = ordered(&setup)?;
    assert!(
        events
            .iter()
            .any(|event| event == "repo:retry:MM_JOB_TIMEOUT"),
        "the run past its timeout was stopped while the other pulled: {events:?}"
    );
    assert!(
        !events.iter().any(|event| event == "runtime:launch"),
        "{events:?}"
    );

    gate.add_permits(1);
    tokio::time::timeout(Duration::from_secs(10), async {
        while stored_run(&setup, 122).ok().map(|run| run.state) != Some(JobRunState::Running) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| "the pulled run never started")?;
    Ok(())
}

/// Unix time that moves with Tokio's clock, which a paused test advances.
struct Ticking(tokio::time::Instant);

impl Clock for Ticking {
    fn now_unix_ms(&self) -> i64 {
        NOW + i64::try_from(self.0.elapsed().as_millis()).unwrap_or(i64::MAX)
    }
}

#[tokio::test(start_paused = true)]
async fn a_pull_longer_than_a_lease_keeps_the_claim_and_the_job_launches_once()
-> Result<(), Box<dyn std::error::Error>> {
    let state = crate::temp_dirs::tempdir()?;
    let store: Arc<dyn SchedulerRepository> =
        Arc::new(SqliteStateStore::open(&state.path().join("state.sqlite3"))?);
    let setup = setup_with(
        true,
        Arc::new(Ticking(tokio::time::Instant::now())),
        Some(store),
    )?;
    let plan = setup
        .service
        .plan_reconciliation(&setup.selection, &approval())?;
    setup
        .service
        .apply_reconciliation(&setup.selection, &approval(), &plan.plan_hash)?;
    setup.runtime.follows_launches.store(true, Ordering::SeqCst);
    let gate = setup.runtime.gate_pulls()?;
    let id = Uuid::from_u128(124);
    let request = JobRunRequest {
        run: Some(id),
        ..JobRunRequest::manual("task", false)
    };

    let asked = approval();
    let first = setup.service.run_job(&setup.selection, &request, &asked);
    let second = async {
        tokio::time::sleep(Duration::from_secs(45)).await;
        setup
            .service
            .follow_run(&setup.selection, id, false, &approval())
            .await
    };
    let slow_registry = async {
        tokio::time::sleep(Duration::from_secs(60)).await;
        gate.add_permits(2);
    };
    let (first, second, ()) = tokio::join!(first, second, slow_registry);
    assert_eq!(first?.state, JobRunState::Running);
    assert_eq!(second?.state, JobRunState::Running);
    let events = ordered(&setup)?;
    assert_eq!(
        events
            .iter()
            .filter(|event| *event == "runtime:launch")
            .count(),
        1,
        "{events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("runtime:pull"))
            .count(),
        1,
        "{events:?}"
    );
    Ok(())
}

fn no_effect_on(events: &[String]) -> bool {
    !events.iter().any(|event| {
        event == "runtime:launch"
            || event == "repo:mark_running"
            || event.starts_with("repo:complete")
            || event.starts_with("repo:retry")
    })
}

#[tokio::test(start_paused = true)]
async fn a_cancel_during_the_pull_launches_nothing_and_records_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let asked = approval();
    let setup = setup(true)?;
    let _gate = setup.runtime.gate_pulls()?;
    setup.repository.queue_run(launching(25))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let cancel = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        setup.repository.updated_run(Uuid::from_u128(125), |run| {
            run.cancellation_requested = true;
            run.state = JobRunState::Cancelling;
        })
    };
    let (looked, cancelled) = tokio::join!(setup.service.cycle(&setup.selection, &asked), cancel);
    looked?;
    cancelled?;
    let events = ordered(&setup)?;
    assert!(
        events.iter().any(|event| event == "runtime:pull:worker"),
        "{events:?}"
    );
    assert!(no_effect_on(&events), "{events:?}");
    assert_eq!(stored_run(&setup, 125)?.state, JobRunState::Cancelling);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_store_that_misses_a_renewal_keeps_the_pull_and_one_that_stays_silent_gives_it_up()
-> Result<(), Box<dyn std::error::Error>> {
    let asked = approval();
    let setup = setup_with(true, Arc::new(Ticking(tokio::time::Instant::now())), None)?;
    let gate = setup.runtime.gate_pulls()?;
    setup.repository.queue_run(launching(26))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let busy_once = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        setup
            .repository
            .lock()
            .map(|mut state| state.unanswered_renewals = 1)
    };
    let slow_registry = async {
        tokio::time::sleep(Duration::from_secs(25)).await;
        gate.add_permits(1);
    };
    let (looked, busy, ()) = tokio::join!(
        setup.service.cycle(&setup.selection, &asked),
        busy_once,
        slow_registry
    );
    looked?;
    busy?;
    assert_eq!(stored_run(&setup, 126)?.state, JobRunState::Running);

    let setup = setup_with(true, Arc::new(Ticking(tokio::time::Instant::now())), None)?;
    let _gate = setup.runtime.gate_pulls()?;
    setup.repository.queue_run(launching(27))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let silent = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        setup
            .repository
            .lock()
            .map(|mut state| state.unanswered_renewals = usize::MAX)
    };
    let (looked, silent) = tokio::join!(setup.service.cycle(&setup.selection, &asked), silent);
    looked?;
    silent?;
    let events = ordered(&setup)?;
    assert!(no_effect_on(&events), "{events:?}");
    assert_eq!(stored_run(&setup, 127)?.state, JobRunState::Launching);
    Ok(())
}

async fn settled(setup: &Setup, id: u128, state: JobRunState) -> Result<JobRun, String> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(run) = stored_run(setup, id)
                && run.state == state
            {
                return run;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| format!("run {id} never became {state:?}"))
}

#[tokio::test]
async fn a_background_start_records_every_way_it_ends() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    let controller = setup.service.clone().with_background_launches();
    setup.repository.queue_run(launching(28))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let refused = MmError::ProcessTimeout {
        program: "docker (launch one-shot job)".to_owned(),
        seconds: 120,
    };
    let code = refused.code();
    setup.runtime.refuse(refused)?;
    controller.cycle(&setup.selection, &approval()).await?;
    let retried = settled(&setup, 128, JobRunState::RetryWait).await?;
    assert_eq!(retried.error_code.as_deref(), Some(code));

    let mut last = launching(29);
    last.run.attempt = 2;
    setup.repository.queue_run(last)?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    setup.runtime.pull_breaks.store(true, Ordering::SeqCst);
    controller.cycle(&setup.selection, &approval()).await?;
    let failed = settled(&setup, 129, JobRunState::Failed).await?;
    assert_eq!(failed.error_code.as_deref(), Some("MM_RUNTIME_LAUNCH"));
    assert!(
        failed.output.contains("stopped inside mm"),
        "{}",
        failed.output
    );
    Ok(())
}
