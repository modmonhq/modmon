//! A daemon that stops answering is noticed, and a daemon that starts
//! after one that was in the middle of a rollout says so.

mod daemon_host;
mod temp_dirs;

use std::time::{Duration, Instant};

use daemon_host::{Host, OPS, PROMPT, TestResult, project};
use mm::adapters::remote::Liveness;
use mm::domain::remote::{RemoteMethod, RemoteResult};
use mm::domain::rollout::RolloutState;
use tokio::net::UnixDatagram;

/// The next notification the service manager gets, within `patience`.
async fn notified(socket: &UnixDatagram, patience: Duration) -> Option<String> {
    let mut buffer = [0_u8; 512];
    match tokio::time::timeout(patience, socket.recv(&mut buffer)).await {
        Ok(Ok(length)) => Some(String::from_utf8_lossy(&buffer[..length]).into_owned()),
        _ => None,
    }
}

/// Reads notifications for `time` and returns how many were keep-alives.
async fn keep_alives_within(socket: &UnixDatagram, time: Duration) -> usize {
    let until = Instant::now() + time;
    let mut count = 0;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match notified(socket, left).await {
            Some(notification) if notification.starts_with("WATCHDOG=1") => count += 1,
            Some(_) => {}
            None => break,
        }
    }
    count
}

#[tokio::test]
async fn the_keep_alive_is_sent_only_while_requests_are_answered() -> TestResult {
    let host = Host::new()?;
    let notify = host.temporary.path().join("notify.sock");
    let manager = UnixDatagram::bind(&notify)?;
    let watchdog = Duration::from_millis(900);
    let daemon = host.next_daemon()?;
    let serving = host.on_daemon(daemon.run_reporting(Liveness::to(Some(notify), Some(watchdog))));

    let ready = notified(&manager, PROMPT)
        .await
        .ok_or("the daemon never said it was ready")?;
    assert!(ready.starts_with("READY=1\n"), "{ready}");
    assert!(
        keep_alives_within(&manager, Duration::from_millis(1_500)).await >= 3,
        "a daemon that answers reports itself alive several times per interval"
    );

    // The one thread that serves requests is held, as it was by a lock it
    // waited for in the kernel. The process is alive; requests are not
    // answered.
    let blocked = host.on_daemon(async { std::thread::sleep(Duration::from_millis(2_500)) });
    // A keep-alive that was already on its way.
    keep_alives_within(&manager, Duration::from_millis(500)).await;
    assert_eq!(
        keep_alives_within(&manager, Duration::from_millis(1_500)).await,
        0,
        "a daemon that answers no request reported itself alive"
    );
    blocked.await?;
    assert!(
        keep_alives_within(&manager, Duration::from_millis(2_000)).await >= 1,
        "a daemon that answers again reports itself alive again"
    );
    serving.abort();
    Ok(())
}

/// Writes what a host remembers of a rollout an earlier daemon was in
/// the middle of: begun or only holding the lock, its client still there.
fn rollout_in_progress(host: &Host, state: &str) -> TestResult {
    let now = chrono::Utc::now().timestamp_millis();
    let directory = host
        .config
        .state_root
        .join("projects")
        .join(project().0.to_string())
        .join("deployments");
    std::fs::create_dir_all(&directory)?;
    std::fs::write(
        directory.join("production.json"),
        serde_json::to_vec(&serde_json::json!({
            "rollout": {
                "plan-hash": "c".repeat(64),
                "commit": "d".repeat(40),
                "principal-id": uuid::Uuid::from_u128(77),
                "started-by": "alice",
                "started-at-unix-ms": now - 60_000,
                "heard-at-unix-ms": now - 5_000,
                "expires-at-unix-ms": now + 600_000,
                "state": state,
                "stacks": ["app"],
            }
        }))?,
    )?;
    Ok(())
}

async fn deployment_status(host: &Host) -> TestResult<mm::domain::rollout::NodeDeploymentStatus> {
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

#[tokio::test]
async fn a_daemon_that_starts_marks_the_rollout_it_finds_in_progress_as_interrupted() -> TestResult
{
    let host = Host::new()?;
    rollout_in_progress(&host, "running")?;
    let rollout = deployment_status(&host)
        .await?
        .rollout
        .ok_or("no rollout")?;
    assert!(
        rollout.active,
        "before the restart the step's lease holds the environment"
    );

    host.next_daemon()?.resume_after_restart()?;
    let rollout = deployment_status(&host)
        .await?
        .rollout
        .ok_or("no rollout")?;
    assert_eq!(rollout.state, RolloutState::Running);
    assert!(!rollout.active, "the lease is given up at once");
    assert!(rollout.unfinished());
    let reason = rollout
        .failure
        .ok_or("the interruption is not reported")?
        .reason;
    assert!(
        reason.contains("mmd on this node restarted at 20")
            && reason.contains("interrupted the step in progress"),
        "{reason}"
    );
    let interrupted = host
        .audit()?
        .into_iter()
        .find(|record| record["event"] == "rollout-interrupted")
        .ok_or("the interruption is not in the audit log")?;
    assert_eq!(interrupted["began"], true);
    assert_eq!(interrupted["plan_hash"], "c".repeat(64));

    // A daemon that starts again finds nothing in progress.
    host.next_daemon()?.resume_after_restart()?;
    let events = host.audit()?;
    assert_eq!(
        events
            .iter()
            .filter(|record| record["event"] == "rollout-interrupted")
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn a_daemon_that_starts_frees_an_environment_that_was_only_held() -> TestResult {
    let host = Host::new()?;
    rollout_in_progress(&host, "held")?;
    assert!(
        deployment_status(&host)
            .await?
            .rollout
            .is_some_and(|rollout| rollout.active)
    );
    host.next_daemon()?.resume_after_restart()?;
    let rollout = deployment_status(&host)
        .await?
        .rollout
        .ok_or("no rollout")?;
    assert_eq!(rollout.state, RolloutState::Held);
    assert!(!rollout.active);
    assert!(
        rollout.failure.is_none(),
        "nothing had begun, so nothing was interrupted"
    );
    Ok(())
}
