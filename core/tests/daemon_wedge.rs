//! A second mutation cannot wedge the daemon. Before, every mutation took
//! a blocking file lock on a runtime thread and kept it for the whole
//! request: a second mutation on the same daemon stopped the thread in the
//! kernel, and on a host with one such thread nothing was answered again.

mod daemon_host;
mod temp_dirs;

use std::sync::atomic::Ordering;
use std::time::Duration;

use daemon_host::{ALICE, Host, OPS, PROMPT, TestResult, exec_request};
use mm::domain::remote::{RemoteMethod, RemoteResult, RemoteStreamItem, ServerFrame};

/// The reproduction: a deployment is held open inside its handler, a
/// second mutation arrives on the same daemon, and then a read.
#[tokio::test]
async fn a_second_mutation_and_a_read_are_answered_while_a_mutation_is_held_open() -> TestResult {
    let host = Host::with_alice().await?;
    let plan = host.plan(ALICE).await?;
    host.handlers.hold_apply.store(true, Ordering::SeqCst);
    let apply = RemoteMethod::ApplyDeployment {
        remote_plan_hash: plan,
    };
    let mut first = host.ask(ALICE, apply).await?;
    host.handlers.entered(1).await?;

    // The second mutation: a command in a container.
    let mut second = host.ask(OPS, RemoteMethod::Exec(exec_request())).await?;
    let started = second.next(PROMPT).await.map_err(|error| {
        format!("the second mutation got no answer while the first was held open: {error}")
    })?;
    assert!(
        matches!(started, ServerFrame::Result { ref result, .. } if matches!(**result, RemoteResult::StreamStarted)),
        "unexpected first frame of the command: {started:?}"
    );
    assert!(matches!(
        second.next(PROMPT).await?,
        ServerFrame::Stream { item, .. } if matches!(*item, RemoteStreamItem::Exec(_))
    ));
    assert!(matches!(
        second.next(PROMPT).await?,
        ServerFrame::End { frames: 1, .. }
    ));

    // The read.
    let status = host
        .ask(ALICE, RemoteMethod::Status)
        .await?
        .result(PROMPT)
        .await
        .map_err(|error| format!("the read got no answer after the second mutation: {error}"))?;
    assert!(matches!(status, RemoteResult::Status(_)));

    // The first mutation was not disturbed and finishes when its handler does.
    assert!(first.is_silent(Duration::from_millis(100)).await);
    host.handlers.hold_apply.store(false, Ordering::SeqCst);
    assert!(matches!(
        first.result(PROMPT).await?,
        RemoteResult::DeploymentApplied(_)
    ));
    assert_eq!(host.deployed_commit()?, host.commit);
    Ok(())
}
