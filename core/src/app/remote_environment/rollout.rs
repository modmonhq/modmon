//! Rolling a reviewed deployment out across the nodes of a cluster.
//!
//! The client orders the steps; each node acts only on its own engine. One
//! stage per stack, in the plan's order; within a stage one batch of nodes
//! at a time. A node is done with a stack when its containers are healthy
//! and every node resolves them. When a step fails, every node that was
//! changed returns to what it ran before (or, for a service with
//! `failure: pause`, everything stays as it is), and the nodes the rollout
//! had not reached are never touched.
//!
//! While it works the client holds the environment's deployment lock on
//! every node of the plan and says "still here" every few seconds, so a
//! second deployment is refused and a client that went away is noticed.

use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use std::time::Duration;

use futures_util::future::join_all;

use super::plans::NodeProgress;
use super::progress::Watched;
use super::{RemoteEnvironment, RemoteNode, in_node};
use crate::domain::deployment::DeploymentResult;
use crate::domain::discovery::NameReport;
use crate::domain::error::MmError;
use crate::domain::remote::{NodeApplyOutcome, NodeApplyState};
use crate::domain::rollout::{
    ClusterRollout, DISCOVERY_PATIENCE, DeploymentStatus, LOCK_RENEW_MS, NodeStatusRow,
    ProgressStep, RolloutAnswer, RolloutProgress, RolloutStage, RolloutStep,
};
use crate::domain::shared_storage::SealedStorage;

/// How long a step that waits for no health check may take.
const SETUP_PATIENCE: Duration = Duration::from_secs(30 * 60);
/// What a stack step may take on top of the plan's convergence window:
/// pulling images and starting containers.
const STACK_OVERHEAD: Duration = Duration::from_secs(20 * 60);
const LOCK_PATIENCE: Duration = Duration::from_secs(30);
const DISCOVERY_LOOK: Duration = Duration::from_secs(1);

type Outcome = NodeApplyOutcome<DeploymentResult>;

#[path = "rollout_jobs.rs"]
mod jobs;

/// Why the rollout stopped at one node.
struct Fault {
    index: usize,
    /// The stack and service that failed, when a node named them.
    stack: Option<String>,
    service: Option<String>,
    text: String,
    /// The last lines the failed service's container logged.
    log: Vec<String>,
    /// The node undid its own part already (a finish that failed).
    undone: bool,
    /// The node failed while it prepared, before any container changed.
    preparing: bool,
    /// The code of the error that stopped it, when an error did.
    code: Option<String>,
}

/// What one node did in one stage.
struct NodeStep {
    index: usize,
    began: bool,
    fault: Option<Fault>,
}

struct Walk<'a> {
    environment: &'a RemoteEnvironment,
    plan_hash: &'a str,
    nodes: &'a [&'a RemoteNode],
    rollout: &'a ClusterRollout,
    progress: NodeProgress<'a>,
    /// The nodes that began, in the order they did.
    begun: Vec<usize>,
}

impl RemoteEnvironment {
    /// Rolls the reviewed cluster plan out over `nodes` (the plan's nodes,
    /// in declared order). An error means no node was changed; otherwise
    /// every node has an outcome.
    pub(super) async fn roll_out(
        &self,
        plan_hash: &str,
        nodes: &[&RemoteNode],
        (rollout, storage): (&ClusterRollout, Option<&SealedStorage>),
        progress: NodeProgress<'_>,
    ) -> Result<Vec<Outcome>, MmError> {
        self.hold_all(plan_hash, nodes).await?;
        self.address_all(plan_hash, nodes, progress).await?;
        if let Some(storage) = storage {
            self.ready_storage(plan_hash, nodes, storage, progress)
                .await?;
        }
        let mut walk = Walk {
            environment: self,
            plan_hash,
            nodes,
            rollout,
            progress,
            begun: Vec::new(),
        };
        let beat = still_here(nodes, plan_hash);
        let work = walk.run();
        tokio::pin!(beat, work);
        Ok(tokio::select! {
            outcomes = &mut work => outcomes,
            never = &mut beat => match never {},
        })
    }

    /// Takes the environment's deployment lock on every node of the plan,
    /// or on none. A node the plan leaves out must not be in the middle of
    /// another deployment either.
    async fn hold_all(&self, plan_hash: &str, nodes: &[&RemoteNode]) -> Result<(), MmError> {
        let planned = |node: &RemoteNode| nodes.iter().any(|planned| planned.name == node.name);
        let (held, others) = tokio::join!(
            join_all(nodes.iter().map(|node| step(
                node,
                plan_hash,
                RolloutStep::Hold,
                LOCK_PATIENCE
            ))),
            join_all(
                self.nodes
                    .iter()
                    .filter(|node| !planned(node))
                    .map(|node| async move {
                        let status = node
                            .connect()?
                            .deployment_status(node.scope.clone())
                            .await?;
                        Ok::<_, MmError>((self.name_of(node), status))
                    })
            )
        );
        // A node the plan leaves out and that does not answer is why it
        // was left out; it cannot be deploying.
        let busy = others.into_iter().flatten().find_map(|(name, status)| {
            status
                .rollout
                .filter(|rollout| rollout.active && rollout.plan_hash != plan_hash)
                .map(|rollout| MmError::Remote {
                    code: "MM_DEPLOY_RUNNING",
                    message: format!(
                        "node '{name}', which this plan leaves out, is in the middle of a deployment {} started (plan {}); wait for it to finish. `mm deploy status` shows where it is",
                        rollout.started_by,
                        rollout.plan_hash.get(..12).unwrap_or(&rollout.plan_hash),
                    ),
                })
        });
        let refused = nodes
            .iter()
            .zip(&held)
            .filter_map(|(node, held)| held.as_ref().err().map(|error| (node, error)))
            // Another deployment is the answer to give, before a node that
            // did not answer.
            .min_by_key(|(_, error)| error.code() != "MM_DEPLOY_RUNNING")
            .map(|(node, error)| in_node(node.name.as_deref(), clone_error(error)));
        let Some(error) = refused.or(busy) else {
            return Ok(());
        };
        join_all(
            nodes
                .iter()
                .zip(&held)
                .filter(|(_, held)| held.is_ok())
                .map(|(node, _)| step(node, plan_hash, RolloutStep::Release, LOCK_PATIENCE)),
        )
        .await;
        Err(error)
    }

    /// Has every node of the plan store the addresses the plan gives the
    /// cluster's networks, before any node is replaced: a container that
    /// starts on a new network is then reachable and resolvable from the
    /// nodes whose turn has not come. No container changes; when a node
    /// refuses, the lock is given up and nothing else was done.
    async fn address_all(
        &self,
        plan_hash: &str,
        nodes: &[&RemoteNode],
        progress: NodeProgress<'_>,
    ) -> Result<(), MmError> {
        let stored = join_all(nodes.iter().map(|node| {
            self.with_heartbeat(
                Watched {
                    node,
                    step: "networks",
                    operation: plan_hash,
                },
                progress,
                step(node, plan_hash, RolloutStep::Networks, SETUP_PATIENCE),
            )
        }))
        .await;
        let refused = nodes
            .iter()
            .zip(stored)
            .find_map(|(node, stored)| stored.err().map(|error| (node, error)));
        let Some((node, error)) = refused else {
            return Ok(());
        };
        join_all(
            nodes
                .iter()
                .map(|node| step(node, plan_hash, RolloutStep::Release, LOCK_PATIENCE)),
        )
        .await;
        Err(in_node(node.name.as_deref(), error))
    }

    /// Makes the plan's shared volumes ready before any node is replaced:
    /// first every node that mounts one checks that it reaches the storage
    /// node, then the storage node creates each volume's directory. So a
    /// deployment that cannot go through creates nothing, and no node ever
    /// mounts a directory that is not there. No container changes and
    /// nothing is removed; when a node refuses, the lock is given up and
    /// the nodes run what they ran.
    async fn ready_storage(
        &self,
        plan_hash: &str,
        nodes: &[&RemoteNode],
        storage: &SealedStorage,
        progress: NodeProgress<'_>,
    ) -> Result<(), MmError> {
        let is_storage = |node: &&&RemoteNode| node.name.as_deref() == Some(storage.node.as_str());
        let mounting = nodes
            .iter()
            .filter(|node| !is_storage(node))
            .collect::<Vec<_>>();
        let checked = join_all(mounting.iter().map(|node| {
            self.with_heartbeat(
                Watched {
                    node,
                    step: "shared volumes",
                    operation: plan_hash,
                },
                progress,
                step(node, plan_hash, RolloutStep::Storage, SETUP_PATIENCE),
            )
        }))
        .await;
        let mut refused = mounting
            .into_iter()
            .zip(checked)
            .find_map(|(node, checked)| checked.err().map(|error| (*node, error)));
        if let (None, Some(node)) = (&refused, nodes.iter().find(is_storage)) {
            progress(&RolloutProgress::new(
                storage.node.clone(),
                ProgressStep::Note("shared volumes".to_owned()),
            ));
            let watched = Watched {
                node,
                step: "shared volumes",
                operation: plan_hash,
            };
            let made = step(node, plan_hash, RolloutStep::Storage, SETUP_PATIENCE);
            match self.with_heartbeat(watched, progress, made).await {
                Ok(RolloutAnswer::StorageReady { created }) => {
                    for directory in created {
                        progress(&RolloutProgress::new(
                            storage.node.clone(),
                            ProgressStep::Note(format!("created {directory}")),
                        ));
                    }
                }
                Ok(_) => {}
                Err(error) => refused = Some((*node, error)),
            }
        }
        let Some((node, error)) = refused else {
            return Ok(());
        };
        join_all(
            nodes
                .iter()
                .map(|node| step(node, plan_hash, RolloutStep::Release, LOCK_PATIENCE)),
        )
        .await;
        Err(in_node(node.name.as_deref(), error))
    }

    /// What every host of the environment remembers of its deployments.
    /// Never fails: a host that does not answer is a row that says why.
    pub async fn deployment_status(&self) -> DeploymentStatus {
        let nodes = join_all(self.nodes.iter().map(|node| async move {
            let answer = match node.connect() {
                Ok(transport) => transport.deployment_status(node.scope.clone()).await,
                Err(error) => Err(error),
            };
            let (status, problem) = match answer {
                Ok(status) => (Some(status), None),
                Err(error) => (None, Some(error.to_string())),
            };
            NodeStatusRow {
                node: self.name_of(node).to_owned(),
                problem,
                status,
            }
        }))
        .await;
        DeploymentStatus {
            target: self.target.clone(),
            environment: self.first().scope.environment.clone(),
            nodes,
            answered_unhealthy: Vec::new(),
        }
    }
}

/// One step on one node.
async fn step(
    node: &RemoteNode,
    plan_hash: &str,
    step: RolloutStep,
    patience: Duration,
) -> Result<RolloutAnswer, MmError> {
    node.connect()?
        .rollout(node.scope.clone(), plan_hash.to_owned(), step, patience)
        .await
}

/// Tells every node of the plan, every few seconds and for as long as the
/// rollout works, that its client is still there. A node that misses it
/// for a minute lets another deployment in.
async fn still_here(nodes: &[&RemoteNode], plan_hash: &str) -> std::convert::Infallible {
    loop {
        tokio::time::sleep(Duration::from_millis(LOCK_RENEW_MS)).await;
        join_all(
            nodes
                .iter()
                .map(|node| step(node, plan_hash, RolloutStep::Hold, LOCK_PATIENCE)),
        )
        .await;
    }
}

/// An error to hand on when the original is only borrowed.
fn clone_error(error: &MmError) -> MmError {
    match error {
        MmError::Remote { code, message } => MmError::Remote {
            code,
            message: message.clone(),
        },
        MmError::Unreachable {
            subject,
            problem,
            help,
        } => MmError::Unreachable {
            subject: subject.clone(),
            problem: problem.clone(),
            help: help.clone(),
        },
        MmError::Security { message } => MmError::Security {
            message: message.clone(),
        },
        MmError::Timeout { message } => MmError::Timeout {
            message: message.clone(),
        },
        MmError::Cancelled { message } => MmError::Cancelled {
            message: message.clone(),
        },
        MmError::Launch { message } => MmError::Launch {
            message: message.clone(),
        },
        other => MmError::Environment {
            message: other.to_string(),
        },
    }
}

fn outcome(node: &str, state: NodeApplyState) -> Outcome {
    NodeApplyOutcome {
        node: node.to_owned(),
        state,
        result: None,
        error: None,
        rolled_back: false,
        log: Vec::new(),
        stopped: Vec::new(),
        unchanged: false,
        code: None,
    }
}

impl Walk<'_> {
    fn name(&self, index: usize) -> &str {
        self.nodes
            .get(index)
            .and_then(|node| node.name.as_deref())
            .unwrap_or_default()
    }

    fn say(&self, index: usize, step: ProgressStep) {
        (self.progress)(&RolloutProgress::new(self.name(index), step));
    }

    /// One step on one node. While a step that changes the node runs, its
    /// host is asked how it is doing every heartbeat, and a host that
    /// shows no progress ends the wait with a timeout.
    async fn step(
        &self,
        index: usize,
        step: RolloutStep,
        patience: Duration,
    ) -> Result<RolloutAnswer, MmError> {
        let Some(node) = self.nodes.get(index) else {
            return Err(MmError::State {
                message: "the rollout names a node the plan does not cover".to_owned(),
            });
        };
        if !step.changes_host() {
            return self::step(node, self.plan_hash, step, patience).await;
        }
        let label = match &step {
            RolloutStep::Start { stack } => format!("{stack} (new containers next to the old)"),
            RolloutStep::Apply { stack } => stack.clone(),
            RolloutStep::Ahead { stack } => format!("{stack} (what its jobs need)"),
            RolloutStep::Job { stack, job } => format!("{stack}: job {job}"),
            RolloutStep::Begin => "preparing".to_owned(),
            RolloutStep::Finish => "finishing".to_owned(),
            RolloutStep::Rollback => "rolling back".to_owned(),
            other => other.name().to_owned(),
        };
        let watched = Watched {
            node,
            step: &label,
            operation: self.plan_hash,
        };
        let stepping = self::step(node, self.plan_hash, step, patience);
        self.environment
            .with_heartbeat(watched, self.progress, stepping)
            .await
    }

    async fn run(&mut self) -> Vec<Outcome> {
        let mut outcomes = (0..self.nodes.len())
            .map(|index| outcome(self.name(index), NodeApplyState::NotStarted))
            .collect::<Vec<_>>();
        let fault = match self.stages().await {
            Some(fault) => Some(fault),
            None => self.finish(&mut outcomes).await,
        };
        if let Some(fault) = fault {
            self.unwind(&mut outcomes, fault).await;
        }
        outcomes
    }

    /// Every stage in order; returns where the rollout stopped, if it did.
    async fn stages(&mut self) -> Option<Fault> {
        let rollout = self.rollout;
        for stage in &rollout.stages {
            if let Some(fault) = self.stage_jobs(stage).await {
                return Some(fault);
            }
            for batch in &stage.batches {
                let indices = batch
                    .iter()
                    .filter_map(|name| {
                        self.nodes
                            .iter()
                            .position(|node| node.name.as_deref() == Some(name.as_str()))
                    })
                    .collect::<Vec<_>>();
                let steps = join_all(
                    indices
                        .iter()
                        .map(|index| self.roll_node(stage, *index, self.begun.contains(index))),
                )
                .await;
                let mut fault = None;
                for step in steps {
                    if step.began && !self.begun.contains(&step.index) {
                        self.begun.push(step.index);
                    }
                    fault = fault.or(step.fault);
                }
                if fault.is_some() {
                    return fault;
                }
            }
        }
        None
    }

    /// One node's turn with one stack: begin if it has not, the new
    /// containers of `start-first` services, then the stack. After each
    /// change, every node has to resolve the node's new containers.
    async fn roll_node(&self, stage: &RolloutStage, index: usize, begun: bool) -> NodeStep {
        let fault = |began: bool, service: Option<String>, text: String| NodeStep {
            index,
            began,
            fault: Some(Fault {
                index,
                stack: Some(stage.stack.clone()),
                service,
                text,
                log: Vec::new(),
                undone: false,
                preparing: false,
                code: None,
            }),
        };
        if !begun {
            self.say(index, ProgressStep::Preparing);
            // A node that did not say it began may have begun all the
            // same; it is asked to roll back like the others.
            if let Err(error) = self.step(index, RolloutStep::Begin, SETUP_PATIENCE).await {
                let mut step = fault(true, None, format!("while preparing: {error}"));
                if let Some(fault) = &mut step.fault {
                    fault.preparing = true;
                    fault.code = Some(error.code().to_owned());
                }
                return step;
            }
        }
        let patience = self.stack_patience();
        let stack = || stage.stack.clone();
        let mut turns = Vec::new();
        if !stage.start_first.is_empty() {
            let what = ProgressStep::Note(format!("{}: start-first", stack()));
            turns.push((what, RolloutStep::Start { stack: stack() }));
        }
        turns.push((
            ProgressStep::Stack(stack()),
            RolloutStep::Apply { stack: stack() },
        ));
        for (what, turn) in turns {
            let applying = matches!(turn, RolloutStep::Apply { .. });
            if applying {
                self.say(index, what.clone());
            }
            let addresses = match self.step(index, turn, patience).await {
                Ok(next @ RolloutAnswer::Started { .. }) => self.started_next(index, stack(), next),
                Ok(RolloutAnswer::Applied { addresses }) => addresses,
                Ok(RolloutAnswer::Failed(failure)) => {
                    let mut step = fault(true, failure.service.clone(), failure.to_string());
                    if let Some(fault) = &mut step.fault {
                        fault.log = failure.log;
                        fault.code = failure.code;
                    }
                    return step;
                }
                Ok(_) => {
                    return fault(
                        true,
                        None,
                        "the node gave another answer than a stack step's".to_owned(),
                    );
                }
                Err(error) => {
                    // A timeout of the client's names its step already.
                    let text = if matches!(error, MmError::Timeout { .. }) {
                        error.to_string()
                    } else {
                        format!("during {what}: {error}")
                    };
                    let mut step = fault(true, None, text);
                    if let Some(fault) = &mut step.fault {
                        fault.code = Some(error.code().to_owned());
                    }
                    return step;
                }
            };
            if let Err(problem) = self.resolved_everywhere(&addresses).await {
                return fault(true, None, format!("stack '{}': {problem}", stage.stack));
            }
            if applying {
                self.say(index, ProgressStep::Healthy(stack()));
            }
        }
        NodeStep {
            index,
            began: true,
            fault: None,
        }
    }

    /// Waits until every node that holds a part of the networks resolves
    /// `addresses`: the containers a node just made healthy.
    async fn resolved_everywhere(&self, addresses: &[Ipv4Addr]) -> Result<(), String> {
        if addresses.is_empty() {
            return Ok(());
        }
        let environment = self.environment;
        let waited = tokio::time::Instant::now();
        loop {
            // The nodes of the plan: a node it leaves out learns the
            // plan's addresses at its own next deployment.
            let answers = join_all(self.nodes.iter().map(|node| async move {
                let answer = match node.connect() {
                    Ok(transport) => transport.names(node.scope.clone()).await,
                    Err(error) => Err(error),
                };
                (environment.name_of(node).to_owned(), answer)
            }))
            .await;
            let mut views = Vec::new();
            let mut waiting = Vec::new();
            for (name, answer) in answers {
                match answer {
                    Ok(view) => views.push((name, view)),
                    Err(error) => {
                        waiting.push(format!(
                            "node '{name}' does not say what it resolves ({error})"
                        ));
                    }
                }
            }
            let merged = NameReport::merged(views);
            for address in addresses {
                let listed = merged
                    .names
                    .iter()
                    .flat_map(|name| &name.addresses)
                    .filter(|listed| listed.address == *address)
                    .collect::<Vec<_>>();
                let unknown = listed
                    .iter()
                    .flat_map(|listed| listed.unknown_to.iter().map(String::as_str))
                    .collect::<BTreeSet<_>>();
                if listed.is_empty() {
                    waiting.push(format!("no node lists {address} yet"));
                } else if !unknown.is_empty() {
                    waiting.push(format!(
                        "{address} is not known to {}",
                        unknown.into_iter().collect::<Vec<_>>().join(", ")
                    ));
                } else if listed.iter().any(|listed| !listed.health.answers()) {
                    waiting.push(format!("{address} is not healthy"));
                }
            }
            if waiting.is_empty() {
                return Ok(());
            }
            if waited.elapsed() >= DISCOVERY_PATIENCE {
                return Err(format!(
                    "its containers are healthy, but after {} s not every node resolves them: {}. `mm service status` shows what each node knows",
                    DISCOVERY_PATIENCE.as_secs(),
                    waiting.join("; ")
                ));
            }
            tokio::time::sleep(DISCOVERY_LOOK).await;
        }
    }

    /// Every stack is healthy on every node. First every node the plan
    /// gives no stack begins, so that nothing is final while a node can
    /// still refuse; then each node removes what it no longer runs and
    /// records the deployment.
    async fn finish(&mut self, outcomes: &mut [Outcome]) -> Option<Fault> {
        let fault = |index: usize, text: String, undone: bool| Fault {
            index,
            stack: None,
            service: None,
            text,
            log: Vec::new(),
            undone,
            preparing: false,
            code: None,
        };
        for index in 0..self.nodes.len() {
            if self.begun.contains(&index) {
                continue;
            }
            self.say(index, ProgressStep::Preparing);
            self.begun.push(index);
            if let Err(error) = self.step(index, RolloutStep::Begin, SETUP_PATIENCE).await {
                let mut stopped = fault(index, error.to_string(), false);
                stopped.code = Some(error.code().to_owned());
                return Some(stopped);
            }
        }
        for index in 0..self.nodes.len() {
            self.say(index, ProgressStep::Finishing);
            match self
                .step(index, RolloutStep::Finish, self.undo_patience())
                .await
            {
                Ok(RolloutAnswer::Finished(result)) => {
                    if let Some(outcome) = outcomes.get_mut(index) {
                        outcome.state = NodeApplyState::Applied;
                        outcome.result = result.map(|result| *result);
                    }
                }
                Ok(_) => {
                    return Some(fault(
                        index,
                        "the node gave another answer than a finished deployment".to_owned(),
                        false,
                    ));
                }
                // A node whose finish failed has undone its own part.
                Err(error) => {
                    let mut stopped = fault(index, error.to_string(), true);
                    stopped.code = Some(error.code().to_owned());
                    return Some(stopped);
                }
            }
        }
        None
    }

    /// After a fault: the nodes that were changed return to what they ran
    /// before, newest change first, or stay as they are when the failed
    /// service's `update.failure` is `pause`. Nodes that never began only
    /// give up the lock.
    async fn unwind(&self, outcomes: &mut [Outcome], fault: Fault) {
        let pause = fault
            .stack
            .as_deref()
            .is_some_and(|stack| self.rollout.pauses(stack, fault.service.as_deref()));
        for index in self.begun.iter().rev().copied() {
            let Some(outcome) = outcomes.get_mut(index) else {
                continue;
            };
            if outcome.state == NodeApplyState::Applied {
                continue;
            }
            let failed = index == fault.index;
            if failed && fault.undone {
                continue;
            }
            if pause {
                // The lock goes; the node stays as it is.
                self.step(index, RolloutStep::Release, LOCK_PATIENCE)
                    .await
                    .unwrap_or(RolloutAnswer::Released);
                outcome.state = NodeApplyState::Unfinished;
                continue;
            }
            self.say(index, ProgressStep::RollingBack);
            match self
                .step(index, RolloutStep::Rollback, self.undo_patience())
                .await
            {
                // The node had changed nothing: it only gave up the lock,
                // or failed while it prepared, before any container.
                Ok(RolloutAnswer::Released) => outcome.state = NodeApplyState::NotStarted,
                Ok(_) if failed && fault.preparing => outcome.state = NodeApplyState::NotStarted,
                Ok(_) => {
                    outcome.state = NodeApplyState::RolledBack;
                    outcome.rolled_back = failed;
                }
                Err(error) => {
                    outcome.state = NodeApplyState::Unfinished;
                    outcome.error = Some(format!("could not be rolled back: {error}"));
                }
            }
        }
        let untouched = (0..self.nodes.len())
            .filter(|index| !self.begun.contains(index))
            .collect::<Vec<_>>();
        join_all(
            untouched
                .iter()
                .map(|index| self.step(*index, RolloutStep::Release, LOCK_PATIENCE)),
        )
        .await;
        if let Some(outcome) = outcomes.get_mut(fault.index) {
            outcome.rolled_back = fault.undone || outcome.rolled_back;
            outcome.error = Some(match outcome.error.take() {
                Some(stuck) => format!("{}. The node {stuck}", fault.text),
                None => fault.text,
            });
            outcome.log = fault.log;
            outcome.state = NodeApplyState::Failed;
            outcome.rolled_back = outcome.rolled_back && !fault.preparing;
            outcome.unchanged = fault.preparing;
            outcome.code = fault.code;
        }
    }
}
