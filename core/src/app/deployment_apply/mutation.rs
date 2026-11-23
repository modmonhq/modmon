use super::*;
use crate::domain::rollout::StepFailure;

impl DeploymentService {
    /// Applies `plan` stack by stack, up to the routes (`settle`);
    /// `changes` notes each stack before it changes, so a failure undoes
    /// only what changed.
    pub(super) async fn apply_stacks(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
        changes: &mut crate::domain::rollout::RolloutChanges,
    ) -> Result<(), MmError> {
        self.prepare_workloads(plan, target).await?;
        self.sweep_jobs(selection, plan, target).await?;
        for stack in plan
            .rollout
            .ordered(&plan.stacks, |stack| stack.stack.as_str())
        {
            changes.note(&stack.stack, true);
            let jobs = self
                .run_stack_jobs(selection, (plan, target), &stack.stack, environments)
                .await?;
            let failure = match jobs {
                Some(failure) => Some(failure),
                None => {
                    changes.note(&stack.stack, false);
                    match self
                        .start_beside_old(plan, target, stack, environments)
                        .await?
                    {
                        Err(failure) => Some(failure),
                        Ok(_) => self.apply_stack(plan, target, stack, environments).await?,
                    }
                }
            };
            // With the runtime's own code: a convergence failure is a
            // runtime failure, with the service and what it logged.
            if let Some(failure) = failure {
                let message = format!(
                    "the deployment did not become healthy: {}",
                    failure.with_log()
                );
                return Err(match crate::domain::rollout::unhealthy_code(&failure) {
                    "MM_RUNTIME_RESPONSE" => MmError::RuntimeResponse { message },
                    code => MmError::Remote { code, message },
                });
            }
        }
        Ok(())
    }

    /// What follows the last healthy stack: the routes, then everything the
    /// plan no longer runs.
    pub(super) async fn settle(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<DeploymentResult, MmError> {
        let wrote_dynamic_config = self.publish_ingress_config(plan, target).await?;
        self.remove_obsolete_resources(plan, target).await?;
        let route_set = self
            .commit_route_claims(plan, target, wrote_dynamic_config)
            .await?;
        self.planner
            .reconcile_local_certificates(&plan.complete_route_claims)?;

        Ok(DeploymentResult {
            plan_hash: plan.plan_hash.clone(),
            target_name: plan.target_name.clone(),
            target_id: plan.target_id,
            environment: plan.environment.clone(),
            applied_stacks: plan
                .stacks
                .iter()
                .map(|stack| stack.stack.clone())
                .collect(),
            removed_stacks: plan
                .removed_stacks
                .iter()
                .map(|stack| stack.stack.clone())
                .collect(),
            route_generation: route_set.generation,
            route_count: route_set.claims.len(),
            moved_away: plan
                .node
                .iter()
                .flat_map(|node| node.moved().cloned())
                .collect(),
            jobs: Vec::new(),
        })
    }

    /// What every stack needs before its first container starts: the
    /// networks, the kept volumes, and the target's ingress.
    pub(super) async fn prepare_workloads(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<(), MmError> {
        self.ensure_managed_networks(target, plan).await?;
        self.ensure_preserved_volumes(target, plan).await?;
        if let Some(ingress) = &plan.static_ingress {
            self.prepare_ingress(target, ingress).await?;
            self.ports
                .runtime
                .apply(&ingress_runtime_target(plan, target, ingress))
                .await?;
            self.ports
                .verifier
                .verify_listener(ingress.artifact.manifest.proxy_verify_port)
                .await?;
        }
        Ok(())
    }

    /// For the stack's `start-first` services: starts the new container
    /// next to the old one and waits until it is healthy. Returns the
    /// services that now run two containers, or why a new one is not
    /// healthy; the old containers are untouched either way.
    pub(super) async fn start_beside_old(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        stack: &DeploymentStackPlan,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    ) -> Result<Result<Vec<String>, StepFailure>, MmError> {
        let services = plan.rollout.start_first(&stack.stack);
        if services.is_empty() {
            return Ok(Ok(services));
        }
        let runtime_target = stack_runtime_target(plan, target, stack, environments, false);
        self.pull_images(plan, &runtime_target, Some(&services))
            .await?;
        let started = self
            .ports
            .runtime
            .start_beside(&runtime_target, &services)
            .await?;
        if started.is_empty() {
            return Ok(Ok(started));
        }
        Ok(
            match self
                .await_health(plan, &runtime_target, Some(&started))
                .await?
            {
                Some(failure) => Err(failure),
                None => Ok(started),
            },
        )
    }

    /// Pulls the images `services` of the target's stack (every service
    /// with `None`) lack, before any of them starts or stops, within the
    /// plan's `deploy.pull-timeout`.
    pub(super) async fn pull_images(
        &self,
        plan: &DeploymentPlan,
        runtime_target: &RuntimeTarget,
        services: Option<&[String]>,
    ) -> Result<(), MmError> {
        if services.is_some_and(<[String]>::is_empty) {
            return Ok(());
        }
        self.ports
            .runtime
            .pull_images(runtime_target, services, plan.rollout.pull_timeout())
            .await
    }

    /// Applies one stack and waits until it is healthy; `Some` says why it
    /// is not. An old container that `start_beside_old` replaced goes
    /// first, and on a cluster node it leaves (`domain::leaving`): out of
    /// every node's answers and ingress proxies, its drain, its stop
    /// signal.
    pub(super) async fn apply_stack(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        stack: &DeploymentStackPlan,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    ) -> Result<Option<StepFailure>, MmError> {
        let runtime_target = stack_runtime_target(plan, target, stack, environments, false);
        self.pull_images(
            plan,
            &runtime_target,
            self.awaited_services(plan, stack).as_deref(),
        )
        .await?;
        let start_first = plan.rollout.start_first(&stack.stack);
        if !start_first.is_empty() {
            self.retire_replaced(plan, &runtime_target, &start_first)
                .await?;
        }
        self.retire_outdated(plan, &runtime_target, stack, &start_first)
            .await?;
        // A cluster node removes what left it itself (`moves.rs`), so a
        // service that moved keeps running while it starts elsewhere. It
        // runs its share of each scaled service.
        if plan.node.is_some() {
            self.await_scaled_leaves(plan, &stack.stack).await;
            let counts = self.apply_counts(plan, &runtime_target).await?;
            self.ports
                .runtime
                .apply_scaled(&runtime_target, &counts, true)
                .await?;
            self.remove_departed_services(plan, target, Some(&stack.stack))
                .await?;
        } else {
            self.ports.runtime.apply(&runtime_target).await?;
        }
        self.remove_job_only_containers(plan, &runtime_target)
            .await?;
        match self.awaited_services(plan, stack) {
            // The node runs none of the stack's services now.
            Some(awaited) if awaited.is_empty() => Ok(None),
            awaited => {
                self.await_health(plan, &runtime_target, awaited.as_deref())
                    .await
            }
        }
    }

    /// The old containers of `services` that a new one runs beside: each
    /// service's leave together, with its own drain, on a cluster node;
    /// removed at once on a single host.
    async fn retire_replaced(
        &self,
        plan: &DeploymentPlan,
        runtime_target: &RuntimeTarget,
        services: &[String],
    ) -> Result<(), MmError> {
        let Some(leaving) = self.planner.leaving() else {
            let replaced = self
                .ports
                .runtime
                .replaced_containers(runtime_target, services)
                .await?;
            return self
                .ports
                .runtime
                .remove_containers(runtime_target, &replaced)
                .await;
        };
        let leaves = services.iter().map(|service| async move {
            let replaced = self
                .ports
                .runtime
                .replaced_containers(runtime_target, std::slice::from_ref(service))
                .await?;
            if replaced.is_empty() {
                return Ok(());
            }
            let drain = plan.rollout.drain(&runtime_target.stack_name, service);
            leaving.leave(&replaced, drain).await
        });
        futures_util::future::join_all(leaves)
            .await
            .into_iter()
            .collect::<Result<Vec<()>, MmError>>()
            .map(drop)
    }

    /// On a cluster node: a `stop-first` container the apply is about to
    /// replace leaves first (`domain::leaving`) when another node serves
    /// its service, so nothing is sent to it while it stops. A service
    /// that runs only here keeps its short gap: with nobody else to serve
    /// it, a drain would only make the gap longer. `skip` are the services
    /// the step leaves alone here: `start-first` ones, whose old container
    /// leaves once the new one runs, or what an ahead step does not start.
    pub(super) async fn retire_outdated(
        &self,
        plan: &DeploymentPlan,
        runtime_target: &RuntimeTarget,
        stack: &DeploymentStackPlan,
        skip: &[String],
    ) -> Result<(), MmError> {
        let (Some(leaving), Some(names), Some(node)) = (
            self.planner.leaving(),
            self.planner.names(),
            plan.node.as_ref(),
        ) else {
            return Ok(());
        };
        let report = names
            .report(&crate::domain::cluster_network::network_key(
                plan.project_id,
                &plan.environment,
                "",
            ))
            .only_networks(|network| plan.networks.iter().any(|own| own.logical_name == network));
        let elsewhere = |service: &str| {
            report
                .names
                .iter()
                .filter(|name| name.name == service)
                .flat_map(|name| name.answers())
                .any(|answer| answer.node != node.node)
        };
        let services = stack
            .artifact
            .manifest
            .services
            .iter()
            .map(|service| service.service.as_str())
            .filter(|service| !skip.iter().any(|skipped| skipped == service) && elsewhere(service))
            .collect::<Vec<_>>();
        let leaves = services.into_iter().map(|service| async move {
            let outdated = self
                .ports
                .runtime
                .outdated_containers(runtime_target, &[service.to_owned()])
                .await?;
            if outdated.is_empty() {
                return Ok(());
            }
            leaving
                .leave(&outdated, plan.rollout.drain(&stack.stack, service))
                .await
        });
        futures_util::future::join_all(leaves)
            .await
            .into_iter()
            .collect::<Result<Vec<()>, MmError>>()
            .map(drop)
    }

    async fn publish_ingress_config(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<bool, MmError> {
        if !manages_ingress(plan) {
            return Ok(false);
        }
        let volume = ingress_dynamic_config_volume_name(plan.target_id);
        self.ports
            .runtime
            .ensure_volume(target, &volume, INGRESS_CONFIG_RESOURCE)
            .await?;
        for certificate in &plan.local_certificates {
            for host in &certificate.hosts {
                let (certificate_path, key_path) = self.planner.local_certificate_paths(host)?;
                self.ports
                    .runtime
                    .replace_volume_file(
                        target,
                        &volume,
                        INGRESS_CONFIG_RESOURCE,
                        &format!("{host}.crt"),
                        &certificate_path,
                        0o644,
                    )
                    .await?;
                self.ports
                    .runtime
                    .replace_volume_file(
                        target,
                        &volume,
                        INGRESS_CONFIG_RESOURCE,
                        &format!("{host}.key"),
                        &key_path,
                        0o600,
                    )
                    .await?;
            }
        }
        self.ports
            .runtime
            .replace_volume_file(
                target,
                &volume,
                INGRESS_CONFIG_RESOURCE,
                "dynamic.yaml",
                &plan.dynamic_config.path,
                0o644,
            )
            .await?;
        let verify_port = crate::domain::target_ingress::ingress_verify_port(plan.target_id);
        if let Err(error) = self.verify_routes(plan, verify_port).await {
            return Err(self
                .restore_previous_routes(target, plan, error, true)
                .await);
        }
        if let Some(ingress) = &plan.static_ingress {
            if let Err(error) = self.ports.rootless.install(ingress).await {
                return Err(self
                    .restore_previous_routes(target, plan, error, true)
                    .await);
            }
            if let Err(error) = self.ports.privileged.install(ingress).await {
                return Err(self
                    .restore_previous_routes(target, plan, error, true)
                    .await);
            }
        }
        Ok(true)
    }

    async fn remove_obsolete_resources(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<(), MmError> {
        self.remove_departed_services(plan, target, None).await?;
        let node = plan.node.as_ref();
        for stack in &plan.removed_stacks {
            // A stack whose services moved away stays until they are
            // stopped; stopping the last one removes the project.
            if node.is_some_and(|node| node.stack_has_moved(&stack.stack)) {
                continue;
            }
            self.ports
                .runtime
                .remove(&stack_runtime_target(plan, target, stack, None, false))
                .await?;
        }
        for volume in &plan.removed_preserved_volumes {
            self.ports
                .runtime
                .remove_volume(target, &volume.physical_name, &volume.resource)
                .await?;
        }
        // A moved service still holds its networks.
        if node.is_none_or(|node| node.moved().next().is_none()) {
            self.remove_unused_networks(target, plan).await?;
        }
        if let Some(ingress) = &plan.removed_ingress {
            self.ports.privileged.uninstall(ingress).await?;
            self.ports.rootless.uninstall(ingress).await?;
        }
        Ok(())
    }

    async fn commit_route_claims(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        wrote_dynamic_config: bool,
    ) -> Result<crate::domain::ingress::IngressRouteSet, MmError> {
        if plan.route_state_changed {
            return match self.ports.routes.replace_owner_claims_cas(
                crate::app::ports::IngressClaimsReplacement {
                    target_id: plan.target_id,
                    project_id: plan.project_id,
                    environment: &plan.environment,
                    expected_generation: plan.expected_route_generation,
                    claims: &plan.desired_route_claims,
                    middlewares: &plan.desired_middlewares,
                    preserve_gate_attachments: true,
                    config_digest: Some(&plan.dynamic_config.manifest.digest),
                    now_unix_ms: self.ports.clock.now_unix_ms(),
                },
            ) {
                Ok(route_set) => Ok(route_set),
                Err(error) => Err(self
                    .restore_previous_routes(target, plan, error, wrote_dynamic_config)
                    .await),
            };
        }
        let route_set = self.ports.routes.route_set(plan.target_id)?;
        if route_set.generation != plan.expected_route_generation {
            return Err(MmError::State {
                message: "ingress routes changed after the reviewed deployment plan".to_owned(),
            });
        }
        Ok(route_set)
    }

    pub(super) async fn ensure_managed_networks(
        &self,
        target: &NamedTarget,
        plan: &DeploymentPlan,
    ) -> Result<(), MmError> {
        // On a cluster node the networks the reviewed plan gave addresses
        // span the nodes; every other network stays local to the host.
        let cluster = match (&plan.node, self.planner.cluster_networks()) {
            (Some(_), Some(host)) => host.stored()?.map(|stored| (host, stored)),
            _ => None,
        };
        for network in &plan.networks {
            if network.members.is_empty() {
                continue;
            }
            let resource = managed_network_resource(&network.logical_name);
            let spanning = cluster.as_ref().and_then(|(_, stored)| {
                stored.local_network(&crate::domain::cluster_network::network_key(
                    plan.project_id,
                    &plan.environment,
                    &network.logical_name,
                ))
            });
            let runtime = &self.ports.runtime;
            match spanning {
                Some(spanning) => {
                    runtime
                        .ensure_cluster_network(
                            target,
                            &network.physical_name,
                            &resource,
                            &spanning,
                        )
                        .await?
                }
                None if network.internal => {
                    runtime
                        .ensure_internal_network(target, &network.physical_name, &resource)
                        .await?
                }
                None => {
                    runtime
                        .ensure_network(target, &network.physical_name, &resource)
                        .await?
                }
            };
        }
        // Routes, firewall rules, and bridge settings follow the networks,
        // before any container starts on them.
        if let Some((host, _)) = cluster {
            host.converge().await?;
        }
        Ok(())
    }

    pub(super) async fn ensure_preserved_volumes(
        &self,
        target: &NamedTarget,
        plan: &DeploymentPlan,
    ) -> Result<(), MmError> {
        for volume in &plan.preserved_volumes {
            self.ports
                .runtime
                .ensure_volume(target, &volume.physical_name, &volume.resource)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn remove_unused_networks(
        &self,
        target: &NamedTarget,
        plan: &DeploymentPlan,
    ) -> Result<(), MmError> {
        for network in &plan.networks {
            if !network.members.is_empty() {
                continue;
            }
            self.ports
                .runtime
                .remove_network(
                    target,
                    &network.physical_name,
                    &managed_network_resource(&network.logical_name),
                )
                .await?;
        }
        Ok(())
    }

    pub(super) async fn prepare_ingress(
        &self,
        target: &NamedTarget,
        ingress: &TargetIngressPlan,
    ) -> Result<(), MmError> {
        let manifest = &ingress.artifact.manifest;
        crate::app::ensure_ingress_network(
            self.ports.runtime.as_ref(),
            target,
            manifest,
            INGRESS_NETWORK_RESOURCE,
        )
        .await?;
        self.ports
            .runtime
            .ensure_volume(target, &manifest.acme_volume_name, INGRESS_ACME_RESOURCE)
            .await?;
        self.ports
            .runtime
            .ensure_volume(
                target,
                &manifest.dynamic_config_volume_name,
                INGRESS_CONFIG_RESOURCE,
            )
            .await?;
        self.ports
            .runtime
            .create_volume_file_if_absent(
                target,
                &manifest.acme_volume_name,
                INGRESS_ACME_RESOURCE,
                "acme.json",
                0o600,
            )
            .await?;
        self.ports
            .runtime
            .create_volume_file_if_absent(
                target,
                &manifest.dynamic_config_volume_name,
                INGRESS_CONFIG_RESOURCE,
                "dynamic.yaml",
                0o644,
            )
            .await
    }

    pub(super) async fn verify_routes(
        &self,
        plan: &DeploymentPlan,
        verify_port: u16,
    ) -> Result<(), MmError> {
        self.ports.verifier.verify_listener(verify_port).await?;
        for claim in &plan.desired_route_claims {
            self.ports
                .verifier
                .verify_route(verify_port, &claim.host, &claim.path_prefix)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn restore_previous_routes(
        &self,
        target: &NamedTarget,
        plan: &DeploymentPlan,
        original: MmError,
        wrote_dynamic_config: bool,
    ) -> MmError {
        if !wrote_dynamic_config {
            return original;
        }
        let volume = ingress_dynamic_config_volume_name(plan.target_id);
        let restored = self
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
            .await;
        match restored {
            Ok(()) => original,
            Err(rollback) => MmError::Security {
                message: format!(
                    "deployment failed ({original}) and the previous ingress routes could not be restored ({rollback})"
                ),
            },
        }
    }
}

/// Whether applying `plan` writes the target's route configuration: it has
/// routes, had routes, or changes them. A host without any has no ingress
/// volume and no proxy to verify, on the way forward or back.
pub(super) fn manages_ingress(plan: &DeploymentPlan) -> bool {
    plan.static_ingress.is_some()
        || plan.expected_route_config_digest.is_some()
        || plan.route_state_changed
}
