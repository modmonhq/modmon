//! Asking every node at once while saying where each one is.

use std::future::Future;

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode};
use crate::app::ports::{ProgressReporter, progress_on_node};
use crate::domain::error::MmError;
use crate::domain::progress::{NodeState, ProgressEvent};

/// The steps a remote plan reports.
pub(super) const DELIVER: &str = "deliver";
pub(super) const PLAN: &str = "plan";
pub(super) const SEAL: &str = "seal";

impl RemoteEnvironment {
    /// Runs `ask` for every node of `selected` at once.
    pub(super) async fn ask_all<'a, T, Fut>(
        &'a self,
        (step, detail): (&str, &str),
        selected: &[&'a RemoteNode],
        progress: &dyn ProgressReporter,
        ask: impl Fn(&'a RemoteNode) -> Fut,
    ) -> Vec<Result<T, MmError>>
    where
        Fut: Future<Output = Result<T, MmError>>,
    {
        join_all(
            selected.iter().map(|node| {
                progress_on_node(progress, (step, self.name_of(node), detail), ask(node))
            }),
        )
        .await
    }

    /// Marks the nodes a plan leaves out.
    pub(super) fn report_left_out(
        &self,
        step: &str,
        skipped: &[String],
        progress: &dyn ProgressReporter,
    ) {
        for node in skipped {
            progress.report(ProgressEvent::node(
                step,
                node,
                NodeState::Skipped,
                Some("left out; keeps what it runs"),
            ));
        }
    }
}

/// `1 node`, `3 nodes`.
pub(super) fn nodes(count: usize) -> String {
    format!("{count} node{}", if count == 1 { "" } else { "s" })
}

/// What a failed first deployment left on the nodes, and what to run next.
pub(super) fn kept_first(environment: &str, service: Option<&str>) -> String {
    let service = service.unwrap_or("SERVICE");
    format!(
        ". Nothing ran before this first deployment of '{environment}', so nothing was rolled back: every node keeps the stacks it started as they are, the one that failed too. `mm service status --environment {environment}` shows each container and `mm service logs {service} --environment {environment}` its log; fix what failed, then `mm deploy plan --environment {environment}` deploys over them"
    )
}
