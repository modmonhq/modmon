//! What runs on a host is visible, stoppable, and never holds the
//! environment for good: an operation in flight is listed with
//! its ID, who started it, and since when; a client that goes away stops
//! what it started and frees the environment; `Cancel` stops it for anyone
//! who may deploy; a step past its deadline ends with a timeout; and an
//! apply whose answer cannot be delivered is still recorded as done.

mod daemon_host;
mod temp_dirs;

use std::sync::atomic::Ordering;
use std::time::Duration;

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult};
use mm::domain::remote::{RemoteMethod, RemoteResult};
use mm::domain::rollout::{NodeDeploymentStatus, RolloutState};

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

/// Waits until `check` holds for the host's deployment status.
async fn status_until(
    host: &Host,
    check: impl Fn(&NodeDeploymentStatus) -> bool,
) -> TestResult<NodeDeploymentStatus> {
    for _ in 0..250 {
        let status = deployment_status(host).await?;
        if check(&status) {
            return Ok(status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!(
        "the host never got there: {:?}",
        deployment_status(host).await?
    )
    .into())
}

fn apply(hash: &str) -> RemoteMethod {
    RemoteMethod::ApplyDeployment {
        remote_plan_hash: hash.to_owned(),
    }
}

#[tokio::test]
async fn a_deployment_in_flight_is_listed_with_its_id_who_and_since_when() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let mut applying = host.ask(ALICE, apply(&hash)).await?;
    host.handlers.entered(1).await?;

    let status = deployment_status(&host).await?;
    let [operation] = status.operations.as_slice() else {
        return Err(format!("expected one operation, got {:?}", status.operations).into());
    };
    assert_eq!(operation.id, hash);
    assert_eq!(operation.what, "deployment");
    assert_eq!(operation.principal, "alice");
    assert!(operation.deadline_unix_ms > operation.started_at_unix_ms);
    let rollout = status.rollout.ok_or("the deployment is not on record")?;
    assert_eq!(rollout.plan_hash, hash);
    assert_eq!(rollout.state, RolloutState::Running);
    assert!(
        rollout.active,
        "a deployment that runs holds the environment"
    );

    // The node report says the same, for `mm doctor`.
    let facts = match host
        .ask(OPS, RemoteMethod::Node)
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::Node(facts) => facts,
        other => return Err(format!("unexpected node answer: {other:?}").into()),
    };
    let daemon = facts
        .daemon
        .ok_or("the node does not say how its daemon fares")?;
    assert_eq!(daemon.operations.len(), 1);
    assert!(daemon.late_children.is_empty());

    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        applying.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    let status = status_until(&host, |status| status.operations.is_empty()).await?;
    let rollout = status
        .rollout
        .ok_or("the finished deployment is not on record")?;
    assert_eq!(rollout.state, RolloutState::Finished);
    assert!(!rollout.active);
    assert_eq!(
        status.current.map(|current| current.commit),
        Some(host.commit.clone())
    );
    Ok(())
}

#[tokio::test]
async fn a_second_deployment_is_refused_at_once_while_one_runs() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let first = host.plan(ALICE).await?;
    let second = host.plan(OPS).await?;
    let _applying = host.ask(ALICE, apply(&first)).await?;
    host.handlers.entered(1).await?;
    let started = std::time::Instant::now();
    let (code, message) = host.ask(OPS, apply(&second)).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_DEPLOY_RUNNING");
    assert!(
        message.contains("alice started deploying plan"),
        "{message}"
    );
    assert!(message.contains("mm deploy cancel"), "{message}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the refusal waited"
    );
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    Ok(())
}

#[tokio::test]
async fn a_deployment_whose_client_goes_away_is_stopped_and_frees_the_environment() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let applying = host.ask(ALICE, apply(&hash)).await?;
    host.handlers.entered(1).await?;
    applying.hang_up();

    let status = status_until(&host, |status| status.operations.is_empty()).await?;
    assert_eq!(host.handlers.abandoned.load(Ordering::SeqCst), 1);
    let rollout = status
        .rollout
        .ok_or("the stopped deployment is not on record")?;
    assert_eq!(
        rollout.state,
        RolloutState::Running,
        "it is left unfinished"
    );
    assert!(!rollout.active, "the environment is free at once");
    let failure = rollout.failure.ok_or("no reason is recorded")?.reason;
    assert!(
        failure.contains("client that started it went away"),
        "{failure}"
    );
    assert!(host.audit()?.iter().any(|record| {
        record["method"] == "apply_deployment"
            && record["result"] == "failed"
            && record["error_code"] == "MM_CANCELLED"
    }));

    // The next deployment goes ahead with no manual step.
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    let next = host.plan(ALICE).await?;
    assert!(matches!(
        host.ask(ALICE, apply(&next)).await?.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    Ok(())
}

#[tokio::test]
async fn cancel_stops_a_deployment_rolls_it_back_and_names_who_did_what() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let mut applying = host.ask(ALICE, apply(&hash)).await?;
    host.handlers.entered(1).await?;

    let cancel = RemoteMethod::Cancel {
        operation: Some(hash[..12].to_owned()),
    };
    let outcome = match host.ask(OPS, cancel).await?.result(PROMPT).await? {
        RemoteResult::Cancelled(outcome) => outcome,
        other => return Err(format!("unexpected cancel answer: {other:?}").into()),
    };
    assert_eq!(outcome.stopped.len(), 1);
    assert_eq!(outcome.stopped[0].id, hash);
    assert_eq!(outcome.rolled_back.as_deref(), Some(hash.as_str()));
    assert!(outcome.problem.is_none(), "{:?}", outcome.problem);
    assert_eq!(host.handlers.abandoned.load(Ordering::SeqCst), 1);
    assert_eq!(host.handlers.rolled_back.load(Ordering::SeqCst), 1);

    // The client that started it hears why.
    let (code, message) = applying.refusal(PROMPT).await?;
    assert_eq!(code, "MM_CANCELLED");
    assert!(
        message.contains("cancelled by break-glass:ops"),
        "{message}"
    );

    let status = deployment_status(&host).await?;
    assert!(status.operations.is_empty());
    let rollout = status
        .rollout
        .ok_or("the cancelled deployment is not on record")?;
    assert_eq!(rollout.state, RolloutState::RolledBack);
    assert!(!rollout.active);
    assert!(rollout.failure.is_some_and(|failure| {
        failure
            .reason
            .contains("the deployment alice started was cancelled by break-glass:ops")
    }));
    let audited = host
        .audit()?
        .into_iter()
        .find(|record| record["result"] == "cancelled")
        .ok_or("the cancellation is not in the audit log")?;
    assert_eq!(audited["cancelled_by"], "break-glass:ops");
    assert_eq!(audited["started_by"], "alice");
    assert_eq!(audited["operations"][0]["started_by"], "alice");

    // Nothing runs any more: a second cancel finds nothing.
    let again = match host
        .ask(OPS, RemoteMethod::Cancel { operation: None })
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::Cancelled(outcome) => outcome,
        other => return Err(format!("unexpected cancel answer: {other:?}").into()),
    };
    assert!(again.is_empty(), "{again:?}");
    Ok(())
}

#[tokio::test]
async fn a_step_past_its_deadline_ends_with_a_timeout_and_frees_the_environment() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let mut applying = host
        .ask_within(ALICE, apply(&hash), Duration::from_secs(4))
        .await?;
    let (code, message) = applying.refusal(Duration::from_secs(10)).await?;
    assert_eq!(code, "MM_TIMEOUT");
    assert!(message.contains("passed its deadline"), "{message}");
    assert_eq!(host.handlers.abandoned.load(Ordering::SeqCst), 1);
    let status = deployment_status(&host).await?;
    assert!(status.operations.is_empty());
    let rollout = status.rollout.ok_or("the deployment is not on record")?;
    assert!(!rollout.active);
    assert!(rollout.unfinished());
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    Ok(())
}

#[tokio::test]
async fn an_apply_whose_answer_cannot_be_delivered_is_recorded_as_done() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let applying = host.ask(ALICE, apply(&hash)).await?;
    host.handlers.entered(1).await?;
    // The way back breaks; the client is still connected.
    let _applying = applying.stop_reading()?;
    host.handlers.hold_apply.store(false, Ordering::SeqCst);

    let status = status_until(&host, |status| {
        status
            .rollout
            .as_ref()
            .is_some_and(|rollout| rollout.state != RolloutState::Running)
    })
    .await?;
    let rollout = status.rollout.ok_or("the deployment is not on record")?;
    assert_eq!(rollout.state, RolloutState::Finished);
    assert_eq!(
        status.current.map(|current| current.commit),
        Some(host.commit.clone())
    );
    assert_eq!(host.deployed_commit()?, host.commit);
    // The journal keeps what the apply did apart from delivering it.
    for _ in 0..100 {
        if host
            .audit()?
            .iter()
            .any(|record| record["result"] == "undelivered")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let audit = host.audit()?;
    let applied = audit
        .iter()
        .filter(|record| record["method"] == "apply_deployment")
        .map(|record| record["result"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(applied, ["authorized", "succeeded", "undelivered"]);
    Ok(())
}

#[tokio::test]
async fn a_daemon_that_starts_marks_the_requests_and_the_apply_it_finds_running() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let hash = host.plan(ALICE).await?;
    let _applying = host.ask(ALICE, apply(&hash)).await?;
    host.handlers.entered(1).await?;

    host.next_daemon()?.resume_after_restart()?;
    let requests = std::fs::read_dir(host.config.state_root.join("requests"))?
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .filter(|record| record["phase"] == "interrupted")
        .count();
    assert_eq!(
        requests, 1,
        "the running apply's request is marked interrupted"
    );
    let rollout = deployment_status(&host)
        .await?
        .rollout
        .ok_or("the deployment is not on record")?;
    assert_eq!(rollout.plan_hash, hash);
    assert!(!rollout.active, "the environment is free after the restart");
    assert!(rollout.unfinished());
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    Ok(())
}
