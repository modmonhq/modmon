//! Status, logs, and commands across the nodes of a remote environment.
//!
//! A service is all of its containers on all of its nodes. Every node is
//! asked at once and answers for itself; a node that does not answer is
//! named, and the others' answers are not held back for it for long.

use std::time::Duration;

use futures_util::StreamExt as _;
use futures_util::stream::FuturesUnordered;

use super::{RemoteEnvironment, RemoteNode, in_node};
use crate::app::ports::{ExecStream, LogStream};
use crate::domain::error::MmError;
use crate::domain::observability::{ExecRequest, LogRequest, OperationCancellation};
use crate::domain::runtime::{ServiceOnNode, ServiceSnapshot, ServiceState, state_across_nodes};
use crate::domain::workspace::{NodeStatus, StackSnapshot, WorkspaceSnapshot};

/// How long the nodes that have not answered a read are waited for once
/// the first node has. A node that takes longer is listed as silent, so a
/// stuck daemon on one node costs a read a few seconds, not its timeout.
pub const SILENT_NODE_PATIENCE: Duration = Duration::from_secs(5);

/// Called with the status so far each time a node of a cluster answers.
/// A node that has not answered yet is listed as not reachable with no
/// problem (`NodeStatus::is_pending`). The last word is the result of
/// `status_reporting`, never a call to this.
pub type StatusSoFar<'a> = &'a (dyn Fn(WorkspaceSnapshot) + Send + Sync);

/// What one node of a cluster said when asked for its status.
#[derive(Debug, Clone)]
enum Answer {
    Pending,
    Answered(Box<WorkspaceSnapshot>),
    /// The node did not answer or refused: the row says why.
    Failed(String),
}

/// Each node's answer to the last status read, kept by a value that is
/// asked again and again, such as the terminal UI's. Until a node answers
/// a new read, what it said last stands for it, so a refresh shows each
/// node as it answers without the others flickering to "not answered yet".
#[derive(Debug, Clone, Default)]
pub(super) struct LastStatus(std::sync::Arc<std::sync::Mutex<Vec<Answer>>>);

impl LastStatus {
    fn get(&self, nodes: usize) -> Vec<Answer> {
        let kept = match self.0.lock() {
            Ok(kept) => kept.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        if kept.len() == nodes {
            kept
        } else {
            vec![Answer::Pending; nodes]
        }
    }

    fn set(&self, answers: &[Answer]) {
        match self.0.lock() {
            Ok(mut kept) => *kept = answers.to_vec(),
            Err(poisoned) => *poisoned.into_inner() = answers.to_vec(),
        }
    }
}

impl RemoteEnvironment {
    /// The environment's status. On a cluster every node is asked at once
    /// and each service row carries the node it runs on; a node that does
    /// not answer is listed with the reason instead of failing the status.
    pub async fn status(&self) -> Result<WorkspaceSnapshot, MmError> {
        self.status_reporting(None).await
    }

    /// `status`, telling `so_far` what is known each time a node answers,
    /// so a slow or silent node does not hold back the others.
    pub async fn status_reporting(
        &self,
        so_far: Option<StatusSoFar<'_>>,
    ) -> Result<WorkspaceSnapshot, MmError> {
        if !self.is_cluster() {
            let node = self.first();
            return node.connect()?.status(node.scope.clone()).await;
        }
        let mut asked = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| async move {
                let answer = match node.connect() {
                    Ok(transport) => transport.status(node.scope.clone()).await,
                    Err(error) => Err(error),
                };
                (index, answer)
            })
            .collect::<FuturesUnordered<_>>();
        // A node that has not answered this read yet shows what it said
        // to the last one; on the first read, that it has not answered.
        let mut answers = self.last_status.get(self.nodes.len());
        let mut errors = self.nodes.iter().map(|_| None).collect::<Vec<_>>();
        let mut answered = vec![false; self.nodes.len()];
        let mut waiting = self.nodes.len();
        // Once a node has answered, the others get this long.
        let mut give_up = None::<tokio::time::Instant>;
        while waiting > 0 {
            let next = tokio::select! {
                next = asked.next() => next,
                () = async {
                    match give_up {
                        Some(at) => tokio::time::sleep_until(at).await,
                        None => std::future::pending().await,
                    }
                } => None,
            };
            let Some((index, answer)) = next else {
                break;
            };
            waiting -= 1;
            answered[index] = true;
            answers[index] = match answer {
                Ok(snapshot) => {
                    give_up
                        .get_or_insert_with(|| tokio::time::Instant::now() + SILENT_NODE_PATIENCE);
                    Answer::Answered(Box::new(snapshot))
                }
                Err(error) => {
                    // The row already names the node: say what is wrong
                    // and what to do, as `mm target nodes` does.
                    let problem = match &error {
                        MmError::Unreachable { problem, help, .. } => {
                            format!("{problem}. {help}")
                        }
                        other => other.to_string(),
                    };
                    errors[index] = Some(in_node(self.nodes[index].name.as_deref(), error));
                    Answer::Failed(problem)
                }
            };
            if waiting > 0
                && let Some(so_far) = so_far
                && let Some(snapshot) = self.merged(&answers)
            {
                so_far(snapshot);
            }
        }
        // A node that did not answer this read is silent, whatever it said
        // before.
        for (answer, fresh) in answers.iter_mut().zip(&answered) {
            if !fresh {
                *answer = Answer::Failed(format!(
                    "has not answered after {} seconds. Check `systemctl status mmd` on the node; `mm target nodes` asks it again",
                    SILENT_NODE_PATIENCE.as_secs()
                ));
            }
        }
        self.last_status.set(&answers);
        // When no node answered, the first one in declared order says why.
        match (self.merged(&answers), errors.into_iter().flatten().next()) {
            (Some(snapshot), _) => Ok(snapshot),
            (None, Some(error)) => Err(error),
            (None, None) => Err(MmError::Target {
                message: format!("target '{}' lists no node", self.target),
            }),
        }
    }

    /// One snapshot from the nodes' answers, in the target's declared
    /// order; `None` while no node has answered with one. Each service is
    /// listed once, with what every node says about it.
    fn merged(&self, answers: &[Answer]) -> Option<WorkspaceSnapshot> {
        let mut merged: Option<WorkspaceSnapshot> = None;
        let mut nodes = Vec::with_capacity(self.nodes.len());
        let mut names = Vec::new();
        for (node, answer) in self.nodes.iter().zip(answers) {
            let name = self.name_of(node).to_owned();
            let Answer::Answered(snapshot) = answer else {
                nodes.push(NodeStatus {
                    problem: match answer {
                        Answer::Failed(problem) => Some(problem.clone()),
                        _ => None,
                    },
                    ..NodeStatus::pending(name)
                });
                continue;
            };
            let mut snapshot = snapshot.as_ref().clone();
            nodes.push(NodeStatus {
                name: name.clone(),
                reachable: true,
                problem: None,
                network_drift: std::mem::take(&mut snapshot.network_drift),
                deployment: snapshot.deployment.take(),
                drift: std::mem::take(&mut snapshot.drift),
                tunnel: snapshot.tunnel.take(),
            });
            // What this node's resolver answers; the views of all nodes
            // become one report below.
            names.extend(snapshot.names.take().map(|view| (name.clone(), view)));
            merged = Some(match merged {
                None => labelled(snapshot, &name),
                Some(mut merged) => {
                    merge_stacks(&mut merged.stacks, labelled(snapshot, &name).stacks);
                    merged
                }
            });
        }
        let mut snapshot = merged?;
        for service in snapshot
            .stacks
            .iter_mut()
            .flat_map(|stack| stack.services.iter_mut())
        {
            leave_out_idle_nodes(service);
        }
        // One receipt for the environment when every node that answered
        // runs the same release.
        let mut deployments = nodes.iter().filter(|node| node.reachable);
        snapshot.deployment = deployments
            .next()
            .and_then(|first| first.deployment.clone())
            .filter(|first| {
                deployments.all(|node| {
                    node.deployment
                        .as_ref()
                        .is_some_and(|other| other.same_release(first))
                })
            });
        self.add_unanswered_shares(&mut snapshot.stacks, answers);
        snapshot.nodes = nodes;
        snapshot.names =
            (!names.is_empty()).then(|| crate::domain::discovery::NameReport::merged(names));
        Some(snapshot)
    }

    /// Gives every service this checkout places on a node that did not
    /// answer, or has not answered yet, a share there whose state nobody
    /// knows: one container wanted, none seen. Without it a service would
    /// read as whole from the nodes that did answer.
    fn add_unanswered_shares(&self, stacks: &mut [StackSnapshot], answers: &[Answer]) {
        for (node, answer) in self.nodes.iter().zip(answers) {
            let (Answer::Failed(_) | Answer::Pending, Some(placed)) = (answer, &node.placed) else {
                continue;
            };
            for placed in placed.iter() {
                let Some(stack) = stacks.iter_mut().find(|stack| stack.name == placed.stack) else {
                    continue;
                };
                let share = unanswered(&placed.stack, &placed.service, self.name_of(node));
                match stack
                    .services
                    .iter_mut()
                    .find(|known| known.name == placed.service)
                {
                    Some(known) => add_share(known, share),
                    None => stack.services.push(share),
                }
            }
        }
        // Shares in the target's declared order, whoever answered first.
        let order = |name: &str| {
            self.nodes
                .iter()
                .position(|node| self.name_of(node) == name)
        };
        for service in stacks.iter_mut().flat_map(|stack| &mut stack.services) {
            service.nodes.sort_by_key(|share| order(&share.node));
        }
    }

    /// The log lines of a service, or of every service, from every node
    /// (or the nodes the request names) as one stream in the order they
    /// were written. A node that does not answer is one line in the
    /// stream; the read fails only when no node answers.
    pub async fn logs(
        &self,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        if !self.is_cluster() {
            if !request.nodes.is_empty() {
                return Err(MmError::Target {
                    message: format!(
                        "target '{}' names one host; --node applies to a target that lists nodes:",
                        self.target
                    ),
                });
            }
            let node = self.first();
            return node
                .connect()?
                .logs(node.scope.clone(), request, cancellation)
                .await;
        }
        let selected = self.log_nodes(&request)?;
        let scope = self.target_scope();
        let sources = selected
            .into_iter()
            .map(|node| {
                let connection = node.connection();
                let name = self.name_of(node).to_owned();
                let mut request = request.clone();
                // The node reads its own containers; which nodes are read
                // is this side's business.
                request.nodes.clear();
                crate::app::observability::merged::LogSourceRead {
                    node: Some(name),
                    opening: Box::pin(async move {
                        let (transport, scope) = connection?;
                        transport
                            .logs(scope, request, OperationCancellation::default())
                            .await
                    }),
                }
            })
            .collect();
        Ok(crate::app::observability::merged::merged(
            crate::app::observability::merged::MergedRead {
                sources,
                follow: request.follow,
                tail: request.tail,
                hold: NODE_HOLD,
                patience: Some(SILENT_NODE_PATIENCE),
                identity: crate::domain::runtime::ServiceIdentity {
                    project_id: scope.project_id,
                    environment: scope.environment,
                    stack: String::new(),
                    module: None,
                    service: String::new(),
                },
                buffered_records: request.limits.buffered_records,
            },
            cancellation,
        ))
    }

    /// The nodes of a cluster a log read asks: the ones the request names,
    /// or all of them, and of those the ones that run the service it names.
    fn log_nodes(&self, request: &LogRequest) -> Result<Vec<&RemoteNode>, MmError> {
        let only = (!request.nodes.is_empty()).then_some(request.nodes.as_slice());
        let known = self
            .nodes
            .iter()
            .filter_map(|node| node.name.as_deref())
            .collect::<Vec<_>>();
        if let Some(unknown) = request
            .nodes
            .iter()
            .find(|name| !known.contains(&name.as_str()))
        {
            return Err(MmError::Target {
                message: format!(
                    "--node names '{unknown}', which is not a node of target '{}'; its nodes are {}",
                    self.target,
                    known.join(", ")
                ),
            });
        }
        let (selected, _) = self.select(only)?;
        // The checkout says which nodes run a named service: the others
        // would only answer that they have nothing.
        Ok(match request.service() {
            Some(service) => {
                let running = selected
                    .iter()
                    .copied()
                    .filter(|node| {
                        node.placed
                            .as_ref()
                            .is_none_or(|placed| placed.has_service(service))
                    })
                    .collect::<Vec<_>>();
                if running.is_empty() {
                    return Err(MmError::Environment {
                        message: format!(
                            "no node of target '{}' {}runs a service named '{service}' in this environment; `mm service status` lists the services and their nodes",
                            self.target,
                            if only.is_some() {
                                "that --node names "
                            } else {
                                ""
                            },
                        ),
                    });
                }
                running
            }
            None => selected,
        })
    }

    /// Runs a command in a service's container, on the node that runs it.
    pub async fn exec(
        &self,
        request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        self.exec_on(request, None, cancellation).await
    }

    /// `exec` on the node `node` names, or else on the node that runs the
    /// container the request names (a status read of every node finds it),
    /// or else on the first node this checkout places the service on.
    pub async fn exec_on(
        &self,
        request: ExecRequest,
        node: Option<&str>,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        let node = match (node, request.selector.container.as_deref()) {
            (Some(name), _) => self.named_node(name)?,
            (None, Some(container)) if self.is_cluster() => {
                self.node_of_container(&request.selector.service, container)
                    .await?
            }
            (None, _) => self.node_running(&request.selector.service)?,
        };
        node.connect()?
            .exec(node.scope.clone(), request, cancellation)
            .await
            .map_err(|error| in_node(node.name.as_deref(), error))
    }

    /// The node of a cluster target named `name`.
    fn named_node(&self, name: &str) -> Result<&RemoteNode, MmError> {
        self.nodes
            .iter()
            .find(|node| node.name.as_deref() == Some(name))
            .ok_or_else(|| MmError::Target {
                message: format!(
                    "'{name}' is not a node of target '{}'; its nodes are {}",
                    self.target,
                    self.nodes
                        .iter()
                        .filter_map(|node| node.name.as_deref())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            })
    }

    /// The node that runs `service`: the single host, or the first node this
    /// checkout's configuration places the service on.
    pub fn node_running(&self, service: &str) -> Result<&RemoteNode, MmError> {
        if !self.is_cluster() {
            return Ok(self.first());
        }
        self.nodes
            .iter()
            .find(|node| {
                node.placed
                    .as_ref()
                    .is_some_and(|placed| placed.has_service(service))
            })
            .ok_or_else(|| MmError::Environment {
                message: format!(
                    "no node of target '{}' runs a service named '{service}' in this environment; `mm service status` lists the services and their nodes",
                    self.target
                ),
            })
    }
}

/// How long a followed line waits for an older line from another node:
/// longer than the difference in how fast two nodes' lines reach here.
const NODE_HOLD: Duration = Duration::from_millis(400);

/// The snapshot as one node's share: every service says what this node
/// reports for it, and every container which node it is on.
fn labelled(mut snapshot: WorkspaceSnapshot, node: &str) -> WorkspaceSnapshot {
    for stack in &mut snapshot.stacks {
        for service in &mut stack.services {
            service.nodes = vec![ServiceOnNode {
                node: node.to_owned(),
                state: service.state,
                starting: false,
                desired_count: service.desired_count,
                running_count: service.running_count,
                healthy_count: service.healthy_count,
            }];
            for container in &mut service.containers {
                container.node = Some(node.to_owned());
            }
            service.note_starting();
        }
    }
    snapshot
}

/// A service's share on a node that did not answer: it is placed there,
/// and whether it runs there is not known.
fn unanswered(stack: &str, service: &str, node: &str) -> ServiceSnapshot {
    ServiceSnapshot {
        stack: stack.to_owned(),
        name: service.to_owned(),
        module: None,
        nodes: vec![ServiceOnNode {
            node: node.to_owned(),
            state: ServiceState::Unknown,
            starting: false,
            desired_count: 1,
            running_count: 0,
            healthy_count: 0,
        }],
        state: ServiceState::Unknown,
        starting: false,
        desired_count: 1,
        running_count: 0,
        healthy_count: 0,
        image: None,
        revision: None,
        restart_count: None,
        exit_code: None,
        created_at: None,
        started_at: None,
        finished_at: None,
        diagnostic_code: None,
        error: None,
        containers: Vec::new(),
        scale: None,
        restarts: None,
    }
}

/// An autoscaled service runs no container on some of its nodes while its
/// count is low: those nodes are not missing it, and its counts and state
/// are those of the nodes that run it.
fn leave_out_idle_nodes(service: &mut ServiceSnapshot) {
    let missing =
        |node: &ServiceOnNode| node.state == crate::domain::runtime::ServiceState::Missing;
    if service.scale.is_none()
        || !service.nodes.iter().any(missing)
        || service.nodes.iter().all(missing)
    {
        return;
    }
    service.nodes.retain(|node| !missing(node));
    service.desired_count = service.nodes.iter().map(|node| node.desired_count).sum();
    service.running_count = service.nodes.iter().map(|node| node.running_count).sum();
    service.healthy_count = service.nodes.iter().map(|node| node.healthy_count).sum();
    service.state = state_across_nodes(
        &service
            .nodes
            .iter()
            .map(|node| node.state)
            .collect::<Vec<_>>(),
    );
    service.note_starting();
}

/// Adds one node's stacks to the merged list: a service that another node
/// already reported gains this node's share.
fn merge_stacks(merged: &mut Vec<StackSnapshot>, stacks: Vec<StackSnapshot>) {
    for stack in stacks {
        match merged
            .iter_mut()
            .find(|existing| existing.name == stack.name)
        {
            Some(existing) => {
                for service in stack.services {
                    match existing
                        .services
                        .iter_mut()
                        .find(|known| known.name == service.name)
                    {
                        Some(known) => add_share(known, service),
                        None => existing.services.push(service),
                    }
                }
                for resource in stack.owned_resources {
                    if !existing.owned_resources.contains(&resource) {
                        existing.owned_resources.push(resource);
                    }
                }
            }
            None => merged.push(stack),
        }
    }
}

/// Adds another node's share of a service: the counts add up, the state is
/// the service's across its nodes, and the containers are listed together.
fn add_share(service: &mut ServiceSnapshot, share: ServiceSnapshot) {
    service.scale = match (service.scale.take(), share.scale) {
        (Some(known), Some(other)) => Some(known.merged(other)),
        (known, other) => known.or(other),
    };
    service.restarts = match (service.restarts.take(), share.restarts) {
        (Some(known), Some(other)) => Some(known.merged(other)),
        (known, other) => known.or(other),
    };
    service.desired_count += share.desired_count;
    service.running_count += share.running_count;
    service.healthy_count += share.healthy_count;
    service.nodes.extend(share.nodes);
    service.state = state_across_nodes(
        &service
            .nodes
            .iter()
            .map(|node| node.state)
            .collect::<Vec<_>>(),
    );
    service.containers.extend(share.containers);
    service.restart_count = service.restart_count.max(share.restart_count);
    service.image = service.image.take().or(share.image);
    service.revision = service.revision.take().or(share.revision);
    service.exit_code = service.exit_code.or(share.exit_code);
    service.created_at = earliest(service.created_at.take(), share.created_at);
    service.started_at = earliest(service.started_at.take(), share.started_at);
    service.finished_at = service.finished_at.take().max(share.finished_at);
    service.diagnostic_code = service.diagnostic_code.take().or(share.diagnostic_code);
    service.error = service.error.take().or(share.error);
    service.note_starting();
}

fn earliest(left: Option<String>, right: Option<String>) -> Option<String> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (left, right) => left.or(right),
    }
}
