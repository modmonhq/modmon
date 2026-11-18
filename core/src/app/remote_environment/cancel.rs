//! `mm deploy cancel`: stops what runs for the environment on every host,
//! returns the hosts a deployment changed to what they ran before, and
//! frees the environment. A host acts on its own part, so every host
//! is asked at once, and one that does not answer is named.

use futures_util::future::join_all;

use super::RemoteEnvironment;
use crate::domain::error::MmError;
use crate::domain::rollout::{CancelReport, NodeCancel};

impl RemoteEnvironment {
    /// Cancels `operation` (its ID or the start of it), or whatever runs.
    /// The report says what each host did; the error, when there is one,
    /// is the first host's that did not answer.
    pub async fn cancel(&self, operation: Option<&str>) -> (CancelReport, Option<MmError>) {
        let answers = join_all(self.nodes.iter().map(|node| async move {
            let answer = match node.connect() {
                Ok(transport) => {
                    transport
                        .cancel(node.scope.clone(), operation.map(str::to_owned))
                        .await
                }
                Err(error) => Err(error),
            };
            (self.name_of(node).to_owned(), answer)
        }))
        .await;
        let mut first = None;
        let nodes = answers
            .into_iter()
            .map(|(node, answer)| match answer {
                Ok(outcome) => NodeCancel {
                    node,
                    outcome: Some(outcome),
                    problem: None,
                },
                Err(error) => {
                    let problem = error.to_string();
                    first.get_or_insert(error);
                    NodeCancel {
                        node,
                        outcome: None,
                        problem: Some(problem),
                    }
                }
            })
            .collect();
        let report = CancelReport {
            target: self.target.clone(),
            environment: self.first().scope.environment.clone(),
            operation: operation.map(str::to_owned),
            nodes,
        };
        (report, first)
    }
}
