//! A reviewed plan applied in steps another party orders.
//!
//! `apply_reviewed` does everything in one call. A cluster rolls out stack
//! by stack and node by node, so on a node the same work is cut into
//! steps: begin (open the operation, prepare networks and volumes), one or
//! two steps per stack, and finish (routes, removals, close the
//! operation). Between the steps the operation stays open in the journal,
//! which is also how an interrupted rollout is found and undone.

use std::net::Ipv4Addr;
use std::time::Duration;

use super::*;
use crate::domain::rollout::{RolloutChanges, StepFailure};

/// How long a node waits for its own resolver to list a container that
/// just became healthy.
const OWN_RESOLVER_PATIENCE: Duration = Duration::from_secs(15);
const OWN_RESOLVER_LOOK: Duration = Duration::from_millis(250);

/// Which half of a stack's turn a step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackStep {
    /// New containers of `start-first` services next to the old ones.
    Start,
    /// The stack itself.
    Apply,
    /// The services the stack's `before-deploy` jobs' services depend on.
    Ahead,
}

/// What a stack step did on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackOutcome {
    Done {
        /// For `Start`: the services that now run two containers.
        started: Vec<String>,
        /// The addresses of this node's containers of the stack that the
        /// other nodes are to resolve before the rollout goes on.
        addresses: Vec<Ipv4Addr>,
    },
    /// The stack is not healthy. Nothing was undone.
    Failed(StepFailure),
}

/// An operation `begin_rollout` opened and no step closed yet.
pub(super) struct RunningRollout {
    pub(super) operation_id: OperationId,
    pub(super) plan: DeploymentPlan,
    pub(super) target: NamedTarget,
    _guard: Box<dyn crate::app::ports::TargetOperationLockGuard>,
}

impl DeploymentService {
    /// Opens the operation of the reviewed plan and prepares the host for
    /// its stacks. No workload changes. A failure closes the operation and
    /// leaves the host as it was.
    pub async fn begin_rollout(
        &self,
        selection: &WorkspaceSelection,
        expected_plan_hash: &str,
    ) -> Result<(), MmError> {
        let ReviewedApply {
            plan,
            target,
            operation,
            guard: _guard,
        } = self.begin_reviewed(selection, expected_plan_hash).await?;
        let prepared: Result<(), MmError> = async {
            self.record_phase(operation.id, OperationPhase::Validate)?;
            self.record_phase(operation.id, OperationPhase::Apply)?;
            self.prepare_workloads(&plan, &target).await?;
            self.sweep_jobs(selection, &plan, &target).await
        }
        .await;
        match prepared {
            Ok(()) => Ok(()),
            Err(error) => {
                let failures = self
                    .undo(
                        selection,
                        (&plan, &target, None),
                        operation.id,
                        error.code(),
                    )
                    .await;
                Err(undo_error(operation.id, error, failures))
            }
        }
    }

    /// One step of one stack of the rollout `plan_hash` began.
    pub async fn roll_stack(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
        stack: &str,
        step: StackStep,
    ) -> Result<StackOutcome, MmError> {
        let running = self.running_rollout(selection, plan_hash)?.ok_or_else(|| {
            MmError::State {
                message: format!(
                    "this host has no rollout of plan {plan_hash} in progress; run `mm deploy plan` again"
                ),
            }
        })?;
        let plan = &running.plan;
        let stack_plan = plan
            .stacks
            .iter()
            .find(|rendered| rendered.stack == stack)
            .ok_or_else(|| MmError::State {
                message: format!("plan {plan_hash} has no stack '{stack}' on this host"),
            })?;
        let mut environment = self
            .prepare_rollout_environment(selection, plan, running.operation_id)
            .await?;
        let bundle = environment.bundle();
        // What the node answered with before a start: the new containers
        // are the addresses that come after.
        let before = match step {
            StackStep::Start => self.own_addresses(plan),
            StackStep::Apply | StackStep::Ahead => std::collections::BTreeSet::new(),
        };
        let rolled = match step {
            StackStep::Start => {
                self.start_beside_old(plan, &running.target, stack_plan, bundle)
                    .await
            }
            StackStep::Apply => self
                .apply_stack(plan, &running.target, stack_plan, bundle)
                .await
                .map(|failure| match failure {
                    None => Ok(Vec::new()),
                    Some(failure) => Err(failure),
                }),
            StackStep::Ahead => {
                self.apply_ahead(plan, &running.target, stack_plan, bundle)
                    .await
            }
        };
        environment.dispose()?;
        let started = match rolled? {
            Ok(started) => started,
            Err(failure) => return Ok(StackOutcome::Failed(failure)),
        };
        // After a start, the services that got a second container; after
        // an apply, every service of the stack.
        let wanted = match step {
            StackStep::Start => Wanted::Twice(started.as_slice()),
            StackStep::Apply => Wanted::Every,
            StackStep::Ahead => Wanted::Once(started.as_slice()),
        };
        Ok(
            match self.resolved_here(plan, stack, wanted, &before).await {
                Ok(addresses) => StackOutcome::Done { started, addresses },
                Err(failure) => StackOutcome::Failed(failure),
            },
        )
    }

    /// Every stack is healthy: publishes the routes, removes what the plan
    /// no longer runs, and closes the operation. A failure returns the
    /// host to what it ran before.
    pub async fn finish_rollout(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
    ) -> Result<DeploymentResult, MmError> {
        let running = self.running_rollout(selection, plan_hash)?.ok_or_else(|| {
            MmError::State {
                message: format!(
                    "this host has no rollout of plan {plan_hash} in progress, so there is nothing to finish; run `mm deploy plan` again"
                ),
            }
        })?;
        let settled = self.settle(&running.plan, &running.target).await;
        match settled {
            Ok(result) => {
                self.forget_environment(running.operation_id);
                let closed = self.close_operation(
                    selection,
                    &running.plan,
                    running.operation_id,
                    Ok(result),
                );
                if closed.is_ok() {
                    self.prune_superseded_versions(&running.plan).await;
                }
                closed
            }
            Err(error) => {
                let failures = self
                    .undo(
                        selection,
                        (&running.plan, &running.target, None),
                        running.operation_id,
                        error.code(),
                    )
                    .await;
                Err(undo_error(running.operation_id, error, failures))
            }
        }
    }

    /// Returns the host to what it ran before the rollout `plan_hash`
    /// began, and closes its operation: what `changes` lists, or
    /// everything when it is `None`. `Ok(false)` when the host has no
    /// such rollout in progress and so nothing to undo.
    pub async fn rollback_rollout(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
        changes: Option<&RolloutChanges>,
    ) -> Result<bool, MmError> {
        let Some(running) = self.running_rollout(selection, plan_hash)? else {
            return Ok(false);
        };
        let failures = self
            .undo(
                selection,
                (&running.plan, &running.target, changes),
                running.operation_id,
                "MM_ROLLED_BACK",
            )
            .await;
        if failures.is_empty() {
            return Ok(true);
        }
        Err(MmError::State {
            message: format!(
                "the host could not be returned to what it ran before: {}. `mm deploy plan` plans from what it runs now",
                failures.join("; ")
            ),
        })
    }

    /// After a step of `stack` of the rollout `plan_hash` was stopped: the
    /// containers the step had begun to take out (`retire_outdated`,
    /// `retire_replaced`) are routed to again, and one whose stop had begun
    /// is stopped and removed first. A leave goes on when the step that
    /// asked for it stops waiting; taken back here, it does not change the
    /// host after the environment is free. Returns the containers
    /// taken back.
    pub async fn take_back_leaves(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
        stack: &str,
    ) -> Result<Vec<String>, MmError> {
        let Some(leaving) = self.planner.leaving() else {
            return Ok(Vec::new());
        };
        let Some(running) = self.running_rollout(selection, plan_hash)? else {
            return Ok(Vec::new());
        };
        let Some(stack) = running
            .plan
            .stacks
            .iter()
            .find(|known| known.stack == stack)
        else {
            return Ok(Vec::new());
        };
        leaving.recall(&stack.compose_project_name).await
    }

    /// The open operation of plan `plan_hash`, with the target's lock.
    pub(super) fn running_rollout(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
    ) -> Result<Option<RunningRollout>, MmError> {
        let (document, environment, target) = self.selected_target(selection)?;
        let guard = self.ports.locks.acquire(target.definition.id)?;
        let running = self
            .ports
            .state
            .running_deployments_for_owner(
                document.config.project.id,
                &environment,
                target.definition.id,
            )?
            .into_iter()
            .find(|(_, plan)| plan.plan_hash == plan_hash);
        let Some((operation_id, plan)) = running else {
            return Ok(None);
        };
        validate_target(&plan, &target)?;
        Ok(Some(RunningRollout {
            operation_id,
            plan,
            target,
            _guard: guard,
        }))
    }

    /// Returns the host to the deployment before `plan` (what `changes`
    /// lists of it, or everything) and closes the operation as failed with
    /// `code`. Returns what went wrong on the way; nothing, when the host
    /// runs what it ran before.
    async fn undo(
        &self,
        selection: &WorkspaceSelection,
        (plan, target, changes): (&DeploymentPlan, &NamedTarget, Option<&RolloutChanges>),
        operation_id: OperationId,
        code: &str,
    ) -> Vec<String> {
        let mut failures = Vec::new();
        if let Err(phase) = self.record_phase(operation_id, OperationPhase::Rollback) {
            failures.push(phase.to_string());
        }
        let previous = match self.ports.state.latest_successful_deployment_for_owner(
            plan.project_id,
            &plan.environment,
            plan.target_id,
        ) {
            Ok(previous) => previous,
            Err(error) => {
                failures.push(error.to_string());
                None
            }
        };
        match self
            .prepare_rollout_environment(selection, plan, operation_id)
            .await
        {
            Ok(mut environment) => {
                if let Err(rollback) = self
                    .rollback_runtime(
                        plan,
                        target,
                        previous.as_ref(),
                        environment.bundle(),
                        changes,
                    )
                    .await
                {
                    failures.push(rollback.to_string());
                }
                if let Err(cleanup) = environment.dispose() {
                    failures.push(cleanup.to_string());
                }
            }
            Err(error) => failures.push(error.to_string()),
        }
        self.forget_environment(operation_id);
        if let Err(journal) =
            self.finish_operation(operation_id, OperationState::Failed, Some(code))
        {
            failures.push(journal.to_string());
        }
        failures
    }

    /// The addresses this node's resolver answers with for its own
    /// containers, on every network of the plan's environment.
    fn own_addresses(&self, plan: &DeploymentPlan) -> std::collections::BTreeSet<Ipv4Addr> {
        let (Some(names), Some(node)) = (self.planner.names(), plan.node.as_ref()) else {
            return std::collections::BTreeSet::new();
        };
        let prefix =
            crate::domain::cluster_network::network_key(plan.project_id, &plan.environment, "");
        names
            .report(&prefix)
            .only_networks(|network| plan.networks.iter().any(|own| own.logical_name == network))
            .names
            .iter()
            .flat_map(|name| name.answers())
            .filter(|address| address.node == node.node)
            .map(|address| address.address)
            .collect()
    }

    /// The addresses of this node's healthy containers of `stack` on the
    /// networks that span the cluster, once the node's own resolver lists
    /// them: one per service `wanted` names and network, and for a service
    /// started next to its old container two, or one that it did not
    /// answer with `before` (the new container: the old one is not on a
    /// network the service just joined). Empty where no resolver answers
    /// names.
    async fn resolved_here(
        &self,
        plan: &DeploymentPlan,
        stack: &str,
        wanted: Wanted<'_>,
        before: &std::collections::BTreeSet<Ipv4Addr>,
    ) -> Result<Vec<Ipv4Addr>, StepFailure> {
        let (Some(names), Some(node), Some(host)) = (
            self.planner.names(),
            plan.node.as_ref(),
            self.planner.cluster_networks(),
        ) else {
            return Ok(Vec::new());
        };
        let Ok(Some(stored)) = host.stored() else {
            return Ok(Vec::new());
        };
        let key = |network: &str| {
            crate::domain::cluster_network::network_key(plan.project_id, &plan.environment, network)
        };
        // (service, network, how many addresses the node should answer).
        let idle = self.idle_services(plan, stack);
        let counts = self.node_counts(plan, stack);
        let expected = plan
            .networks
            .iter()
            .filter(|network| stored.local_network(&key(&network.logical_name)).is_some())
            .flat_map(|network| {
                network
                    .members
                    .iter()
                    .filter(|member| member.stack == stack && !idle.contains(&member.service))
                    .map(|member| (member.service.as_str(), network.logical_name.as_str()))
            })
            .filter_map(|(service, network)| match wanted {
                Wanted::Twice(started) => started
                    .iter()
                    .any(|name| name == service)
                    .then_some((service, network, true)),
                Wanted::Once(only) => only
                    .iter()
                    .any(|name| name == service)
                    .then_some((service, network, false)),
                Wanted::Every => Some((service, network, false)),
            })
            .collect::<Vec<_>>();
        if expected.is_empty() {
            return Ok(Vec::new());
        }
        let waited = tokio::time::Instant::now();
        loop {
            let report = names.report(&key(""));
            let listed = |service: &str, network: &str| {
                report
                    .names
                    .iter()
                    .filter(|name| name.name == service && name.network == network)
                    .flat_map(|name| name.answers())
                    .filter(|address| address.node == node.node)
                    .map(|address| address.address)
                    .collect::<Vec<_>>()
            };
            let missing = expected.iter().find(|(service, network, new)| {
                let listed = listed(service, network);
                // One more than the node runs is the old ones and the new
                // one, also when an interrupted start had left the new one
                // already.
                if *new {
                    let running = counts.get(*service).copied().unwrap_or(1);
                    listed.len() <= usize::try_from(running).unwrap_or(usize::MAX)
                        && listed.iter().all(|address| before.contains(address))
                } else {
                    listed.is_empty()
                }
            });
            let Some((service, network, _)) = missing else {
                return Ok(expected
                    .iter()
                    .flat_map(|(service, network, _)| listed(service, network))
                    .collect());
            };
            if waited.elapsed() >= OWN_RESOLVER_PATIENCE {
                return Err(StepFailure {
                    stack: stack.to_owned(),
                    service: Some((*service).to_owned()),
                    reason: format!(
                        "its container is healthy, but this node's resolver does not answer with it on network '{network}' after {} s; `mm service status {service}` shows what each node knows",
                        OWN_RESOLVER_PATIENCE.as_secs()
                    ),
                    log: Vec::new(),
                    code: None,
                });
            }
            tokio::time::sleep(OWN_RESOLVER_LOOK).await;
        }
    }
}

/// Which of a stack's containers a node's resolver has to answer with
/// before a step is done.
#[derive(Debug, Clone, Copy)]
enum Wanted<'a> {
    /// Every service of the stack, one container each.
    Every,
    /// These services, one container each.
    Once(&'a [String]),
    /// These services, a new container each next to the old: two answered,
    /// or one the node did not answer with before the start (the old one
    /// is not on a network the service just joined).
    Twice(&'a [String]),
}

/// The error of a step that failed and was undone: the cause, with what
/// the undo could not do.
fn undo_error(operation_id: OperationId, cause: MmError, failures: Vec<String>) -> MmError {
    if failures.is_empty() {
        return cause;
    }
    MmError::operation_failure(
        operation_id.0.to_string(),
        crate::domain::error::MutationStatus::Unknown,
        cause,
        failures,
        "the host could not be returned to what it ran before; `mm deploy plan` plans from what it runs now",
    )
}
