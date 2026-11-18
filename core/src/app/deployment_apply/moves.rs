//! Services that leave a cluster node.
//!
//! A node applies its project without removing what the project no longer
//! lists, then removes those services itself. A service that is gone from
//! the environment goes at once. A service that moved to other nodes keeps
//! running until those nodes have applied the same plan: `finish_moves` is
//! the step that has its containers leave (`domain::leaving`) and removes
//! it, and the client calls it last.

use std::collections::BTreeMap;

use super::*;
use crate::domain::deployment::LeavingService;
use crate::domain::placement::PlacedService;

/// The leaving services of each stack that `keep` selects, with the
/// stack's Compose project.
fn by_stack(
    plan: &DeploymentPlan,
    keep: impl Fn(&LeavingService) -> bool,
) -> BTreeMap<&str, (&str, Vec<String>)> {
    let mut stacks = BTreeMap::<&str, (&str, Vec<String>)>::new();
    for leaving in plan.node.iter().flat_map(|node| &node.leaving) {
        if keep(leaving) {
            stacks
                .entry(leaving.stack.as_str())
                .or_insert_with(|| (leaving.compose_project_name.as_str(), Vec::new()))
                .1
                .push(leaving.service.clone());
        }
    }
    stacks
}

/// A stack's project on this host: the one the plan renders, or, for a
/// stack with no service left here, the bare project its containers carry.
fn project_target(
    plan: &DeploymentPlan,
    target: &NamedTarget,
    stack: &str,
    project: &str,
) -> (RuntimeTarget, bool) {
    if let Some(rendered) = plan.stacks.iter().find(|rendered| rendered.stack == stack) {
        return (
            stack_runtime_target(plan, target, rendered, None, false),
            true,
        );
    }
    let bare = RuntimeTarget {
        root: std::path::PathBuf::new(),
        project_directory: std::path::PathBuf::new(),
        target: Some(target.clone()),
        project_id: plan.project_id,
        environment: plan.environment.clone(),
        stack_name: stack.to_owned(),
        project_name: project.to_owned(),
        artifact_hash: String::new(),
        compose_files: Vec::new(),
        user_compose_file_count: 0,
        interpolation_environment: Default::default(),
        installation_scoped: false,
        registry_credentials: Vec::new(),
    };
    (bare, false)
}

impl DeploymentService {
    /// On a cluster node: removes the services that left this node and run
    /// nowhere else. Moved services stay for `finish_moves`. `only` names
    /// one stack the node still renders; without it, the stacks it no
    /// longer renders are cleaned up.
    pub(super) async fn remove_departed_services(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        only: Option<&str>,
    ) -> Result<(), MmError> {
        let Some(node) = &plan.node else {
            return Ok(());
        };
        for (stack, (project, services)) in by_stack(plan, |leaving| !leaving.moved()) {
            let (runtime_target, rendered) = project_target(plan, target, stack, project);
            if only.map_or(rendered, |only| only != stack) {
                continue;
            }
            // The project's networks go with its last service; a moved
            // service of the same stack still needs them.
            let whole_project = !rendered && !node.stack_has_moved(stack);
            self.ports
                .runtime
                .remove_services(&runtime_target, &services, whole_project)
                .await?;
        }
        Ok(())
    }

    /// Stops the services that moved away from this node. `plan_hash` is
    /// the plan this host applied; when a later deployment replaced it,
    /// nothing is stopped and the next plan lists what still runs here.
    pub async fn finish_moves(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
    ) -> Result<Vec<PlacedService>, MmError> {
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
                    "deployment plan {plan_hash} is not the last one applied here, so the services it moved away were not stopped; run `mm deploy plan` again: it lists what still runs on this host"
                ),
            })?;
        validate_target(&plan, &target)?;
        let mut stopped = Vec::new();
        for (stack, (project, services)) in by_stack(&plan, LeavingService::moved) {
            let (runtime_target, rendered) = project_target(&plan, &target, stack, project);
            // Every node runs the service now: these containers leave
            // without dropping a request before they go.
            self.leave_moved(&plan, &runtime_target, &services).await?;
            self.ports
                .runtime
                .remove_services(&runtime_target, &services, !rendered)
                .await?;
            stopped.extend(services.into_iter().map(|service| PlacedService {
                stack: stack.to_owned(),
                service,
            }));
        }
        if !stopped.is_empty() {
            self.remove_unused_networks(&target, &plan).await?;
        }
        Ok(stopped)
    }
}
