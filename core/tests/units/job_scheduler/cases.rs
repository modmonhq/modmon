use super::*;
use mm::domain::scheduler::ReconciliationAction;

#[test]
fn reviewed_reconciliation_is_exact_and_stale_or_unauthorized_plans_have_no_effect()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    let plan = setup
        .service
        .plan_reconciliation(&setup.selection, &approval())?;
    assert_eq!(plan.schema, mm::domain::scheduler::JOB_PLAN_SCHEMA);
    assert!(
        plan.changes
            .iter()
            .any(|change| change.action == ReconciliationAction::CreateJob)
    );
    assert_eq!(plan.calculate_hash()?, plan.plan_hash);

    setup.repository.clear_events()?;
    assert!(matches!(
        setup
            .service
            .apply_reconciliation(&setup.selection, &approval(), "stale"),
        Err(MmError::Security { .. })
    ));
    assert!(
        !setup
            .repository
            .events()?
            .iter()
            .any(|event| event == "repo:reconcile")
    );

    setup.repository.clear_events()?;
    let applied =
        setup
            .service
            .apply_reconciliation(&setup.selection, &approval(), &plan.plan_hash)?;
    assert_eq!(applied.plan_hash, plan.plan_hash);
    assert!(
        setup
            .repository
            .events()?
            .iter()
            .any(|event| event == "repo:reconcile")
    );

    setup.repository.clear_events()?;
    setup.authorizer.denied.store(true, Ordering::SeqCst);
    assert!(matches!(
        setup
            .service
            .plan_reconciliation(&setup.selection, &approval()),
        Err(MmError::Security { .. })
    ));
    assert!(setup.repository.events()?.is_empty());
    Ok(())
}

#[test]
fn scheduler_queries_mutations_and_owner_checks_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    let approval = approval();
    assert_queries(&setup, &approval)?;

    assert!(
        setup
            .service
            .show_run(&setup.selection, Uuid::nil(), &approval)
            .is_err()
    );
    assert!(
        setup
            .service
            .enqueue_job(&setup.selection, "missing", &approval)
            .is_err()
    );
    let manual = setup
        .service
        .enqueue_job(&setup.selection, "task", &approval)?;
    assert_eq!(manual.trigger, JobTrigger::Manual);
    let scheduled = setup
        .service
        .run_schedule_now(&setup.selection, "every-minute", &approval)?;
    assert_eq!(scheduled.trigger, JobTrigger::RunNow);

    assert!(
        setup
            .service
            .retry_run(&setup.selection, manual.id, &approval)
            .is_err()
    );
    let mut terminal = manual.clone();
    terminal.id = Uuid::from_u128(77);
    terminal.state = JobRunState::Failed;
    setup.repository.put_run(terminal.clone())?;
    let retry = setup
        .service
        .retry_run(&setup.selection, terminal.id, &approval)?;
    assert_eq!(retry.trigger, JobTrigger::Retry);
    let cancelled = setup
        .service
        .cancel_run(&setup.selection, scheduled.id, &approval)?;
    assert_eq!(cancelled.state, JobRunState::Cancelling);

    let mut foreign = terminal;
    foreign.id = Uuid::from_u128(78);
    foreign.environment = "production".to_owned();
    setup.repository.put_run(foreign.clone())?;
    assert!(matches!(
        setup
            .service
            .show_run(&setup.selection, foreign.id, &approval),
        Err(MmError::Security { .. })
    ));

    let enabled =
        setup
            .service
            .set_schedule_enabled(&setup.selection, "every-minute", true, &approval)?;
    assert!(enabled.next_due_at_unix_ms.is_some());
    let disabled =
        setup
            .service
            .set_schedule_enabled(&setup.selection, "every-minute", false, &approval)?;
    assert_eq!(disabled.next_due_at_unix_ms, None);
    assert!(
        setup
            .service
            .set_schedule_enabled(&setup.selection, "missing", true, &approval)
            .is_err()
    );
    Ok(())
}

fn assert_queries(
    setup: &Setup,
    approval: &ApprovalContext,
) -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        setup
            .service
            .list_jobs(&setup.selection, approval)?
            .jobs
            .len(),
        1
    );
    assert_eq!(
        setup
            .service
            .list_schedules(&setup.selection, approval)?
            .len(),
        1
    );
    setup.service.history(
        &setup.selection,
        Some("task"),
        Some("every-minute"),
        usize::MAX,
        approval,
    )?;
    assert!(
        setup
            .repository
            .events()?
            .iter()
            .any(|event| event == "repo:history:1000")
    );
    Ok(())
}

#[tokio::test]
async fn cycle_reconciles_every_runtime_state_with_ordered_terminal_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    setup.repository.queue_schedule(ClaimedSchedule {
        project_id: ProjectId(Uuid::from_u128(1)),
        environment: "local".to_owned(),
        name: "every-minute".to_owned(),
        definition: schedule(),
        job_definition: job(),
        config_hash: "a".repeat(64),
        next_due_at_unix_ms: NOW - 60_000,
        lease_owner: Uuid::from_u128(10),
        fence_token: 1,
    })?;

    let mut cancelled = claimed_run(1, JobRunState::Cancelling, 1);
    cancelled.run.cancellation_requested = true;
    setup.repository.queue_run(cancelled)?;
    setup
        .repository
        .queue_run(claimed_run(2, JobRunState::Running, 2))?;
    setup.runtime.observe(Ok(RuntimeJobState::Exited {
        exit_code: 0,
        output: "bounded output".to_owned(),
        truncated: false,
    }))?;
    setup
        .repository
        .queue_run(claimed_run(3, JobRunState::Running, 1))?;
    setup.runtime.observe(Ok(RuntimeJobState::Exited {
        exit_code: 9,
        output: String::new(),
        truncated: false,
    }))?;
    setup
        .repository
        .queue_run(claimed_run(4, JobRunState::Running, 2))?;
    setup.runtime.observe(Ok(RuntimeJobState::Running))?;
    setup
        .repository
        .queue_run(claimed_run(5, JobRunState::Running, 1))?;
    setup.runtime.observe(Ok(RuntimeJobState::Unknown {
        diagnostic_code: "MM_RUNTIME_UNCERTAIN".to_owned(),
    }))?;
    setup
        .repository
        .queue_run(claimed_run(6, JobRunState::Running, 1))?;
    setup
        .runtime
        .observe(Err(state_error("runtime transport failed")))?;
    setup
        .repository
        .queue_run(claimed_run(7, JobRunState::Running, 2))?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;
    let mut queued = claimed_run(8, JobRunState::Queued, 1);
    queued.run.runtime_id = None;
    queued.run.started_at_unix_ms = None;
    setup.repository.queue_run(queued)?;
    setup.runtime.observe(Ok(RuntimeJobState::Missing))?;

    let report = setup.service.cycle(&setup.selection, &approval()).await?;
    assert_eq!(report.schedules_claimed, 1);
    assert_eq!(report.runs_enqueued, 1);
    assert_eq!(report.runs_processed, 8);
    assert_eq!(report.unknown_runs, 2);
    let events = ordered(&setup)?;
    assert!(events.windows(3).any(|events| events
        == [
            "runtime:cancel",
            "repo:complete:Cancelled",
            "runtime:cleanup"
        ]));
    assert!(
        events
            .windows(3)
            .any(|events| events == ["repo:complete:Succeeded", "runtime:cleanup", "repo:prune:5"])
    );
    assert!(
        events
            .windows(2)
            .any(|events| events == ["runtime:cleanup", "repo:retry:MM_JOB_NON_ZERO"])
    );
    assert!(
        events
            .windows(2)
            .any(|events| events == ["runtime:launch", "repo:mark_running"])
    );
    assert!(events.iter().any(|event| event == "repo:audit"));
    Ok(())
}

#[tokio::test]
async fn capability_and_runtime_failures_stop_before_later_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let unsupported = setup(false)?;
    assert!(matches!(
        unsupported
            .service
            .cycle(&unsupported.selection, &approval())
            .await,
        Err(MmError::UnsupportedCapability { .. })
    ));
    assert!(
        !unsupported
            .repository
            .events()?
            .iter()
            .any(|event| event == "repo:claim_schedules")
    );

    let failing = setup(true)?;
    let mut cancelling = claimed_run(20, JobRunState::Cancelling, 1);
    cancelling.run.cancellation_requested = true;
    failing.repository.queue_run(cancelling)?;
    failing.runtime.fail_cancel.store(true, Ordering::SeqCst);
    assert!(
        failing
            .service
            .cycle(&failing.selection, &approval())
            .await
            .is_err()
    );
    let events = ordered(&failing)?;
    let cancel = events
        .iter()
        .position(|event| event == "runtime:cancel")
        .ok_or("cancel effect was not recorded")?;
    assert!(
        events[cancel + 1..]
            .iter()
            .all(|event| !event.starts_with("repo:complete") && event != "runtime:cleanup")
    );
    Ok(())
}

#[tokio::test]
async fn a_last_attempt_that_fails_keeps_its_output() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup(true)?;
    setup
        .repository
        .queue_run(claimed_run(9, JobRunState::Running, 2))?;
    setup.runtime.observe(Ok(RuntimeJobState::Exited {
        exit_code: 3,
        output: "connecting\ncannot-migrate\n".to_owned(),
        truncated: true,
    }))?;
    setup.service.cycle(&setup.selection, &approval()).await?;
    let run = setup
        .repository
        .lock()?
        .runs
        .get(&Uuid::from_u128(109))
        .cloned()
        .ok_or("the run is gone")?;
    assert_eq!(run.state, JobRunState::Failed);
    assert_eq!(run.exit_code, Some(3));
    assert_eq!(run.output, "connecting\ncannot-migrate\n");
    assert!(run.output_truncated);
    Ok(())
}

#[tokio::test]
async fn a_deployment_cancels_a_run_an_earlier_deployment_left_instead_of_relaunching_it()
-> Result<(), Box<dyn std::error::Error>> {
    use mm::app::ports::DeploymentJobs as _;
    let setup = setup(true)?;
    let mut left = claimed_run(11, JobRunState::Running, 1);
    left.run.trigger = JobTrigger::BeforeDeploy;
    setup.repository.queue_run(left)?;
    setup.service.sweep(&setup.selection, &[]).await?;
    let events = ordered(&setup)?;
    assert!(
        events.iter().any(|event| event == "repo:request_cancel"),
        "{events:?}"
    );
    assert!(
        events.iter().any(|event| event == "runtime:cancel"),
        "{events:?}"
    );
    assert!(
        !events.iter().any(|event| event == "runtime:launch"),
        "{events:?}"
    );
    let run = setup
        .repository
        .lock()?
        .runs
        .get(&Uuid::from_u128(111))
        .cloned()
        .ok_or("the run is gone")?;
    assert_eq!(run.state, JobRunState::Cancelled);
    Ok(())
}
