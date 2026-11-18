//! What a host's daemon answers a read with: a status carries the
//! deployment the host runs, reads do not wait for a change to the host,
//! and a client may send its request before it has heard the hello.

mod daemon_host;
mod temp_dirs;

use std::sync::atomic::Ordering;

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult};
use mm::domain::observability::{ApprovalContext, LogRequest, ServiceSelector};
use mm::domain::remote::{RemoteMethod, RemoteResult, ServerFrame};
use mm::domain::workspace::{AppliedDeployment, WorkspaceSnapshot};

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

async fn status(host: &Host) -> TestResult<WorkspaceSnapshot> {
    match host
        .ask(OPS, RemoteMethod::Status)
        .await?
        .result(PROMPT)
        .await?
    {
        RemoteResult::Status(snapshot) => Ok(*snapshot),
        other => Err(format!("status returned {other:?}").into()),
    }
}

#[tokio::test]
async fn a_status_carries_the_deployment_the_host_last_finished() -> TestResult {
    let host = Host::with_alice().await?;
    // The host of the fixture was deployed before deployments were
    // remembered: it knows the commit, when, and who.
    assert_eq!(
        status(&host).await?.deployment,
        Some(AppliedDeployment {
            commit: "a".repeat(40),
            deployed_at_unix_ms: 1,
            deployed_by: "test".to_owned(),
            plan_hash: None,
            build_receipt: None,
        })
    );

    let plan = host.plan(ALICE).await?;
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    assert!(matches!(
        host.ask(ALICE, apply).await?.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    let deployment = status(&host)
        .await?
        .deployment
        .ok_or("the status names no deployment")?;
    assert_eq!(deployment.commit, host.commit);
    assert_eq!(deployment.deployed_by, "alice");
    Ok(())
}

#[tokio::test]
async fn a_status_and_a_log_read_are_answered_while_a_deployment_has_the_hosts_turn() -> TestResult
{
    let host = Host::with_alice().await?;
    let plan = host.plan(ALICE).await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    let mut deployment = host.ask(ALICE, apply).await?;
    host.handlers.entered(1).await?;

    // The status is of the deployment that is finished, not the one
    // under way.
    assert_eq!(
        status(&host)
            .await?
            .deployment
            .map(|deployment| deployment.commit),
        Some("a".repeat(40))
    );
    let mut log = host.ask(OPS, web_log()).await?;
    assert!(matches!(
        log.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    assert!(matches!(
        log.next(PROMPT).await?,
        ServerFrame::End { frames: 0, .. }
    ));
    assert_eq!(
        host.handlers.inside.load(Ordering::SeqCst),
        1,
        "the deployment still has the host's turn"
    );

    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        deployment.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    Ok(())
}

#[tokio::test]
async fn a_request_sent_with_the_hello_is_answered_after_the_hello() -> TestResult {
    let host = Host::new()?;
    let nonce = uuid::Uuid::new_v4();
    let mut client = host.ask_at_once(OPS, nonce, RemoteMethod::Status).await?;
    let hello = client.next(PROMPT).await?;
    assert!(
        matches!(&hello, ServerFrame::Hello { hello, .. } if hello.client_nonce == nonce),
        "{hello:?}"
    );
    assert!(matches!(
        client.result(PROMPT).await?,
        RemoteResult::Status(_)
    ));

    // A stream begun the same way: its result, then its end.
    let mut log = host.ask_at_once(OPS, nonce, web_log()).await?;
    assert!(matches!(log.next(PROMPT).await?, ServerFrame::Hello { .. }));
    assert!(matches!(
        log.result(PROMPT).await?,
        RemoteResult::StreamStarted
    ));
    assert!(matches!(log.next(PROMPT).await?, ServerFrame::End { .. }));
    Ok(())
}
