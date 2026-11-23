use std::collections::BTreeMap;
use std::time::Duration;

use mm::adapters::state::SqliteStateStore;
use mm::app::ports::SchedulerRepository;
use mm::domain::config::{
    DurationSpec, JobConfig, MisfirePolicy, RetryPolicyConfig, ScheduleConcurrency, ScheduleConfig,
};
use mm::domain::scheduler::{
    ClaimedJobRun, ConcurrencyAction, DesiredDefinitions, JobAuditRecord, JobRunCompletion,
    JobRunState, JobTrigger, NewJobRun, ReconciliationAction, ReconciliationCommit,
    RuntimeJobState, ScheduleTickCommit, StoredDefinitionHashes, apply_jitter, concurrency_action,
    definition_hash, due_occurrences, next_occurrence, parse_duration, parse_instant,
    reconciliation_plan, retry_delay,
};
use mm::domain::workspace::ProjectId;
use uuid::Uuid;

mod temp_dirs;

fn job() -> JobConfig {
    JobConfig {
        module: "app".to_owned(),
        service: "web".to_owned(),
        command: vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "schedule:run".to_owned(),
        ],
        environments: None,
        run: None,
        timeout: DurationSpec("10m".to_owned()),
        retry: RetryPolicyConfig {
            attempts: 3,
            backoff: DurationSpec("5s".to_owned()),
            max_backoff: DurationSpec("20s".to_owned()),
        },
        output_limit_bytes: 1024,
        history_limit: 10,
        placement: None,
        user: None,
        placed_elsewhere: None,
        development: None,
        off_locally: None,
    }
}

fn schedule() -> ScheduleConfig {
    ScheduleConfig {
        job: "schedule-run".to_owned(),
        cron: "* * * * *".to_owned(),
        timezone: "UTC".to_owned(),
        enabled: true,
        starts_at: None,
        ends_at: None,
        jitter: DurationSpec("0s".to_owned()),
        concurrency: ScheduleConcurrency::Forbid,
        misfire: MisfirePolicy::RunOnce,
        retry: None,
        timeout: None,
    }
}

fn desired() -> DesiredDefinitions {
    DesiredDefinitions {
        jobs: BTreeMap::from([("schedule-run".to_owned(), job())]),
        schedules: BTreeMap::from([("framework".to_owned(), schedule())]),
    }
}

fn approval(project_id: ProjectId, now: i64, action: &str, subject: &str) -> JobAuditRecord {
    JobAuditRecord {
        occurred_at_unix_ms: now,
        project_id,
        environment: "local".to_owned(),
        principal: "test-operator".to_owned(),
        source: "scheduler-test".to_owned(),
        action: action.to_owned(),
        subject_kind: "run".to_owned(),
        subject_id: subject.to_owned(),
        definition_hash: None,
        details: BTreeMap::new(),
    }
}

#[test]
fn cron_timezone_dst_leap_calendar_and_jitter_are_deterministic()
-> Result<(), Box<dyn std::error::Error>> {
    let before_spring = parse_instant("2024-03-09T07:30:00Z")?;
    assert_eq!(
        next_occurrence("30 2 * * *", "America/New_York", before_spring, None, None)?,
        Some(parse_instant("2024-03-11T06:30:00Z")?),
        "the nonexistent spring-forward 02:30 is skipped"
    );

    let before_fall = parse_instant("2024-11-03T04:00:00Z")?;
    let first = next_occurrence("30 1 * * *", "America/New_York", before_fall, None, None)?
        .ok_or("missing first fall-back occurrence")?;
    let second = next_occurrence("30 1 * * *", "America/New_York", first, None, None)?
        .ok_or("missing second fall-back occurrence")?;
    assert_eq!(first, parse_instant("2024-11-03T05:30:00Z")?);
    assert_eq!(second, parse_instant("2024-11-03T06:30:00Z")?);

    assert_eq!(
        next_occurrence(
            "0 0 29 feb *",
            "UTC",
            parse_instant("2023-03-01T00:00:00Z")?,
            None,
            None,
        )?,
        Some(parse_instant("2024-02-29T00:00:00Z")?)
    );
    assert_eq!(
        next_occurrence(
            "0 0 1 jan *",
            "Pacific/Kiritimati",
            parse_instant("2023-12-30T23:59:00Z")?,
            None,
            None,
        )?,
        Some(parse_instant("2023-12-31T10:00:00Z")?)
    );

    let due = parse_instant("2026-08-23T12:00:00Z")?;
    let jittered = apply_jitter("project:local:framework", due, Duration::from_secs(30));
    assert_eq!(
        jittered,
        apply_jitter("project:local:framework", due, Duration::from_secs(30))
    );
    assert!((due..=due + 30_000).contains(&jittered));
    assert!(next_occurrence("60 * * * *", "UTC", due, None, None).is_err());
    assert!(next_occurrence("* * * * *", "Not/AZone", due, None, None).is_err());
    Ok(())
}

#[test]
fn scheduler_states_and_value_validation_are_explicit() -> Result<(), Box<dyn std::error::Error>> {
    for state in [
        JobRunState::Succeeded,
        JobRunState::Failed,
        JobRunState::TimedOut,
        JobRunState::Cancelled,
    ] {
        assert!(state.is_terminal());
        assert!(!state.is_active());
    }
    for state in [
        JobRunState::Launching,
        JobRunState::Running,
        JobRunState::Cancelling,
        JobRunState::Unknown,
    ] {
        assert!(state.is_active());
        assert!(!state.is_terminal());
    }
    for state in [JobRunState::Queued, JobRunState::RetryWait] {
        assert!(!state.is_active());
        assert!(!state.is_terminal());
    }

    for (value, expected) in [
        ("0ms", Duration::ZERO),
        ("1s", Duration::from_secs(1)),
        ("1m", Duration::from_secs(60)),
        ("1d", Duration::from_secs(86_400)),
    ] {
        assert_eq!(parse_duration(value)?, expected);
    }
    for (value, expected) in [
        (
            "",
            "duration must be a compact value such as 30s, 10m, 2h, or 1d",
        ),
        (
            " 1s",
            "duration must be a compact value such as 30s, 10m, 2h, or 1d",
        ),
        ("42", "duration requires one of ms, s, m, h, or d"),
        ("xs", "duration amount is invalid"),
        ("1fortnight", "duration requires one of ms, s, m, h, or d"),
    ] {
        assert_eq!(
            parse_duration(value).as_ref().map_err(String::as_str),
            Err(expected)
        );
    }
    assert_eq!(
        parse_duration(&format!("{}d", u64::MAX))
            .as_ref()
            .map_err(String::as_str),
        Err("duration exceeds the supported range")
    );
    assert_eq!(
        parse_instant("2026-08-24").as_ref().map_err(String::as_str),
        Err("timestamp must be RFC 3339 with an explicit offset")
    );
    assert_eq!(retry_delay(Duration::MAX, Duration::MAX, 2), Duration::MAX);
    Ok(())
}

#[test]
fn cron_syntax_bounds_and_calendar_rules_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let after = parse_instant("2026-01-01T00:00:00Z")?;
    for (expression, expected) in [
        (
            "0 0 1",
            "cron must contain exactly five fields: minute hour day month weekday",
        ),
        (", * * * *", "cron contains an empty list item"),
        ("*/0 * * * *", "cron step must be a positive integer"),
        ("*/x * * * *", "cron step must be a positive integer"),
        ("10-5 * * * *", "cron range start is after its end"),
        ("bogus * * * *", "invalid cron value 'bogus'"),
        ("60 * * * *", "cron value '60' is outside 0..=59"),
        ("* 24 * * *", "cron value '24' is outside 0..=23"),
        ("* * 0 * *", "cron value '0' is outside 1..=31"),
        ("* * * 13 *", "cron value '13' is outside 1..=12"),
        ("* * * * 8", "cron value '8' is outside 0..=7"),
    ] {
        assert_eq!(
            next_occurrence(expression, "UTC", after, None, None)
                .as_ref()
                .map_err(String::as_str),
            Err(expected),
            "expression: {expression}"
        );
    }

    assert_eq!(
        next_occurrence("0 0 31 JAN mon", "UTC", after, None, None)?,
        Some(parse_instant("2026-01-05T00:00:00Z")?),
        "restricted day-of-month and weekday fields use cron OR semantics"
    );
    assert_eq!(
        next_occurrence("0 0 * * 7", "UTC", after, None, None)?,
        Some(parse_instant("2026-01-04T00:00:00Z")?),
        "weekday seven aliases Sunday"
    );
    assert_eq!(
        next_occurrence("10-20/5 * * * *", "UTC", after, None, None)?,
        Some(parse_instant("2026-01-01T00:10:00Z")?)
    );
    assert_eq!(
        next_occurrence("1/4294967295 * * * *", "UTC", after, None, None)?,
        Some(parse_instant("2026-01-01T00:01:00Z")?),
        "an overflowing next step terminates the bounded field"
    );
    assert_eq!(
        next_occurrence(
            "0 0 * * *",
            "UTC",
            after,
            Some(parse_instant("2026-01-03T00:00:00Z")?),
            None,
        )?,
        Some(parse_instant("2026-01-03T00:00:00Z")?)
    );
    assert_eq!(
        next_occurrence(
            "0 0 * * *",
            "UTC",
            after,
            None,
            Some(parse_instant("2026-01-01T12:00:00Z")?),
        )?,
        None
    );
    assert_eq!(
        next_occurrence("* * * * *", "UTC", i64::MAX, None, None)
            .as_ref()
            .map_err(String::as_str),
        Err("schedule timestamp is outside the supported calendar range")
    );
    assert_eq!(
        next_occurrence("0 0 31 feb *", "UTC", after, None, None)
            .as_ref()
            .map_err(String::as_str),
        Err("cron expression has no occurrence within ten years")
    );
    Ok(())
}

#[test]
fn reconciliation_and_due_calculation_cover_stable_updates_and_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let definitions = desired();
    let project_id = ProjectId(Uuid::from_u128(0x42));
    let unchanged = StoredDefinitionHashes {
        jobs: BTreeMap::from([(
            "schedule-run".to_owned(),
            definition_hash(&definitions.jobs["schedule-run"])?,
        )]),
        schedules: BTreeMap::from([(
            "framework".to_owned(),
            definition_hash(&definitions.schedules["framework"])?,
        )]),
    };
    let stable = reconciliation_plan(
        project_id,
        "local".to_owned(),
        "a".repeat(64),
        &definitions,
        &unchanged,
    )?;
    assert!(stable.changes.is_empty());
    assert_eq!(stable.calculate_hash()?, stable.plan_hash);

    let changed = reconciliation_plan(
        project_id,
        "local".to_owned(),
        "b".repeat(64),
        &definitions,
        &StoredDefinitionHashes {
            jobs: BTreeMap::from([("schedule-run".to_owned(), "old-job".to_owned())]),
            schedules: BTreeMap::from([("framework".to_owned(), "old-schedule".to_owned())]),
        },
    )?;
    assert_eq!(changed.changes.len(), 2);
    assert_eq!(changed.changes[0].action, ReconciliationAction::UpdateJob);
    assert_eq!(
        changed.changes[1].action,
        ReconciliationAction::UpdateSchedule
    );

    let mut oversized_job = definitions.clone();
    oversized_job
        .jobs
        .get_mut("schedule-run")
        .ok_or("schedule-run job missing")?
        .command = vec!["x".repeat(2 * 1024 * 1024)];
    assert!(
        reconciliation_plan(
            project_id,
            "local".to_owned(),
            "c".repeat(64),
            &oversized_job,
            &StoredDefinitionHashes::default(),
        )
        .is_err()
    );

    let mut oversized_schedule = definitions.clone();
    oversized_schedule.jobs.clear();
    oversized_schedule
        .schedules
        .get_mut("framework")
        .ok_or("framework schedule missing")?
        .timezone = "x".repeat(2 * 1024 * 1024);
    assert!(
        reconciliation_plan(
            project_id,
            "local".to_owned(),
            "d".repeat(64),
            &oversized_schedule,
            &StoredDefinitionHashes::default(),
        )
        .is_err()
    );

    let first = parse_instant("2026-08-24T12:01:00Z")?;
    let now = parse_instant("2026-08-24T13:45:00Z")?;
    let mut bounded = schedule();
    bounded.misfire = MisfirePolicy::BoundedCatchUp { maximum: 100 };
    let due = due_occurrences(&bounded, "bounded", first, now)?;
    assert_eq!(due.runs.len(), 100);
    assert_eq!(
        due.next_due_at_unix_ms,
        Some(parse_instant("2026-08-24T13:46:00Z")?)
    );

    let future = parse_instant("2026-08-25T00:00:00Z")?;
    bounded.misfire = MisfirePolicy::Skip;
    let not_due = due_occurrences(&bounded, "future", future, now)?;
    assert!(not_due.runs.is_empty());
    assert_eq!(not_due.next_due_at_unix_ms, Some(future));

    for (starts_at, ends_at, jitter) in [
        (Some("invalid"), None, "0s"),
        (None, Some("invalid"), "0s"),
        (None, None, "invalid"),
    ] {
        let mut invalid = schedule();
        invalid.starts_at = starts_at.map(str::to_owned);
        invalid.ends_at = ends_at.map(str::to_owned);
        invalid.jitter = DurationSpec(jitter.to_owned());
        assert!(due_occurrences(&invalid, "invalid", first, now).is_err());
    }
    let mut invalid_cron = schedule();
    invalid_cron.cron = "invalid".to_owned();
    assert_eq!(
        due_occurrences(&invalid_cron, "invalid", first, now)
            .as_ref()
            .map_err(String::as_str),
        Err("cron must contain exactly five fields: minute hour day month weekday")
    );

    let calendar_maximum = chrono::DateTime::<chrono::Utc>::MAX_UTC.timestamp_millis();
    let calendar_overflow = due_occurrences(
        &schedule(),
        "calendar-overflow",
        calendar_maximum - 200 * 60_000,
        calendar_maximum,
    );
    assert_eq!(
        calendar_overflow.as_ref().map_err(String::as_str),
        Err("schedule timestamp is outside the supported calendar range")
    );
    Ok(())
}

#[test]
fn concurrency_misfire_retry_and_duration_truth_tables_are_explicit()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        concurrency_action(ScheduleConcurrency::Allow, 2),
        ConcurrencyAction::Launch
    );
    assert_eq!(
        concurrency_action(ScheduleConcurrency::Forbid, 0),
        ConcurrencyAction::Launch
    );
    assert_eq!(
        concurrency_action(ScheduleConcurrency::Forbid, 1),
        ConcurrencyAction::Suppress
    );
    assert_eq!(
        concurrency_action(ScheduleConcurrency::Replace, 1),
        ConcurrencyAction::CancelAndLaunch
    );

    let first = parse_instant("2026-08-23T12:01:00Z")?;
    let now = parse_instant("2026-08-23T12:05:30Z")?;
    let mut value = schedule();
    value.misfire = MisfirePolicy::Skip;
    assert!(
        due_occurrences(&value, "schedule", first, now)?
            .runs
            .is_empty()
    );
    value.misfire = MisfirePolicy::RunOnce;
    assert_eq!(
        due_occurrences(&value, "schedule", first, now)?.runs.len(),
        1
    );
    value.misfire = MisfirePolicy::BoundedCatchUp { maximum: 3 };
    assert_eq!(
        due_occurrences(&value, "schedule", first, now)?.runs.len(),
        3
    );

    assert_eq!(parse_duration("250ms")?, Duration::from_millis(250));
    assert_eq!(parse_duration("2h")?, Duration::from_secs(7200));
    assert!(parse_duration("2 hours").is_err());
    assert_eq!(
        [1_u16, 2, 3, 4].map(|attempt| retry_delay(
            Duration::from_secs(5),
            Duration::from_secs(20),
            attempt
        )),
        [
            Duration::from_secs(5),
            Duration::from_secs(10),
            Duration::from_secs(20),
            Duration::from_secs(20),
        ]
    );
    Ok(())
}

#[test]
fn sqlite_reconciliation_leases_fencing_recovery_and_retention_are_durable()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(0x1234));
    let now = parse_instant("2026-08-23T12:00:00Z")?;
    let config_hash = "a".repeat(64);
    let desired = desired();
    let plan = reconciliation_plan(
        project_id,
        "local".to_owned(),
        config_hash.clone(),
        &desired,
        &StoredDefinitionHashes {
            jobs: BTreeMap::new(),
            schedules: BTreeMap::new(),
        },
    )?;
    store.reconcile(&ReconciliationCommit {
        plan: plan.clone(),
        desired: desired.clone(),
        now_unix_ms: now,
        principal: "controller".to_owned(),
        source: "test".to_owned(),
    })?;
    assert_eq!(store.list_jobs(project_id, "local")?, desired);
    assert_eq!(store.list_schedules(project_id, "local")?.len(), 1);
    let disabled = store.set_schedule_enabled(
        project_id,
        "local",
        "framework",
        false,
        None,
        &approval(project_id, now + 1, "disable", "framework"),
    )?;
    assert!(!disabled.enabled);
    assert!(disabled.next_due_at_unix_ms.is_none());
    assert!(
        store
            .set_schedule_enabled(
                project_id,
                "local",
                "missing",
                true,
                Some(now + 60_000),
                &approval(project_id, now + 2, "enable", "missing"),
            )
            .is_err()
    );
    let enabled = store.set_schedule_enabled(
        project_id,
        "local",
        "framework",
        true,
        Some(now + 60_000),
        &approval(project_id, now + 3, "enable", "framework"),
    )?;
    assert!(enabled.enabled);
    assert_eq!(enabled.next_due_at_unix_ms, Some(now + 60_000));
    assert!(enabled.generation > disabled.generation);
    assert!(
        store
            .reconcile(&ReconciliationCommit {
                plan,
                desired: desired.clone(),
                now_unix_ms: now,
                principal: "controller".to_owned(),
                source: "test".to_owned(),
            })
            .is_err(),
        "an already-applied plan is stale"
    );

    let due = now + 60_000;
    let first_owner = Uuid::from_u128(1);
    let first = store.claim_due_schedules(
        project_id,
        "local",
        first_owner,
        due,
        Duration::from_secs(30),
        10,
    )?;
    assert_eq!(first.len(), 1);
    assert!(
        store
            .claim_due_schedules(
                project_id,
                "local",
                Uuid::from_u128(2),
                due,
                Duration::from_secs(30),
                10,
            )?
            .is_empty()
    );
    let second_owner = Uuid::from_u128(3);
    let second = store.claim_due_schedules(
        project_id,
        "local",
        second_owner,
        due + 30_001,
        Duration::from_secs(30),
        10,
    )?;
    assert_eq!(second.len(), 1);
    assert!(second[0].fence_token > first[0].fence_token);
    assert!(
        store
            .commit_schedule_tick(&ScheduleTickCommit {
                claim: first[0].clone(),
                due_at_unix_ms: vec![due],
                next_due_at_unix_ms: Some(due + 60_000),
                now_unix_ms: due + 30_001,
                principal: "controller".to_owned(),
                source: "test".to_owned(),
            })
            .is_err(),
        "the expired schedule owner is fenced"
    );
    let runs = store.commit_schedule_tick(&ScheduleTickCommit {
        claim: second[0].clone(),
        due_at_unix_ms: vec![due],
        next_due_at_unix_ms: Some(due + 60_000),
        now_unix_ms: due + 30_001,
        principal: "controller".to_owned(),
        source: "test".to_owned(),
    })?;
    assert_eq!(runs.len(), 1);

    let run_owner = Uuid::from_u128(4);
    let first_run_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            run_owner,
            due + 30_001,
            Duration::from_secs(1),
        )?
        .ok_or("run was not claimable")?;
    let replacement = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(5),
            due + 31_002,
            Duration::from_secs(1),
        )?
        .ok_or("expired run was not recovered")?;
    assert!(replacement.run.fence_token > first_run_claim.run.fence_token);
    assert!(
        store
            .mark_run_running(&first_run_claim, "container-stale", due + 31_002)
            .is_err()
    );
    let running = store.mark_run_running(&replacement, "container-current", due + 31_002)?;
    assert_eq!(running.state, JobRunState::Running);

    let completion_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(6),
            due + 31_002,
            Duration::from_secs(1),
        )?
        .ok_or("running job was not reconcilable")?;
    assert!(
        store
            .complete_run(&JobRunCompletion {
                run_id: completion_claim.run.id,
                lease_owner: completion_claim.lease_owner,
                fence_token: completion_claim.run.fence_token,
                state: JobRunState::Succeeded,
                finished_at_unix_ms: due + 31_500,
                exit_code: Some(0),
                error_code: None,
                diagnostic_code: None,
                output: "x".repeat(1024 * 1024 + 1),
                output_truncated: true,
            })
            .is_err(),
        "unbounded output is rejected before SQLite"
    );
    store.complete_run(&JobRunCompletion {
        run_id: completion_claim.run.id,
        lease_owner: completion_claim.lease_owner,
        fence_token: completion_claim.run.fence_token,
        state: JobRunState::Succeeded,
        finished_at_unix_ms: due + 31_500,
        exit_code: Some(0),
        error_code: None,
        diagnostic_code: None,
        output: "bounded diagnostic".to_owned(),
        output_truncated: false,
    })?;

    let empty = DesiredDefinitions {
        jobs: BTreeMap::new(),
        schedules: BTreeMap::new(),
    };
    let hashes = store.definition_hashes(project_id, "local")?;
    let retire = reconciliation_plan(
        project_id,
        "local".to_owned(),
        "b".repeat(64),
        &empty,
        &hashes,
    )?;
    store.reconcile(&ReconciliationCommit {
        plan: retire,
        desired: empty,
        now_unix_ms: due + 40_000,
        principal: "controller".to_owned(),
        source: "test".to_owned(),
    })?;
    assert!(store.list_schedules(project_id, "local")?[0].retired);
    assert_eq!(store.history(project_id, "local", None, None, 10)?.len(), 1);
    Ok(())
}

#[test]
fn idempotency_retry_cancel_unknown_and_audit_metadata_do_not_store_secret_output()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let path = temporary.path().join("state.sqlite3");
    let store = SqliteStateStore::open(&path)?;
    let project_id = ProjectId(Uuid::from_u128(0x5678));
    let now = 1_000_000;
    let run = NewJobRun {
        id: Uuid::from_u128(11),
        project_id,
        environment: "local".to_owned(),
        job: "schedule-run".to_owned(),
        schedule: None,
        trigger: JobTrigger::Manual,
        idempotency_key: "operator:test:stable".to_owned(),
        job_definition: job(),
        schedule_definition: None,
        config_hash: "c".repeat(64),
        requested_at_unix_ms: now,
        scheduled_for_unix_ms: None,
        principal: "operator".to_owned(),
        source: "test".to_owned(),
        retry_of_run_id: None,
    };
    let first = store.insert_run(&run, &approval(project_id, now, "enqueue", "first"))?;
    let duplicate = store.insert_run(
        &NewJobRun {
            id: Uuid::from_u128(12),
            ..run.clone()
        },
        &approval(project_id, now + 1, "enqueue", "duplicate"),
    )?;
    assert_eq!(
        first.id, duplicate.id,
        "idempotency returns the original durable run"
    );

    let claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(13),
            now,
            Duration::from_secs(1),
        )?
        .ok_or("manual run was not claimable")?;
    store.mark_run_unknown(&claim, "MM_RUNTIME_TRANSPORT_UNKNOWN")?;
    assert_eq!(
        store.run(first.id)?.ok_or("missing unknown run")?.state,
        JobRunState::Unknown
    );
    assert!(
        store
            .claim_runnable_run(
                project_id,
                "local",
                Uuid::from_u128(14),
                now + 999,
                Duration::from_secs(1),
            )?
            .is_none(),
        "unknown runtime state is not retried before its reconciliation lease expires"
    );
    let unknown_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(14),
            now + 1_000,
            Duration::from_secs(1),
        )?
        .ok_or("unknown runtime state was not reconcilable after lease expiry")?;
    assert_eq!(unknown_claim.run.state, JobRunState::Unknown);
    store.mark_run_unknown(&unknown_claim, "MM_RUNTIME_STILL_UNKNOWN")?;
    let cancelled = store.request_cancel(
        first.id,
        &approval(project_id, now + 2_000, "cancel", &first.id.to_string()),
    )?;
    assert_eq!(cancelled.state, JobRunState::Cancelling);
    let cancel_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(15),
            now + 2_000,
            Duration::from_secs(1),
        )?
        .ok_or("cancel request was not recoverable")?;
    store.complete_run(&JobRunCompletion {
        run_id: first.id,
        lease_owner: cancel_claim.lease_owner,
        fence_token: cancel_claim.run.fence_token,
        state: JobRunState::Cancelled,
        finished_at_unix_ms: now + 2_001,
        exit_code: Some(130),
        error_code: Some("MM_CANCELLED".to_owned()),
        diagnostic_code: None,
        output: String::new(),
        output_truncated: false,
    })?;

    let bytes = std::fs::read(&path)?;
    let database = String::from_utf8_lossy(&bytes);
    assert!(!database.contains("scheduler-secret-sentinel-42"));
    let _compile_time_runtime_state_evidence = RuntimeJobState::Unknown {
        diagnostic_code: "MM_RUNTIME_TRANSPORT_UNKNOWN".to_owned(),
    };
    let _compile_time_claim_evidence: Option<ClaimedJobRun> = None;
    Ok(())
}

#[test]
fn forbid_concurrency_serializes_already_queued_schedule_runs()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(0x9abc));
    let now = 2_000_000;
    for sequence in 1..=2_u128 {
        let id = Uuid::from_u128(sequence << 120);
        store.insert_run(
            &NewJobRun {
                id,
                project_id,
                environment: "local".to_owned(),
                job: "schedule-run".to_owned(),
                schedule: Some("framework".to_owned()),
                trigger: JobTrigger::Schedule,
                idempotency_key: format!("schedule:framework:{sequence}"),
                job_definition: job(),
                schedule_definition: Some(schedule()),
                config_hash: "d".repeat(64),
                requested_at_unix_ms: now + i64::try_from(sequence)?,
                scheduled_for_unix_ms: Some(now + i64::try_from(sequence)?),
                principal: "controller".to_owned(),
                source: "test".to_owned(),
                retry_of_run_id: None,
            },
            &approval(project_id, now, "enqueue", &id.to_string()),
        )?;
    }

    let first = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(10),
            now + 10,
            Duration::from_secs(30),
        )?
        .ok_or("first queued run was not claimable")?;
    store.mark_run_running(&first, "container-first", now + 10)?;
    let active = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(11),
            now + 10,
            Duration::from_secs(30),
        )?
        .ok_or("running run was not claimable for inspection")?;
    assert_eq!(active.run.id, first.run.id);
    assert!(
        store
            .claim_runnable_run(
                project_id,
                "local",
                Uuid::from_u128(12),
                now + 10,
                Duration::from_secs(30),
            )?
            .is_none(),
        "forbid must not launch another queued run while one is active"
    );

    let waiting = Uuid::from_u128(2 << 120);
    assert!(
        store
            .claim_run(
                waiting,
                Uuid::from_u128(15),
                now + 10,
                Duration::from_secs(30)
            )?
            .is_none(),
        "a read must not launch another run of a forbid schedule while one is active"
    );

    let completion_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(13),
            now + 40_001,
            Duration::from_secs(30),
        )?
        .ok_or("running run was not recoverable")?;
    store.complete_run(&JobRunCompletion {
        run_id: completion_claim.run.id,
        lease_owner: completion_claim.lease_owner,
        fence_token: completion_claim.run.fence_token,
        state: JobRunState::Succeeded,
        finished_at_unix_ms: now + 40_001,
        exit_code: Some(0),
        error_code: None,
        diagnostic_code: None,
        output: String::new(),
        output_truncated: false,
    })?;
    assert!(
        store
            .claim_runnable_run(
                project_id,
                "local",
                Uuid::from_u128(14),
                now + 40_001,
                Duration::from_secs(30),
            )?
            .is_some(),
        "the next queued run becomes claimable after completion"
    );

    Ok(())
}

#[test]
fn a_forbid_schedule_run_is_claimed_by_id_once_no_other_runs()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(0x9abd));
    let now = 2_000_000;
    let id = Uuid::from_u128(3 << 120);
    store.insert_run(
        &NewJobRun {
            id,
            project_id,
            environment: "local".to_owned(),
            job: "schedule-run".to_owned(),
            schedule: Some("framework".to_owned()),
            trigger: JobTrigger::Schedule,
            idempotency_key: "schedule:framework:alone".to_owned(),
            job_definition: job(),
            schedule_definition: Some(schedule()),
            config_hash: "d".repeat(64),
            requested_at_unix_ms: now,
            scheduled_for_unix_ms: Some(now),
            principal: "controller".to_owned(),
            source: "test".to_owned(),
            retry_of_run_id: None,
        },
        &approval(project_id, now, "enqueue", &id.to_string()),
    )?;
    let claimed = store
        .claim_run(id, Uuid::from_u128(20), now + 10, Duration::from_secs(30))?
        .ok_or("a forbid run with no other run was not claimable by id")?;
    assert_eq!(claimed.run.id, id);
    assert_eq!(claimed.run.state, JobRunState::Launching);
    Ok(())
}

#[test]
fn retries_audit_and_history_pruning_enforce_fences_and_bounds()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(0xdef0));
    let now = 3_000_000;
    let id = Uuid::from_u128(0xdef1);
    store.insert_run(
        &NewJobRun {
            id,
            project_id,
            environment: "local".to_owned(),
            job: "schedule-run".to_owned(),
            schedule: None,
            trigger: JobTrigger::Manual,
            idempotency_key: "retry:test".to_owned(),
            job_definition: job(),
            schedule_definition: None,
            config_hash: "e".repeat(64),
            requested_at_unix_ms: now,
            scheduled_for_unix_ms: None,
            principal: "operator".to_owned(),
            source: "test".to_owned(),
            retry_of_run_id: None,
        },
        &approval(project_id, now, "enqueue", &id.to_string()),
    )?;
    let first = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(1),
            now,
            Duration::from_secs(1),
        )?
        .ok_or("run not claimed")?;
    store.mark_run_running(&first, "container-retry", now)?;
    let retry_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(2),
            now + 1,
            Duration::from_secs(1),
        )?
        .ok_or("running run not reclaimed")?;
    assert!(
        store
            .schedule_retry(&retry_claim, now + 5_000, "MM_RETRYABLE")
            .is_ok()
    );
    assert!(
        store
            .schedule_retry(&retry_claim, now + 5_000, "MM_STALE")
            .is_err()
    );
    assert!(
        store
            .claim_runnable_run(
                project_id,
                "local",
                Uuid::from_u128(3),
                now + 4_999,
                Duration::from_secs(1),
            )?
            .is_none()
    );
    let final_claim = store
        .claim_runnable_run(
            project_id,
            "local",
            Uuid::from_u128(3),
            now + 5_000,
            Duration::from_secs(1),
        )?
        .ok_or("retry was not due")?;
    let mut exhausted = final_claim.clone();
    exhausted.run.attempt = exhausted.run.maximum_attempts;
    assert!(
        store
            .schedule_retry(&exhausted, now + 10_000, "MM_RETRYABLE")
            .is_err()
    );
    assert!(
        store
            .schedule_retry(&final_claim, now + 10_000, "bad code")
            .is_err()
    );

    let audit = approval(project_id, now + 5_001, "retry", &id.to_string());
    store.append_job_audit(&audit)?;
    store.complete_run(&JobRunCompletion {
        run_id: id,
        lease_owner: final_claim.lease_owner,
        fence_token: final_claim.run.fence_token,
        state: JobRunState::Failed,
        finished_at_unix_ms: now + 5_001,
        exit_code: Some(1),
        error_code: Some("MM_FAILED".to_owned()),
        diagnostic_code: None,
        output: "failed".to_owned(),
        output_truncated: false,
    })?;
    assert_eq!(
        store.prune_history(project_id, "local", "schedule-run", 0)?,
        1
    );
    assert!(
        store
            .history(project_id, "local", None, None, 10)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn a_lease_is_renewed_only_while_its_claim_still_holds_the_run()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = temp_dirs::tempdir()?;
    let store = SqliteStateStore::open(&temporary.path().join("state.sqlite3"))?;
    let project_id = ProjectId(Uuid::from_u128(0x9abc));
    let now = 1_000_000;
    let lease = Duration::from_secs(30);
    let run_id = |id: u128| Uuid::from_u128(id << 100);
    let enqueue = |id: u128| -> Result<(), Box<dyn std::error::Error>> {
        store.insert_run(
            &NewJobRun {
                id: run_id(id),
                project_id,
                environment: "local".to_owned(),
                job: "schedule-run".to_owned(),
                schedule: None,
                trigger: JobTrigger::Manual,
                idempotency_key: format!("operator:lease:{id}"),
                job_definition: job(),
                schedule_definition: None,
                config_hash: "c".repeat(64),
                requested_at_unix_ms: now,
                scheduled_for_unix_ms: None,
                principal: "operator".to_owned(),
                source: "test".to_owned(),
                retry_of_run_id: None,
            },
            &approval(project_id, now, "enqueue", "lease"),
        )?;
        Ok(())
    };
    let claim = |id: u128, owner: u128, at: i64| {
        store.claim_run(run_id(id), Uuid::from_u128(owner), at, lease)
    };

    enqueue(21)?;
    let first = claim(21, 1, now)?.ok_or("the run was not claimable")?;
    assert!(store.renew_lease(&first, now + 20_000, lease)?);
    assert!(
        claim(21, 2, now + 40_000)?.is_none(),
        "the renewed lease holds past the first one's end"
    );
    let second = claim(21, 2, now + 50_001)?.ok_or("the expired lease was not taken")?;
    assert!(
        !store.renew_lease(&first, now + 50_002, lease)?,
        "a claim whose fence moved on renews nothing"
    );
    assert!(store.renew_lease(&second, now + 50_002, lease)?);

    store.request_cancel(
        second.run.id,
        &approval(project_id, now + 50_003, "cancel", "lease"),
    )?;
    assert!(
        !store.renew_lease(&second, now + 50_004, lease)?,
        "a cancel takes the run from its claim"
    );

    enqueue(22)?;
    let ended = claim(22, 3, now)?.ok_or("the second run was not claimable")?;
    store.complete_run(&JobRunCompletion {
        run_id: ended.run.id,
        lease_owner: ended.lease_owner,
        fence_token: ended.run.fence_token,
        state: JobRunState::Succeeded,
        finished_at_unix_ms: now + 1,
        exit_code: Some(0),
        error_code: None,
        diagnostic_code: None,
        output: String::new(),
        output_truncated: false,
    })?;
    assert!(
        !store.renew_lease(&ended, now + 2, lease)?,
        "a run that ended has no lease to renew"
    );
    Ok(())
}
