use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

use crate::domain::error::MmError;
use crate::domain::workspace::WorkspaceSnapshot;
use crate::presentation::tui::model::LiveStatus;

/// How often the TUI re-reads workspace, environment, TLS, and service status.
pub(super) const REFRESH_INTERVAL: Duration = Duration::from_secs(3);

/// The refresh interval for a remote environment, where each refresh asks
/// every host for its status.
pub(super) const REMOTE_REFRESH_INTERVAL: Duration = Duration::from_secs(10);

/// Reads current status for the TUI. Implementations must not mutate state.
#[async_trait]
pub trait LiveStatusSource: Send + Sync {
    /// The status now. On a cluster `so_far` is told the status each time a
    /// node answers, so one slow node does not hold back the others.
    async fn refresh(&self, so_far: crate::app::StatusSoFar<'_>) -> Result<LiveStatus, MmError>;
}

/// What a refresh delivers: the status as far as the nodes answered, then
/// the whole of it.
pub(super) enum LiveUpdate {
    SoFar(Box<WorkspaceSnapshot>),
    Done(Box<Result<LiveStatus, String>>),
}

/// Refreshes status on a background task so input never waits on the runtime.
pub(super) struct LiveRefresh {
    receiver: mpsc::UnboundedReceiver<LiveUpdate>,
    trigger: Arc<Notify>,
    task: JoinHandle<()>,
}

impl LiveRefresh {
    pub(super) fn start(source: Arc<dyn LiveStatusSource>, interval: Duration) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let trigger = Arc::new(Notify::new());
        let wake = Arc::clone(&trigger);
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = tokio::time::sleep(interval) => {}
                    () = wake.notified() => {}
                }
                let partial = sender.clone();
                let so_far = move |snapshot: WorkspaceSnapshot| {
                    if partial.send(LiveUpdate::SoFar(Box::new(snapshot))).is_err() {
                        tracing::debug!("TUI closed while a refresh was read");
                    }
                };
                let result = source
                    .refresh(&so_far)
                    .await
                    .map_err(|error| crate::presentation::output::error_line(&error));
                if sender.send(LiveUpdate::Done(Box::new(result))).is_err() {
                    break;
                }
            }
        });
        Self {
            receiver,
            trigger,
            task,
        }
    }

    /// Wakes the refresh task now instead of at the next interval.
    pub(super) fn trigger(&self) -> Arc<Notify> {
        Arc::clone(&self.trigger)
    }

    pub(super) fn try_next(&mut self) -> Option<LiveUpdate> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for LiveRefresh {
    fn drop(&mut self) {
        self.task.abort();
    }
}
