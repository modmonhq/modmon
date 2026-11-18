//! What a client that changes a host says while the host works, and when it
//! stops waiting for one.
//!
//! While a step runs on a host, the client asks that host every 15 seconds
//! what runs there (`DeploymentStatus`, a read that takes no turn and waits
//! for nothing). The answer is the heartbeat line: the step, how long it has
//! run, and whether the host's `mmd` still sees it at work, or what it waits
//! behind. A host that does not answer, or whose operation has not been at
//! work, for `deploy.progress-timeout` (two minutes by default) is given up:
//! the client stops with a timeout that names the host, the step and the
//! operation, and the host stops the step when the client's connection ends.

use std::future::Future;
use std::time::Duration;

use super::plans::NodeProgress;
use super::{RemoteEnvironment, RemoteNode};
use crate::domain::error::MmError;
use crate::domain::rollout::{OperationInFlight, ProgressStep, RolloutProgress, short_id};

/// How often a client asks a host how its step is doing, and so the
/// longest a deployment goes without printing a line.
pub const HEARTBEAT: Duration = Duration::from_secs(15);

/// How long a heartbeat waits for the host's answer before it prints its
/// line anyway; the look goes on, and the next heartbeat takes its answer.
/// Without it a host that did not answer (a frozen `mmd`) held the line for
/// a second `HEARTBEAT`.
const LOOK_GRACE: Duration = Duration::from_secs(1);

/// One request whose host is watched while it runs.
pub(super) struct Watched<'a> {
    pub node: &'a RemoteNode,
    /// What the client is doing there, as its lines say: `app`, `preparing`.
    pub step: &'a str,
    /// The operation the request is part of: the plan's hash.
    pub operation: &'a str,
}

/// What one look at the host found.
enum Look {
    /// The operation runs, and the host saw it at work this long ago.
    Working(Option<i64>),
    /// The operation does not run yet: these hold the host.
    Waiting(Vec<OperationInFlight>),
    /// The host did not answer.
    Silent(String),
}

impl RemoteEnvironment {
    /// Runs `work` on `watched.node`, printing a line every `HEARTBEAT`
    /// while it runs, and gives up with a timeout naming host, step, and
    /// operation once the host shows no progress for the environment's
    /// `deploy.progress-timeout`.
    pub(super) async fn with_heartbeat<T>(
        &self,
        watched: Watched<'_>,
        progress: NodeProgress<'_>,
        work: impl Future<Output = Result<T, MmError>>,
    ) -> Result<T, MmError> {
        tokio::pin!(work);
        let started = tokio::time::Instant::now();
        let mut progressed = started;
        let mut seen_alive = None;
        let mut beat = tokio::time::interval_at(started + HEARTBEAT, HEARTBEAT);
        let name = self.name_of(watched.node).to_owned();
        // A look the host has not answered yet, carried to the next beat.
        let mut asking = None;
        loop {
            tokio::select! {
                result = &mut work => return result,
                _ = beat.tick() => {
                    let mut pending = asking
                        .take()
                        .unwrap_or_else(|| Box::pin(look(watched.node, watched.operation)));
                    let look = match tokio::time::timeout(LOOK_GRACE, &mut pending).await {
                        Ok(look) => look,
                        Err(_) => {
                            asking = Some(pending);
                            Look::Silent("no answer yet".to_owned())
                        }
                    };
                    let line = match &look {
                        Look::Working(alive) => {
                            // At work if the host saw it since the last look.
                            let fresh = seen_alive.is_none() || *alive != seen_alive;
                            seen_alive = *alive;
                            if fresh {
                                progressed = tokio::time::Instant::now();
                                "the host is working on it".to_owned()
                            } else {
                                "the host has not seen it at work since the last look".to_owned()
                            }
                        }
                        Look::Waiting(holders) => {
                            progressed = tokio::time::Instant::now();
                            waiting_line(holders)
                        }
                        Look::Silent(why) => format!("the host did not say how it is doing: {why}"),
                    };
                    progress(&RolloutProgress::new(
                        name.clone(),
                        ProgressStep::StillRunning(format!(
                            "{}, still running after {} ({line})",
                            watched.step,
                            elapsed(started.elapsed())
                        )),
                    ));
                    if progressed.elapsed() >= self.progress_timeout {
                        return Err(self.no_progress(&name, &watched, &look));
                    }
                }
            }
        }
    }

    /// The timeout of a host that showed no progress.
    fn no_progress(&self, name: &str, watched: &Watched<'_>, look: &Look) -> MmError {
        let operation = short_id(watched.operation);
        let why = match look {
            Look::Silent(why) => format!("its mmd did not answer ({why})"),
            _ => "its mmd no longer saw the step at work".to_owned(),
        };
        MmError::Timeout {
            message: format!(
                "node '{name}' showed no progress on step '{}' of operation {operation} for {} (deploy.progress-timeout): {why}. mm stopped waiting, and the host stops the step when it sees this client go. Run `mm deploy status` to see where it is, and `mm deploy cancel {operation}` if it still runs",
                watched.step,
                elapsed(self.progress_timeout)
            ),
        }
    }
}

/// Asks the host what runs there for the environment, within a heartbeat.
async fn look(node: &RemoteNode, operation: &str) -> Look {
    let asked = async { node.connect()?.deployment_status(node.scope.clone()).await };
    match tokio::time::timeout(HEARTBEAT, asked).await {
        Err(_) => Look::Silent(format!("no answer within {} s", HEARTBEAT.as_secs())),
        Ok(Err(error)) => Look::Silent(error.to_string()),
        Ok(Ok(status)) => {
            match status
                .operations
                .iter()
                .find(|running| running.id == operation)
            {
                Some(running) => Look::Working(Some(running.alive_at_unix_ms)),
                None => Look::Waiting(status.operations),
            }
        }
    }
}

fn waiting_line(holders: &[OperationInFlight]) -> String {
    match holders.first() {
        Some(holder) => format!("waiting behind {}", holder.describe()),
        None => "the host answers".to_owned(),
    }
}

/// `45s`, `2m05s`.
fn elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    }
}
