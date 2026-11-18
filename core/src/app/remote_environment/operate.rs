//! What `operate` does on a remote environment: restart a service on the
//! nodes that run it, one node at a time, and run a declared job on the
//! node it belongs to.

use std::time::Duration;

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode, in_node};
use crate::domain::error::MmError;
use crate::domain::observability::{RestartOutcome, RestartRequest};
use crate::domain::scheduler::{JobRun, JobRunRequest, JobRunsRequest, MAX_LISTED_RUNS};

impl RemoteEnvironment {
    /// Restarts a service's containers on every node that runs it, one
    /// node at a time in declared order: each node's containers are
    /// healthy again before the next node's restart. A container named in
    /// the request is restarted on its node alone. The first node that
    /// fails stops the restart; the error says which nodes were restarted
    /// and which were not touched.
    pub async fn restart(&self, request: RestartRequest) -> Result<RestartOutcome, MmError> {
        if !self.is_cluster() {
            let node = self.first();
            return node.connect()?.restart(node.scope.clone(), request).await;
        }
        let service = request.selector.service.clone();
        let nodes = match &request.selector.container {
            Some(container) => vec![self.node_of_container(&service, container).await?],
            None => self
                .nodes
                .iter()
                .filter(|node| {
                    node.placed
                        .as_ref()
                        .is_some_and(|placed| placed.has_service(&service))
                })
                .collect::<Vec<_>>(),
        };
        if nodes.is_empty() {
            self.node_running(&service)?;
        }
        let mut restarted: Option<RestartOutcome> = None;
        for (index, node) in nodes.iter().enumerate() {
            let name = self.name_of(node).to_owned();
            let answer = match node.connect() {
                Ok(transport) => transport.restart(node.scope.clone(), request.clone()).await,
                Err(error) => Err(error),
            };
            let outcome = match answer {
                Ok(outcome) => outcome,
                Err(error) => {
                    let done = restarted.map(|done| done.nodes).unwrap_or_default();
                    let untouched = nodes[index + 1..]
                        .iter()
                        .map(|node| self.name_of(node).to_owned())
                        .collect::<Vec<_>>();
                    return Err(stopped_restart(&name, error, &done, &untouched));
                }
            };
            // A node of a scaled service's placement may run none of it.
            let nodes = if outcome.containers.is_empty() {
                Vec::new()
            } else {
                vec![name]
            };
            restarted = Some(match restarted {
                None => RestartOutcome { nodes, ..outcome },
                Some(mut done) => {
                    done.containers.extend(outcome.containers);
                    done.nodes.extend(nodes);
                    done
                }
            });
        }
        let mut outcome = restarted.ok_or_else(|| MmError::State {
            message: format!("no node restarted service '{service}'"),
        })?;
        outcome.rolling = outcome.nodes.len() > 1;
        Ok(outcome)
    }

    /// The node a container of `service` runs on, from what the nodes
    /// report: an id or a prefix of one.
    pub(super) async fn node_of_container(
        &self,
        service: &str,
        container: &str,
    ) -> Result<&RemoteNode, MmError> {
        let snapshot = self.status().await?;
        let found = snapshot
            .stacks
            .iter()
            .flat_map(|stack| &stack.services)
            .filter(|known| known.name == service)
            .flat_map(|known| &known.containers)
            .filter(|candidate| !candidate.history && candidate.id.starts_with(container))
            .filter_map(|candidate| candidate.node.as_deref())
            .collect::<Vec<_>>();
        let node = match found.as_slice() {
            [node] => *node,
            [] => {
                return Err(MmError::Environment {
                    message: format!(
                        "no node runs a container {container} of service '{service}'; `mm service status {service}` lists its containers"
                    ),
                });
            }
            _ => {
                return Err(MmError::Environment {
                    message: format!(
                        "{container} names more than one container of service '{service}'; give more of its id"
                    ),
                });
            }
        };
        self.nodes
            .iter()
            .find(|candidate| candidate.name.as_deref() == Some(node))
            .ok_or_else(|| MmError::Target {
                message: format!("node '{node}' is not a node of target '{}'", self.target),
            })
    }

    /// Runs a declared job on `node`, the node of a cluster target the job
    /// runs on (`placement::job_node_on`), or on the single host when
    /// `None`. `patience` is how long the answer may take.
    pub async fn run_job(
        &self,
        node: Option<&str>,
        request: JobRunRequest,
        patience: Duration,
    ) -> Result<JobRun, MmError> {
        let node = match node {
            Some(name) => self
                .nodes
                .iter()
                .find(|candidate| candidate.name.as_deref() == Some(name))
                .ok_or_else(|| MmError::Target {
                    message: format!("node '{name}' is not a node of target '{}'", self.target),
                })?,
            None => self.first(),
        };
        let mut run = node
            .connect()?
            .run_job(node.scope.clone(), request, patience)
            .await
            .map_err(|error| in_node(node.name.as_deref(), error))?;
        run.node.clone_from(&node.name);
        Ok(run)
    }

    /// The job runs every node recorded, newest first, each labelled with
    /// its node.
    pub async fn job_runs(&self, request: JobRunsRequest) -> Result<Vec<JobRun>, MmError> {
        let asked = self.nodes.iter().collect::<Vec<_>>();
        let answers = join_all(asked.iter().map(|node| {
            let request = request.clone();
            async move {
                let mut runs = node
                    .connect()?
                    .job_runs(node.scope.clone(), request)
                    .await?;
                for run in &mut runs {
                    run.node.clone_from(&node.name);
                }
                Ok(runs)
            }
        }))
        .await;
        let mut runs = self
            .all_or_explain(&asked, answers)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| {
            right
                .requested_at_unix_ms
                .cmp(&left.requested_at_unix_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        runs.truncate(request.limit.clamp(1, MAX_LISTED_RUNS) as usize);
        Ok(runs)
    }
}

impl RemoteEnvironment {
    /// The runs deployments started after `since_unix_ms`, newest first: what
    /// a rollback to the release deployed then does not undo.
    pub async fn deployment_jobs_since(&self, since_unix_ms: i64) -> Result<Vec<JobRun>, MmError> {
        let request = JobRunsRequest {
            job: None,
            run: None,
            limit: MAX_LISTED_RUNS,
        };
        Ok(self
            .job_runs(request)
            .await?
            .into_iter()
            .filter(|run| {
                run.source.starts_with("deployment ") && run.requested_at_unix_ms > since_unix_ms
            })
            .collect())
    }
}

/// A rolling restart that stopped at `node`: what failed, which nodes were
/// restarted before it, and which it did not reach. The node's own kind of
/// error stays: a node that refuses the restart is a refusal (MM_SECURITY).
fn stopped_restart(node: &str, error: MmError, done: &[String], untouched: &[String]) -> MmError {
    let mut tail = String::new();
    if !done.is_empty() {
        tail.push_str(&format!(
            ". Restarted and healthy before it: {}",
            done.join(", ")
        ));
    }
    if !untouched.is_empty() {
        tail.push_str(&format!(". Not restarted: {}", untouched.join(", ")));
    }
    let said = |text: &str| format!("node '{node}': {text}{tail}");
    let shown = error.to_string();
    match error {
        MmError::Unreachable { subject, help, .. } => MmError::Unreachable {
            subject,
            problem: said(&shown),
            help,
        },
        MmError::Security { message } => MmError::Security {
            message: said(&message),
        },
        MmError::RuntimeResponse { message } => MmError::RuntimeResponse {
            message: said(&message),
        },
        MmError::Remote { code, message } => MmError::Remote {
            code,
            message: said(&message),
        },
        // `mm deploy cancel` stopped it, or the node gave up on it.
        MmError::Cancelled { message } => MmError::Cancelled {
            message: said(&message),
        },
        MmError::Timeout { message } => MmError::Timeout {
            message: said(&message),
        },
        _ => MmError::RuntimeResponse {
            message: said(&shown),
        },
    }
}
