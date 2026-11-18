//! What `operate` adds to `view` on a host: restarting a service and
//! running a declared job. An operator without deploy rights may do both,
//! a viewer may not, and every call is in the audit log with its
//! principal. A restart takes the host's turn; a job does not, so a job
//! that runs for hours never holds up a deployment.

mod daemon_host;
mod temp_dirs;

use std::sync::atomic::Ordering;
use std::time::Duration;

use daemon_host::{ALICE, BOB, CAROL, Host, PROMPT, TestResult};
use mm::domain::observability::{ApprovalContext, RestartRequest, ServiceSelector};
use mm::domain::remote::{RemoteMethod, RemoteResult};
use mm::domain::scheduler::{JobRunRequest, JobRunsRequest};

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

fn run_job() -> RemoteMethod {
    RemoteMethod::RunJob(JobRunRequest::manual("migrate", true))
}

/// The audit records of `method` that say `result`, once there is one:
/// the daemon writes the outcome of a request after its answer.
async fn audited(host: &Host, method: &str, result: &str) -> TestResult<Vec<serde_json::Value>> {
    for _ in 0..100 {
        let found = host
            .audit()?
            .into_iter()
            .filter(|record| record["method"] == method && record["result"] == result)
            .collect::<Vec<_>>();
        if !found.is_empty() {
            return Ok(found);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(Vec::new())
}

#[tokio::test]
async fn an_operator_restarts_a_service_and_runs_a_job_and_both_are_audited() -> TestResult {
    let host = Host::with_alice().await?;
    let RemoteResult::Restarted(outcome) = host.ask(BOB, restart()).await?.result(PROMPT).await?
    else {
        return Err("the restart did not answer with what it restarted".into());
    };
    // The daemon names the principal, whatever the client sent.
    assert_eq!(outcome.containers, vec!["bob-restarted".to_owned()]);
    let RemoteResult::JobRun(run) = host.ask(BOB, run_job()).await?.result(PROMPT).await? else {
        return Err("the job did not answer with its run".into());
    };
    assert_eq!(
        (run.job.as_str(), run.principal.as_str()),
        ("migrate", "bob")
    );
    for method in ["restart", "run_job"] {
        let succeeded = audited(&host, method, "succeeded").await?;
        assert_eq!(
            succeeded.len(),
            1,
            "{method} should be audited once as succeeded"
        );
        assert_eq!(succeeded[0]["principal"], "bob");
        assert_eq!(succeeded[0]["capability"]["action"], method_action(method));
    }
    let restarted = host
        .audit()?
        .into_iter()
        .find(|record| record["result"] == "service-restarted")
        .ok_or("the restart left no record of what it restarted")?;
    assert_eq!(
        (&restarted["principal"], &restarted["detail"]["service"]),
        (&serde_json::json!("bob"), &serde_json::json!("web"))
    );
    let ran = host
        .audit()?
        .into_iter()
        .find(|record| record["result"] == "job-run")
        .ok_or("the job left no record of its run")?;
    assert_eq!(ran["detail"]["job"], "migrate");
    Ok(())
}

fn method_action(method: &str) -> &'static str {
    if method == "restart" {
        "restart"
    } else {
        "job-run"
    }
}

#[tokio::test]
async fn a_viewer_may_read_job_runs_and_may_neither_restart_nor_run_a_job() -> TestResult {
    let host = Host::with_alice().await?;
    for method in [restart(), run_job()] {
        let name = method.name();
        let (code, message) = host.ask(CAROL, method).await?.refusal(PROMPT).await?;
        assert_eq!(code, "MM_SECURITY", "{name}: {message}");
        assert_eq!(
            audited(&host, name, "denied").await?.len(),
            1,
            "{name} should be audited as denied"
        );
    }
    assert_eq!(host.handlers.finished.load(Ordering::SeqCst), 0);
    let runs = RemoteMethod::JobRuns(JobRunsRequest {
        job: None,
        run: None,
        limit: 10,
    });
    let RemoteResult::JobRuns(runs) = host.ask(CAROL, runs).await?.result(PROMPT).await? else {
        return Err("job runs did not answer with runs".into());
    };
    assert_eq!(runs.len(), 1);
    Ok(())
}

#[tokio::test]
async fn deploy_access_includes_restarting_and_running_jobs() -> TestResult {
    let host = Host::with_alice().await?;
    assert!(matches!(
        host.ask(ALICE, restart()).await?.result(PROMPT).await?,
        RemoteResult::Restarted(_)
    ));
    assert!(matches!(
        host.ask(ALICE, run_job()).await?.result(PROMPT).await?,
        RemoteResult::JobRun(_)
    ));
    Ok(())
}

#[tokio::test]
async fn a_running_job_takes_no_turn_and_a_restart_waits_for_its_turn() -> TestResult {
    let host = Host::with_alice().await?;
    host.handlers.hold_job.store(true, Ordering::SeqCst);
    let mut job = host.ask(BOB, run_job()).await?;
    host.handlers.entered(1).await?;
    // A deployment goes ahead while the job runs.
    let plan = host.plan(ALICE).await?;
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    assert!(matches!(
        host.ask(ALICE, apply).await?.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    // A restart during a deployment waits for it, and gives up in time to
    // say who holds the host.
    let plan = host.plan(ALICE).await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let mut deployment = host
        .ask(
            ALICE,
            RemoteMethod::ApplyDeployment {
                remote_plan_hash: plan,
            },
        )
        .await?;
    host.handlers.entered(2).await?;
    let mut waiting = host
        .ask_within(BOB, restart(), Duration::from_millis(4_500))
        .await?;
    let (code, message) = waiting.refusal(Duration::from_secs(10)).await?;
    assert_eq!(code, "MM_HOST_BUSY", "{message}");
    assert!(message.contains("alice's deployment"), "{message}");
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    host.handlers.hold_job.store(false, Ordering::SeqCst);
    assert!(matches!(
        deployment.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    assert!(matches!(job.result(PROMPT).await?, RemoteResult::JobRun(_)));
    Ok(())
}
