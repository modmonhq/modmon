//! What runs long on a host shows that it is at work the whole time, and
//! `mm deploy cancel` stops other operations as it stops a
//! deployment: a job run (its run is recorded as cancelled, and the jobs
//! a cancelled deployment started stop with it) and a restart. The client
//! side, where `deploy.progress-timeout` must not end such a step, is in
//! `rollout_walk.rs`; a delivery's upload is in `push_delivery.rs`.

mod daemon_host;
mod temp_dirs;

use std::sync::atomic::Ordering;
use std::time::Duration;

use daemon_host::{ALICE, BOB, Host, OPS, PROMPT, TestResult};
use mm::domain::observability::{ApprovalContext, RestartRequest, ServiceSelector};
use mm::domain::remote::{RemoteMethod, RemoteResult};
use mm::domain::rollout::{CancelOutcome, NodeDeploymentStatus, OperationInFlight};
use mm::domain::scheduler::{JobRunRequest, JobRunState, StartedRuns};

async fn deployment_status(host: &Host) -> TestResult<NodeDeploymentStatus> {
    match host
        .ask(OPS, RemoteMethod::DeploymentStatus)
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::DeploymentStatus(status) => Ok(*status),
        other => Err(format!("unexpected status: {other:?}").into()),
    }
}

/// The one operation in flight, once it is listed.
async fn listed(host: &Host) -> TestResult<OperationInFlight> {
    for _ in 0..250 {
        if let [operation] = deployment_status(host).await?.operations.as_slice() {
            return Ok(operation.clone());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("no operation was listed".into())
}

/// The host keeps seeing operation `id` at work: over a few seconds of
/// waiting, the last time it looked at it moves on.
async fn seen_at_work(host: &Host, first: &OperationInFlight) -> TestResult {
    tokio::time::sleep(Duration::from_millis(4_500)).await;
    let later = listed(host).await?;
    assert_eq!(later.id, first.id);
    assert!(
        later.alive_at_unix_ms >= first.alive_at_unix_ms + 2_000,
        "the host did not see the operation at work: {first:?} then {later:?}"
    );
    Ok(())
}

async fn cancel(host: &Host, operation: Option<&str>) -> TestResult<CancelOutcome> {
    let method = RemoteMethod::Cancel {
        operation: operation.map(str::to_owned),
    };
    match host.ask(OPS, method).await?.result(PROMPT).await? {
        RemoteResult::Cancelled(outcome) => Ok(*outcome),
        other => Err(format!("unexpected cancel answer: {other:?}").into()),
    }
}

fn run_job() -> RemoteMethod {
    RemoteMethod::RunJob(JobRunRequest::manual("migrate", true))
}

fn restart() -> RemoteMethod {
    RemoteMethod::Restart(RestartRequest {
        selector: ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        },
        approval: ApprovalContext {
            principal: "forged".to_owned(),
            source: "forged".to_owned(),
            approved_high_risk: false,
        },
    })
}

#[tokio::test]
async fn a_job_that_runs_long_is_seen_at_work_the_whole_time() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_job.store(true, Ordering::SeqCst);
    let (mut job, request_id) = host.ask_with_id(BOB, run_job()).await?;
    host.handlers.entered(1).await?;

    let operation = listed(&host).await?;
    assert_eq!(
        operation.id,
        request_id.to_string(),
        "the run's operation is the request"
    );
    assert_eq!(operation.what, "run of job 'migrate'");
    assert_eq!(operation.principal, "bob");
    seen_at_work(&host, &operation).await?;

    host.handlers.hold_job.store(false, Ordering::SeqCst);
    let RemoteResult::JobRun(run) = job.result(PROMPT).await? else {
        return Err("the job did not answer with its run".into());
    };
    assert_eq!(run.id, request_id, "the run takes the request's ID");
    Ok(())
}

/// A stack step that waits for a container's drain (up to 15 minutes, 13)
/// waits inside the engine work of the step, as the held apply does here.
#[tokio::test]
async fn a_stack_step_that_drains_long_is_seen_at_work_the_whole_time() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let mut applying = host
        .ask(
            ALICE,
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: hash.clone(),
            },
        )
        .await?;
    host.handlers.entered(1).await?;
    let operation = listed(&host).await?;
    assert_eq!(operation.id, hash);
    seen_at_work(&host, &operation).await?;
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        applying.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    Ok(())
}

#[tokio::test]
async fn cancel_stops_a_job_run_and_records_the_run_as_cancelled() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_job.store(true, Ordering::SeqCst);
    let (mut job, request_id) = host.ask_with_id(BOB, run_job()).await?;
    host.handlers.entered(1).await?;

    let id = request_id.to_string();
    let outcome = cancel(&host, Some(&id[..12])).await?;
    assert_eq!(outcome.stopped.len(), 1);
    assert_eq!(outcome.stopped[0].id, id);
    let [run] = outcome.runs.as_slice() else {
        return Err(format!("expected one cancelled run: {outcome:?}").into());
    };
    assert_eq!(run.run, request_id);
    assert_eq!(run.state, JobRunState::Cancelled);
    assert_eq!(
        *host
            .handlers
            .cancelled_runs
            .lock()
            .map_err(|_| "poisoned")?,
        [vec![StartedRuns::Run(request_id)]],
        "the host stops the run the request started, and nothing else"
    );
    assert_eq!(host.handlers.abandoned.load(Ordering::SeqCst), 1);

    let (code, message) = job.refusal(PROMPT).await?;
    assert_eq!(code, "MM_CANCELLED");
    assert!(message.contains("run of job 'migrate'"), "{message}");
    let audited = host
        .audit()?
        .into_iter()
        .find(|record| record["result"] == "cancelled")
        .ok_or("the cancellation is not in the audit log")?;
    assert_eq!(audited["job_runs"][0]["run_id"], id);
    assert_eq!(audited["operations"][0]["started_by"], "bob");
    assert!(deployment_status(&host).await?.operations.is_empty());
    Ok(())
}

#[tokio::test]
async fn cancel_stops_the_jobs_a_cancelled_deployment_started() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let _applying = host
        .ask(
            ALICE,
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: hash.clone(),
            },
        )
        .await?;
    host.handlers.entered(1).await?;
    let outcome = cancel(&host, None).await?;
    assert_eq!(outcome.rolled_back.as_deref(), Some(hash.as_str()));
    assert_eq!(
        *host
            .handlers
            .cancelled_runs
            .lock()
            .map_err(|_| "poisoned")?,
        [vec![StartedRuns::Deployment(hash)]],
        "the deployment's before- and after-deploy jobs stop once, before the host is rolled back"
    );
    Ok(())
}

#[tokio::test]
async fn cancel_stops_a_restart_and_the_next_one_has_its_turn() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_restart.store(true, Ordering::SeqCst);
    let mut restarting = host.ask(BOB, restart()).await?;
    host.handlers.entered(1).await?;
    let operation = listed(&host).await?;
    assert_eq!(operation.what, "restart of service 'web'");

    let outcome = cancel(&host, None).await?;
    assert_eq!(outcome.stopped.len(), 1);
    assert!(outcome.runs.is_empty(), "a restart starts no job run");
    assert!(
        outcome.rolled_back.is_none(),
        "a restart changes no deployment"
    );
    let (code, _) = restarting.refusal(PROMPT).await?;
    assert_eq!(code, "MM_CANCELLED");
    assert_eq!(host.handlers.abandoned.load(Ordering::SeqCst), 1);

    // Nothing is left holding the host.
    host.handlers.hold_restart.store(false, Ordering::SeqCst);
    assert!(matches!(
        host.ask(BOB, restart()).await?.result(PROMPT).await?,
        RemoteResult::Restarted(_)
    ));
    Ok(())
}

#[tokio::test]
async fn a_job_run_whose_request_answered_is_listed_and_cancelled_by_its_id() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.job_running.store(true, Ordering::SeqCst);
    let id = uuid::Uuid::from_u128(77).to_string();
    let status = deployment_status(&host).await?;
    let [run] = status.operations.as_slice() else {
        return Err(format!("expected the run to be listed: {status:?}").into());
    };
    assert_eq!(run.id, id);
    assert_eq!(run.what, "run of job 'migrate'");
    assert!(run.deadline_unix_ms > run.started_at_unix_ms);

    assert!(cancel(&host, Some("ffffffffffff")).await?.is_empty());
    let outcome = cancel(&host, Some(run.short_id())).await?;
    assert!(outcome.stopped.is_empty(), "{outcome:?}");
    let [cancelled] = outcome.runs.as_slice() else {
        return Err(format!("expected one cancelled run: {outcome:?}").into());
    };
    assert_eq!(cancelled.run.to_string(), id);
    assert_eq!(cancelled.state, JobRunState::Cancelled);
    assert_eq!(
        host.handlers
            .cancelled_runs
            .lock()
            .map_err(|_| "poisoned")?
            .last()
            .cloned(),
        Some(vec![StartedRuns::Named(run.short_id().to_owned())])
    );
    assert!(deployment_status(&host).await?.operations.is_empty());
    Ok(())
}
