//! What a host answers about an environment whose first deployment did not
//! finish: its reads take the checkout of the attempt, and what it still
//! refuses it refuses with `MM_NOT_DEPLOYED`.

mod daemon_host;
mod temp_dirs;

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult, exec_request, project};
use mm::domain::observability::{ApprovalContext, LogRequest, ServiceSelector};
use mm::domain::remote::{RemoteMethod, RemoteResult};
use mm::domain::rollout::RolloutStep;
use mm::domain::scheduler::JobRunsRequest;

fn job_runs() -> RemoteMethod {
    RemoteMethod::JobRuns(JobRunsRequest {
        job: None,
        run: None,
        limit: 20,
    })
}

fn web_log() -> RemoteMethod {
    RemoteMethod::Logs(LogRequest::new(
        ServiceSelector {
            stack: None,
            module: None,
            service: "web".to_owned(),
            container: None,
        },
        ApprovalContext {
            principal: "forged".to_owned(),
            source: "forged".to_owned(),
            approved_high_risk: false,
        },
    ))
}

fn never_deployed(host: &Host) -> TestResult {
    std::fs::remove_file(
        host.config
            .state_root
            .join("projects")
            .join(project().0.to_string())
            .join("deployed/production.json"),
    )?;
    Ok(())
}

async fn refused_as_not_deployed(host: &Host, key: &str, method: RemoteMethod) -> TestResult {
    let (code, message) = host.ask(key, method).await?.refusal(PROMPT).await?;
    assert_eq!(code, "MM_NOT_DEPLOYED", "{message}");
    assert_eq!(
        message,
        "nothing is deployed to 'production' on this host yet; run `mm deploy plan --environment production`"
    );
    Ok(())
}

#[tokio::test]
async fn a_host_that_never_began_a_deployment_reads_no_job_runs_and_refuses_the_rest() -> TestResult
{
    let host = Host::with_alice().await?;
    never_deployed(&host)?;

    match host.ask(OPS, job_runs()).await?.result(PROMPT).await? {
        RemoteResult::JobRuns(runs) => assert!(runs.is_empty(), "{runs:?}"),
        other => return Err(format!("job runs returned {other:?}").into()),
    }
    refused_as_not_deployed(&host, OPS, RemoteMethod::Status).await?;
    refused_as_not_deployed(&host, OPS, web_log()).await?;
    refused_as_not_deployed(&host, ALICE, RemoteMethod::Exec(exec_request())).await?;
    Ok(())
}

#[tokio::test]
async fn a_failed_first_deployment_is_read_from_the_checkout_it_began_with() -> TestResult {
    let host = Host::with_alice().await?;
    never_deployed(&host)?;
    let plan = host.plan(ALICE).await?;
    for step in [RolloutStep::Hold, RolloutStep::Begin, RolloutStep::Rollback] {
        let answer = host
            .ask(
                ALICE,
                RemoteMethod::Rollout {
                    plan_hash: plan.clone(),
                    step,
                },
            )
            .await?
            .result(PROMPT)
            .await?;
        assert!(matches!(answer, RemoteResult::Rollout(_)), "{answer:?}");
    }
    let checkout = host
        .config
        .projects_root
        .join(project().0.to_string())
        .join("production")
        .join(&host.commit[..12]);

    let snapshot = match host
        .ask(OPS, RemoteMethod::Status)
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::Status(snapshot) => snapshot,
        other => return Err(format!("status returned {other:?}").into()),
    };
    assert_eq!(snapshot.workspace.root, checkout);
    assert_eq!(snapshot.deployment, None, "nothing finished here");

    match host.ask(OPS, job_runs()).await?.result(PROMPT).await? {
        RemoteResult::JobRuns(runs) => assert_eq!(runs.len(), 1, "{runs:?}"),
        other => return Err(format!("job runs returned {other:?}").into()),
    }
    assert!(matches!(
        host.ask(OPS, web_log()).await?.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    refused_as_not_deployed(&host, ALICE, RemoteMethod::Exec(exec_request())).await?;
    Ok(())
}
