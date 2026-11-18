//! Services that do not run one container on each node (`replicas`,
//! `autoscale`): how many a deployment runs of them on this node, and the
//! one step the node's autoscaler asks of a deployment outside one.

use std::collections::BTreeMap;
use std::time::Duration;

use super::*;

/// How often a deployment looks whether a scaled service's leaving
/// container is gone.
const LEAVE_LOOK: Duration = Duration::from_millis(500);

impl DeploymentService {
    /// For each scaled service of `stack` in `plan`: the containers this
    /// node runs of it, its share of the count the nodes agreed on. A
    /// deployment never resets that count; a new range only bounds it.
    pub(super) fn node_counts(&self, plan: &DeploymentPlan, stack: &str) -> BTreeMap<String, u32> {
        let Some(node) = &plan.node else {
            return BTreeMap::new();
        };
        let state = self.planner.scale_state();
        node.scaled
            .iter()
            .filter(|scaled| scaled.stack == stack)
            .map(|scaled| {
                let wanted = state.and_then(|state| state.wanted(&scaled.key()));
                (scaled.service.clone(), scaled.share(&node.node, wanted))
            })
            .collect()
    }

    /// What an apply asks the engine to run of each scaled service of the
    /// stack: the node's share, or the containers it runs already when they
    /// are more. A deployment never removes one of them without its drain:
    /// the node's autoscaler has the extra ones leave afterwards.
    pub(super) async fn apply_counts(
        &self,
        plan: &DeploymentPlan,
        runtime_target: &RuntimeTarget,
    ) -> Result<BTreeMap<String, u32>, MmError> {
        let mut counts = self.node_counts(plan, &runtime_target.stack_name);
        for (service, count) in &mut counts {
            let running = self
                .ports
                .runtime
                .service_container_ids(runtime_target, std::slice::from_ref(service))
                .await?
                .len();
            *count = (*count).max(u32::try_from(running).unwrap_or(u32::MAX));
        }
        Ok(counts)
    }

    /// The services of `stack` that run here and of which the node runs no
    /// container now, and its `role: job` services: nothing waits for them
    /// to become healthy.
    pub(super) fn idle_services(&self, plan: &DeploymentPlan, stack: &str) -> Vec<String> {
        self.node_counts(plan, stack)
            .into_iter()
            .filter(|(_, count)| *count == 0)
            .map(|(service, _)| service)
            .chain(job_only_services(plan, stack))
            .collect()
    }

    /// The services of `stack` to wait for after an apply: every one of
    /// its own, except those the node runs no container of. `None` waits
    /// for all of them.
    pub(super) fn awaited_services(
        &self,
        plan: &DeploymentPlan,
        stack: &DeploymentStackPlan,
    ) -> Option<Vec<String>> {
        let idle = self.idle_services(plan, &stack.stack);
        if idle.is_empty() {
            return None;
        }
        Some(
            stack
                .artifact
                .manifest
                .services
                .iter()
                .map(|service| service.service.clone())
                .filter(|service| !idle.contains(service))
                .collect(),
        )
    }

    /// Removes the containers a `role: job` service of the target's stack
    /// still has from before it was one.
    pub(super) async fn remove_job_only_containers(
        &self,
        plan: &DeploymentPlan,
        runtime_target: &RuntimeTarget,
    ) -> Result<(), MmError> {
        let services = job_only_services(plan, &runtime_target.stack_name);
        if services.is_empty() {
            return Ok(());
        }
        let ids = self
            .ports
            .runtime
            .service_container_ids(runtime_target, &services)
            .await?;
        self.ports
            .runtime
            .remove_containers(runtime_target, &ids)
            .await
    }

    /// Waits until no container of a scaled service of `stack` is leaving
    /// this node: Compose still counts one until it is gone, and would
    /// remove another in its place.
    pub(super) async fn await_scaled_leaves(&self, plan: &DeploymentPlan, stack: &str) {
        let (Some(node), Some(state)) = (&plan.node, self.planner.scale_state()) else {
            return;
        };
        for scaled in node.scaled.iter().filter(|scaled| scaled.stack == stack) {
            let key = scaled.key();
            while state.leaving(&key) > 0 {
                tokio::time::sleep(LEAVE_LOOK).await;
            }
        }
    }

    /// The node's autoscaler: has Compose count `count` containers of
    /// `service` in stack `stack`, as the last deployment `plan_hash` here
    /// runs it, starting the ones it misses with that deployment's files
    /// and environment. Refused while a deployment holds the target, and
    /// for any deployment but the last one applied here.
    pub async fn scale_service(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
        (stack, service): (&str, &str),
        count: u32,
    ) -> Result<(), MmError> {
        let document = self.ports.documents.load_from(&selection.start)?;
        let environment = crate::domain::config::resolve_environment(
            &document,
            selection.environment.as_deref(),
        )?;
        let (environment_pref, project_pref) = crate::app::load_selection_preferences(
            Some(self.ports.state.as_ref()),
            &document,
            &environment,
        )?;
        let (_, target) = crate::app::resolve_selection(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            self.ports.targets.as_ref(),
        )?;
        let _guard = self.ports.locks.acquire(target.definition.id)?;
        let plan = self
            .ports
            .state
            .latest_successful_deployment_for_owner(
                document.config.project.id,
                &environment,
                target.definition.id,
            )?
            .filter(|plan| plan.plan_hash == plan_hash)
            .ok_or_else(|| MmError::State {
                message: format!(
                    "deployment plan {plan_hash} is not the last one applied here; the service is scaled after the next deployment"
                ),
            })?;
        validate_target(&plan, &target)?;
        let stack_plan = plan
            .stacks
            .iter()
            .find(|rendered| rendered.stack == stack)
            .ok_or_else(|| MmError::State {
                message: format!("plan {plan_hash} has no stack '{stack}' on this host"),
            })?;
        let mut environment = self
            .prepare_environment(selection, &plan, OperationId::new())
            .await?;
        let runtime_target =
            stack_runtime_target(&plan, &target, stack_plan, environment.bundle(), false);
        let started = self
            .ports
            .runtime
            .scale_service(&runtime_target, service, count)
            .await;
        environment.dispose()?;
        started
    }

    /// The containers of the services that moved away from this node leave
    /// (`domain::leaving`) before their services are removed: out of every
    /// node's answers and ingress proxies, their drain, their stop signal.
    pub(super) async fn leave_moved(
        &self,
        plan: &DeploymentPlan,
        runtime_target: &RuntimeTarget,
        services: &[String],
    ) -> Result<(), MmError> {
        let Some(leaving) = self.planner.leaving() else {
            return Ok(());
        };
        let leaves = services.iter().map(|service| async move {
            let ids = self
                .ports
                .runtime
                .service_container_ids(runtime_target, std::slice::from_ref(service))
                .await?;
            if ids.is_empty() {
                return Ok(());
            }
            leaving
                .leave(
                    &ids,
                    plan.rollout.drain(&runtime_target.stack_name, service),
                )
                .await
        });
        futures_util::future::join_all(leaves)
            .await
            .into_iter()
            .collect::<Result<Vec<()>, MmError>>()
            .map(drop)
    }
}

/// The `role: job` services of `stack` in `plan`.
pub(super) fn job_only_services(plan: &DeploymentPlan, stack: &str) -> Vec<String> {
    plan.stacks
        .iter()
        .filter(|candidate| candidate.stack == stack)
        .flat_map(|candidate| &candidate.artifact.manifest.services)
        .filter(|service| service.is_job_only())
        .map(|service| service.service.clone())
        .collect()
}
