//! The one place that starts child processes, and what watches over them.
//!
//! A child is started, its output drained, its deadline enforced, and its
//! process group ended on a thread of its own (`mm-children`), not by the
//! task that asked for it. That task only waits for the result. So a
//! caller that is not being run (a busy or stuck runtime) cannot
//! leave a child blocked on a full pipe or alive past its deadline, and a
//! caller that stops waiting ends the child.

use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::oneshot;

use crate::adapters::own_runtime::OwnRuntime;
use crate::domain::error::MmError;

static CHILDREN: OnceLock<Result<OwnRuntime, String>> = OnceLock::new();

fn children() -> Result<&'static OwnRuntime, MmError> {
    CHILDREN
        .get_or_init(|| OwnRuntime::start("mm-children").map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| MmError::State {
            message: format!("cannot watch over child processes: {message}"),
        })
}

/// The children started through the runner that have not ended yet.
static LISTED: std::sync::Mutex<Vec<ListedChild>> = std::sync::Mutex::new(Vec::new());

/// How long past its deadline a child may take to be gone before it is
/// reported: ending a process group and reaping its leader takes a moment.
const LATE_GRACE_MS: i64 = 10_000;

#[derive(Debug, Clone)]
struct ListedChild {
    serial: u64,
    pid: u32,
    program: String,
    started_at_unix_ms: i64,
    deadline_unix_ms: i64,
}

/// A child's place on the list; it leaves the list when dropped.
pub(super) struct Listed(Option<u64>);

impl Listed {
    /// Lists the child `pid` running `program`, which has `timeout`.
    pub(super) fn child(pid: Option<u32>, program: &str, timeout: Duration) -> Self {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let Some(pid) = pid else {
            return Self(None);
        };
        let now = chrono::Utc::now().timestamp_millis();
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        LISTED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(ListedChild {
                serial,
                pid,
                program: std::path::Path::new(program)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(program)
                    .to_owned(),
                started_at_unix_ms: now,
                deadline_unix_ms: now
                    .saturating_add(i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX)),
            });
        Self(Some(serial))
    }
}

impl Drop for Listed {
    fn drop(&mut self) {
        if let Some(serial) = self.0 {
            LISTED
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retain(|child| child.serial != serial);
        }
    }
}

/// The children still there more than a moment after their deadline, as
/// of `now_unix_ms`. Their supervisor ends them at the deadline; one on
/// this list survived that, or its supervisor is stuck.
#[must_use]
pub fn late_children(now_unix_ms: i64) -> Vec<crate::domain::rollout::LateChild> {
    LISTED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|child| now_unix_ms > child.deadline_unix_ms.saturating_add(LATE_GRACE_MS))
        .map(|child| crate::domain::rollout::LateChild {
            pid: child.pid,
            program: child.program.clone(),
            started_at_unix_ms: child.started_at_unix_ms,
            deadline_unix_ms: child.deadline_unix_ms,
        })
        .collect()
}

/// Whether the thread that watches over child processes still runs.
pub fn supervisor_answers(patience: Duration) -> Result<(), String> {
    children()
        .map_err(|error| error.to_string())?
        .answers(patience)
}

/// The task that asked for a child, as its supervisor sees it.
pub(super) struct Requester(oneshot::Receiver<()>);

impl Requester {
    /// Completes when the task that asked stopped waiting.
    pub(super) async fn gone(&mut self) {
        // The sender is never used: it only goes away with the requester.
        if (&mut self.0).await.is_ok() {
            tracing::debug!("a process requester signalled instead of leaving");
        }
    }
}

/// Runs `work` on the supervisor's thread and waits for its result.
pub(super) async fn supervise<T, W>(
    work: impl FnOnce(Requester) -> W + Send + 'static,
) -> Result<T, MmError>
where
    T: Send + 'static,
    W: Future<Output = Result<T, MmError>> + Send,
{
    let (result, answered) = oneshot::channel();
    let (_waiting, requester) = oneshot::channel();
    children()?.spawn(async move {
        if result.send(work(Requester(requester)).await).is_err() {
            tracing::debug!("a process ended after its requester stopped waiting");
        }
    });
    answered.await.map_err(|_| MmError::State {
        message: "the thread that watches over child processes dropped a process".to_owned(),
    })?
}
