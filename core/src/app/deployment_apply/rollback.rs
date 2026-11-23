use super::*;
use crate::domain::environment::PreparedEnvironmentBundle;
use crate::domain::rollout::RolloutChanges;

impl DeploymentService {
    /// Returns the host to `previous`, or removes what `plan` started when
    /// there was nothing before. `changes` says which of the previous
    /// stacks to put back; `None` puts back every one.
    pub(super) async fn rollback_runtime(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        previous: Option<&DeploymentPlan>,
        environment_bundle: Option<&PreparedEnvironmentBundle>,
        changes: Option<&RolloutChanges>,
    ) -> Result<(), MmError> {
        if let Some(previous) = previous {
            self.rollback_to_previous(plan, target, (previous, changes), environment_bundle)
                .await?;
        } else {
            self.rollback_initial_deployment(plan, target, None).await?;
        }
        Ok(())
    }

    async fn rollback_to_previous(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        (previous, changes): (&DeploymentPlan, Option<&RolloutChanges>),
        environment_bundle: Option<&PreparedEnvironmentBundle>,
    ) -> Result<(), MmError> {
        let mut failures = Vec::new();
        let installation_owns_ingress = self.planner.installation_owns_ingress(plan.target_id)?;
        if !installation_owns_ingress
            && let Some(ingress) = &previous.static_ingress
            && let Err(error) = self
                .ports
                .runtime
                .apply(&ingress_runtime_target(previous, target, ingress))
                .await
        {
            failures.push(error);
        }
        for stack in &previous.stacks {
            if let Err(error) = self
                .restore_stack(plan, target, (previous, stack), environment_bundle, changes)
                .await
            {
                failures.push(error);
            }
        }
        for stack in plan.stacks.iter().rev().filter(|stack| {
            !previous
                .stacks
                .iter()
                .any(|candidate| candidate.compose_project_name == stack.compose_project_name)
        }) {
            if let Err(error) = self
                .ports
                .runtime
                .remove(&stack_runtime_target(plan, target, stack, None, false))
                .await
            {
                failures.push(error);
            }
        }
        // A plan that published no routes left none to restore.
        if super::mutation::manages_ingress(plan) {
            self.restore_routes(plan, target, &mut failures).await;
        }
        if !installation_owns_ingress && let Some(ingress) = &previous.static_ingress {
            if let Err(error) = self.ports.rootless.install(ingress).await {
                failures.push(error);
            }
            if let Err(error) = self.ports.privileged.install(ingress).await {
                failures.push(error);
            }
        } else if !installation_owns_ingress && let Some(ingress) = &plan.static_ingress {
            if let Err(error) = self.ports.privileged.uninstall(ingress).await {
                failures.push(error);
            }
            if let Err(error) = self.ports.rootless.uninstall(ingress).await {
                failures.push(error);
            }
        }
        rollback_result(failures)
    }

    /// Puts `stack` of the previous deployment back as far as the rollout
    /// changed it: whole (always, for a stack the plan drops), only the
    /// `ahead` services its jobs had started, or not at all. A stack no step reached keeps running untouched: the
    /// environment is compiled from the newer configuration, so applying
    /// the previous files again would still change its services.
    async fn restore_stack(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        (previous, stack): (&DeploymentPlan, &DeploymentStackPlan),
        environment_bundle: Option<&PreparedEnvironmentBundle>,
        changes: Option<&RolloutChanges>,
    ) -> Result<(), MmError> {
        let mut runtime_target =
            stack_runtime_target(previous, target, stack, environment_bundle, true);
        // The environment is the plan's: its overlays may name a service
        // the previous files do not define, which Compose refuses.
        if let Some(bundle) = environment_bundle {
            let overlays =
                (runtime_target.compose_files).split_off(stack.artifact.compose_files.len());
            let services = (stack.artifact.manifest.services.iter())
                .map(|known| known.service.clone())
                .collect();
            let limited =
                (self.ports.environment_artifacts).limit_overlays(bundle, &overlays, &services)?;
            runtime_target.compose_files.extend(limited);
        }
        // A container a step had begun to take out (its drain) is routed to
        // again, or its stop ends first: none is left half removed.
        if let Some(leaving) = self.planner.leaving() {
            leaving.recall(&runtime_target.project_name).await?;
        }
        // A stack the plan no longer has is removed by its last step.
        let dropped = !(plan.stacks.iter())
            .any(|kept| kept.compose_project_name == stack.compose_project_name);
        // Each scaled service keeps the containers it runs (`apply_counts`).
        let counts = self.apply_counts(previous, &runtime_target).await?;
        let Some(changes) = changes.filter(|changes| !dropped && !changes.whole(&stack.stack))
        else {
            // What the step started leaves first where another node serves
            // its service, as on a stop-first update: no proxy or name
            // sends it a new request while Compose replaces it.
            self.retire_outdated(previous, &runtime_target, stack, &[])
                .await?;
            return self
                .ports
                .runtime
                .apply_scaled(&runtime_target, &counts, false)
                .await;
        };
        if !changes.ahead_only(&stack.stack) {
            return Ok(());
        }
        // A service the previous files do not have is left running.
        let services = plan
            .rollout
            .ahead
            .iter()
            .filter(|placed| placed.stack == stack.stack)
            .filter(|placed| {
                let known = &stack.artifact.manifest.services;
                known.iter().any(|known| known.service == placed.service)
            })
            .map(|placed| placed.service.clone())
            .collect::<Vec<_>>();
        if services.is_empty() {
            return Ok(());
        }
        let others = (stack.artifact.manifest.services.iter())
            .map(|known| known.service.clone())
            .filter(|service| !services.contains(service))
            .collect::<Vec<_>>();
        self.retire_outdated(previous, &runtime_target, stack, &others)
            .await?;
        self.ports
            .runtime
            .apply_services(&runtime_target, &services, &counts)
            .await
    }

    /// Puts the previous route configuration back and checks that the
    /// proxy serves the previous routes.
    async fn restore_routes(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        failures: &mut Vec<MmError>,
    ) {
        let volume = ingress_dynamic_config_volume_name(plan.target_id);
        if let Err(error) = self
            .ports
            .runtime
            .replace_volume_file(
                target,
                &volume,
                INGRESS_CONFIG_RESOURCE,
                "dynamic.yaml",
                &plan.previous_dynamic_config.path,
                0o644,
            )
            .await
        {
            failures.push(error);
        }
        let verify_port = crate::domain::target_ingress::ingress_verify_port(plan.target_id);
        if let Err(error) = self.ports.verifier.verify_listener(verify_port).await {
            failures.push(error);
        }
        for claim in &plan.previous_owner_claims {
            if let Err(error) = self
                .ports
                .verifier
                .verify_route(verify_port, &claim.host, &claim.path_prefix)
                .await
            {
                failures.push(error);
            }
        }
    }

    /// Removes what `plan`, a first deployment, started. With `next`, the
    /// plan applied right after, a stack or network `next` also has stays,
    /// as do the networks of a stack that stays and, when `next` has an
    /// ingress, the ingress.
    pub(super) async fn rollback_initial_deployment(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        next: Option<&DeploymentPlan>,
    ) -> Result<(), MmError> {
        let mut failures = Vec::new();
        let runs_next = |project: &str| {
            next.is_some_and(|next| {
                (next.stacks.iter()).any(|stack| stack.compose_project_name == project)
            })
        };
        let kept = (plan.stacks.iter())
            .filter(|stack| runs_next(&stack.compose_project_name))
            .map(|stack| stack.stack.as_str())
            .collect::<Vec<_>>();
        let network_stays = |network: &crate::domain::stack::CanonicalNetworkIntent| {
            next.is_some_and(|next| {
                (next.networks.iter()).any(|known| known.physical_name == network.physical_name)
            }) || (network.members.iter()).any(|member| kept.contains(&member.stack.as_str()))
        };
        for stack in
            (plan.stacks.iter().rev()).filter(|stack| !runs_next(&stack.compose_project_name))
        {
            if let Err(error) = self
                .ports
                .runtime
                .remove(&stack_runtime_target(plan, target, stack, None, false))
                .await
            {
                failures.push(error);
            }
        }
        for network in plan
            .networks
            .iter()
            .filter(|network| !network.members.is_empty() && !network_stays(network))
        {
            if let Err(error) = self
                .ports
                .runtime
                .remove_network(
                    target,
                    &network.physical_name,
                    &managed_network_resource(&network.logical_name),
                )
                .await
            {
                failures.push(error);
            }
        }
        if let Some(ingress) = &plan.static_ingress
            && next.is_none_or(|next| next.static_ingress.is_none())
            && !self.planner.installation_owns_ingress(plan.target_id)?
        {
            self.remove_initial_ingress(plan, target, (ingress, kept.is_empty()), &mut failures)
                .await;
        }
        rollback_result(failures)
    }

    async fn remove_initial_ingress(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        (ingress, network): (&TargetIngressPlan, bool),
        failures: &mut Vec<MmError>,
    ) {
        if let Err(error) = self.ports.privileged.uninstall(ingress).await {
            failures.push(error);
        }
        if let Err(error) = self.ports.rootless.uninstall(ingress).await {
            failures.push(error);
        }
        let exclusively_owned = plan.complete_route_claims.iter().all(|claim| {
            claim.project_id == plan.project_id && claim.environment == plan.environment
        });
        if exclusively_owned {
            if let Err(error) = self
                .ports
                .runtime
                .remove(&ingress_runtime_target(plan, target, ingress))
                .await
            {
                failures.push(error);
            }
            let manifest = &ingress.artifact.manifest;
            if let Err(error) = self
                .ports
                .runtime
                .remove_volume(
                    target,
                    &manifest.dynamic_config_volume_name,
                    INGRESS_CONFIG_RESOURCE,
                )
                .await
            {
                failures.push(error);
            }
            if let Err(error) = self
                .ports
                .runtime
                .remove_volume(target, &manifest.acme_volume_name, INGRESS_ACME_RESOURCE)
                .await
            {
                failures.push(error);
            }
            if network
                && let Err(error) = self
                    .ports
                    .runtime
                    .remove_network(target, &manifest.network_name, INGRESS_NETWORK_RESOURCE)
                    .await
            {
                failures.push(error);
            }
        }
    }
}

fn rollback_result(failures: Vec<MmError>) -> Result<(), MmError> {
    if failures.is_empty() {
        return Ok(());
    }
    Err(MmError::Security {
        message: format!(
            "deployment rollback encountered {} failure(s): {}",
            failures.len(),
            failures
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        ),
    })
}
