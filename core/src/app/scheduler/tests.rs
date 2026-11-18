//! The helpers behind the scheduler service. The service itself is tested
//! against scripted ports in `tests/units/job_scheduler.rs`.

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::domain::config::{DurationSpec, RetryPolicyConfig, ScheduleConcurrency};
use crate::domain::scheduler::runtime_job_name;
use crate::domain::workspace::ProjectId;

const NOW: i64 = 1_800_000_000_000;

/// The project the service's tests use: one job, one job of another
/// environment, and a schedule.
fn project() -> Result<TempDir, Box<dyn std::error::Error>> {
    let project = crate::test_support::tempdir()?;
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
    Ok(project)
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
        misfire: crate::domain::config::MisfirePolicy::RunOnce,
        retry: None,
        timeout: None,
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

#[test]
fn scheduler_helpers_reject_cross_environment_and_invalid_timeouts()
-> Result<(), Box<dyn std::error::Error>> {
    let project = project()?;
    let document = crate::test_project_documents().load_from(project.path())?;
    let mut cross_environment = document.clone();
    cross_environment
        .config
        .schedules
        .get_mut("every-minute")
        .ok_or("missing schedule fixture")?
        .job = "production-only".to_owned();
    assert!(desired_definitions(&cross_environment, "local").is_err());
    assert!(active_job(&document, "local", "production-only").is_err());
    let mut missing_service = job();
    missing_service.module = "missing".to_owned();
    assert!(job_target(&document, &[], &missing_service).is_err());
    assert!(job_target(&document, &[], &job()).is_err());

    let mut claim = claimed_run(30, JobRunState::Running, 1);
    claim.run.started_at_unix_ms = None;
    assert!(!timed_out(&claim, NOW)?);
    claim.job_definition.timeout = DurationSpec("invalid".to_owned());
    assert!(effective_timeout(&claim).is_err());
    claim.run.started_at_unix_ms = Some(NOW);
    assert!(timed_out(&claim, NOW).is_err());

    let mut invalid = schedule();
    invalid.starts_at = Some("invalid".to_owned());
    assert!(schedule_next(&invalid, "identity", NOW).is_err());
    invalid.starts_at = None;
    invalid.ends_at = Some("invalid".to_owned());
    assert!(schedule_next(&invalid, "identity", NOW).is_err());
    invalid.ends_at = None;
    invalid.jitter = DurationSpec("invalid".to_owned());
    assert!(schedule_next(&invalid, "identity", NOW).is_err());

    let audit = audit(
        &document,
        "local",
        &approval(),
        NOW,
        "test",
        "run",
        "subject".to_owned(),
    );
    assert_eq!(audit.occurred_at_unix_ms, NOW);
    assert_eq!(audit.action, "test");
    Ok(())
}
