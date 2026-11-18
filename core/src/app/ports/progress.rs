use std::future::Future;

use crate::domain::error::MmError;
use crate::domain::progress::{NodeState, ProgressEvent, StepOutcome};

/// Hears what a use case is doing while it works.
pub trait ProgressReporter: Send + Sync {
    fn report(&self, event: ProgressEvent);
}

/// Hears nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl ProgressReporter for NoProgress {
    fn report(&self, _event: ProgressEvent) {}
}

/// Runs `work` as the step `step`: says it started, then how it ended.
pub async fn step<T>(
    progress: &dyn ProgressReporter,
    (step, label): (&str, &str),
    work: impl Future<Output = Result<T, MmError>>,
    summary: impl FnOnce(&T) -> Option<String>,
) -> Result<T, MmError> {
    progress.report(ProgressEvent::started(step, label));
    let result = work.await;
    progress.report(match &result {
        Ok(value) => ProgressEvent::finished(step, StepOutcome::Done, summary(value)),
        Err(_) => ProgressEvent::finished(step, StepOutcome::Failed, None),
    });
    result
}

/// Runs `work` for one node of the step `step`: the node is working, then done or failed.
pub async fn on_node<T>(
    progress: &dyn ProgressReporter,
    (step, node, detail): (&str, &str, &str),
    work: impl Future<Output = Result<T, MmError>>,
) -> Result<T, MmError> {
    progress.report(ProgressEvent::node(
        step,
        node,
        NodeState::Working,
        Some(detail),
    ));
    let result = work.await;
    progress.report(ProgressEvent::node(
        step,
        node,
        if result.is_ok() {
            NodeState::Done
        } else {
            NodeState::Failed
        },
        None,
    ));
    result
}
