use super::*;

struct DeploymentPreparation {
    environment_review: crate::domain::environment::EnvironmentReview,
    compiled: crate::app::stacks::DeploymentStackCompilation,
    runtime_secret_items: Vec<crate::domain::runtime_secret::RuntimeSecretPlanItem>,
    target_fingerprint: String,
    current: crate::domain::ingress::IngressRouteSet,
    previous_dynamic_config: crate::domain::traefik_dynamic::PreparedTraefikDynamicConfig,
}

struct DeploymentRoutePlanning {
    previous_owner_claims: Vec<IngressRouteClaim>,
    adopted_route_claims: Vec<IngressRouteClaim>,
    desired_route_claims: Vec<IngressRouteClaim>,
    complete_route_claims: Vec<IngressRouteClaim>,
    desired_middlewares: BTreeMap<String, crate::domain::ingress::CanonicalMiddleware>,
    previous_deployment: Option<DeploymentPlan>,
    route_state_changed: bool,
    local_certificates: Vec<crate::domain::ca::IssuedCertificate>,
    dynamic_config: crate::domain::traefik_dynamic::PreparedTraefikDynamicConfig,
    static_ingress: Option<crate::domain::target_ingress::TargetIngressPlan>,
}

struct DeploymentRemovals {
    removed_stacks: Vec<DeploymentStackPlan>,
    removed_ingress: Option<crate::domain::target_ingress::TargetIngressPlan>,
    preserved_volumes: Vec<crate::domain::development_mount::PreservedVolumeManifest>,
    removed_preserved_volumes: Vec<crate::domain::development_mount::PreservedVolumeManifest>,
}

impl DeploymentManager {
    /// The stacks with the images of the local build, the receipt, or this
    /// machine's builds, in that order.
    fn compile(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<crate::app::stacks::DeploymentStackCompilation, MmError> {
        match &self.local_build {
            Some(build) => {
                self.stacks
                    .compile_for_local_build(selection, self.state.as_ref(), build.plan())
            }
            None => self.stacks.compile_for_deployment(
                selection,
                self.state.as_ref(),
                self.receipt.as_ref().map(|promoted| &promoted.receipt),
            ),
        }
    }

    pub async fn plan(&self, selection: &WorkspaceSelection) -> Result<DeploymentPlan, MmError> {
        let mut preparation = self.prepare_deployment(selection).await?;
        let routes = self
            .plan_deployment_routes(
                &preparation.compiled,
                &preparation.current,
                &mut preparation.runtime_secret_items,
            )
            .await?;
        let registries = self.environments.registries(selection)?;
        let mut placement = std::mem::take(&mut preparation.compiled.placement);
        // The plan's own part of the notes stays with the compilation.
        preparation.compiled.placement.config_files = config_files_since(
            std::mem::take(&mut placement.config_files),
            routes.previous_deployment.as_ref(),
        );
        preparation.compiled.placement.user_ids = std::mem::take(&mut placement.user_ids);
        // A service a host policy no longer puts under a runtime moves
        // without anything else in the plan saying so.
        let previous_stacks = routes
            .previous_deployment
            .as_ref()
            .map(|previous| previous.stacks.clone())
            .unwrap_or_default();
        let mut plan = self.finish_deployment_plan(preparation, routes)?;
        self.only_kept_that_run(&mut plan).await?;
        plan.runtime_changes =
            crate::domain::hardening::runtime_changes(&previous_stacks, &plan.stacks);
        plan.registries = registries;
        let tunnel = self.environments.environment_tunnel(selection)?;
        if let Some(scope) = &selection.node {
            let certificate = self.environments.environment_tls(selection)?;
            let mut node = self
                .node_project(scope, &plan, placement, certificate)
                .await?;
            if let (Some(ingress), Some(tunnel)) = (node.ingress.as_mut(), tunnel) {
                ingress.tunnel = Some(self.node_tunnel(selection, tunnel).await?);
            }
            plan.node = Some(node);
        } else if selection.host_values && tunnel.is_some() {
            return Err(MmError::Environment {
                message: "ingress.type cloudflared applies on a cluster target, one that lists `nodes:`; a target of one host takes its routes on ports 80 and 443. Remove `type: cloudflared` from this environment, or list the host under `nodes:`".to_owned(),
            });
        }
        if !plan.registries.is_empty() || plan.node.is_some() {
            plan.plan_hash = plan
                .canonical_hash()
                .map_err(|message| MmError::Security { message })?;
        }
        Ok(plan)
    }

    async fn prepare_deployment(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<DeploymentPreparation, MmError> {
        let compiled = self.compile(selection)?;
        let environment_review = self
            .environments
            .review_for_deployment(
                &selection.with_release(compiled.release.clone()),
                &compiled.target,
            )
            .await?;
        let runtime_secret_items = self.environments.validate_secret_delivery(
            selection,
            compiled.selection.runtime,
            &environment_review.secret_revisions,
        )?;
        self.owners_hold_under_remap(selection, &compiled).await?;
        if let Some(required) = environment_review
            .delivery
            .mode
            .required_runtime_capability()
        {
            let probe = self.runtime.probe().await;
            if probe.kind != compiled.selection.runtime {
                return Err(MmError::Security {
                    message: "the runtime adapter does not match direct-delivery planning"
                        .to_owned(),
                });
            }
            probe.require_capabilities(
                "direct_environment_delivery",
                &[required],
                Some(&compiled.selection.target),
            )?;
        }
        let target = &compiled.target;
        if target.definition.transport != TargetTransport::Local {
            return Err(MmError::Target {
                message: format!(
                    "target '{}' is not local; remote deployment planning is not implemented",
                    target.name
                ),
            });
        }
        let target_fingerprint = target.definition.canonical_fingerprint();
        if target.fingerprint != target_fingerprint {
            return Err(MmError::Security {
                message: "resolved deployment target fingerprint does not match its definition"
                    .to_owned(),
            });
        }
        let current = self.routes.route_set(target.definition.id)?;
        if current.target_id != target.definition.id {
            return Err(MmError::State {
                message: format!(
                    "ingress route registry returned target '{}' while planning target '{}'",
                    current.target_id.0, target.definition.id.0
                ),
            });
        }
        let previous_local_hosts = local_tls_hosts(&current.claims);
        if !previous_local_hosts.is_empty() {
            self.local_ca.issue(&previous_local_hosts)?;
        }
        let previous_rendered = crate::app::render_traefik_dynamic_routes(
            target.definition.id,
            &current.claims,
            &current.middlewares,
            &previous_local_hosts,
        )
        .map_err(|error| MmError::State {
            message: format!("could not render the currently claimed target routes: {error}"),
        })?;
        if let Some(expected_digest) = &current.config_digest
            && expected_digest != &previous_rendered.manifest.digest
        {
            return Err(MmError::State {
                message: format!(
                    "the stored route digest of target '{}' does not match its stored routes: mm's state database was changed outside mm, or a release that writes routes differently deployed to the target last. Run `mm deploy plan` again with that release, or after restoring the database",
                    target.name
                ),
            });
        }
        let previous_dynamic_config = self
            .dynamic_configs
            .publish(target.definition.id, &previous_rendered)?;
        Ok(DeploymentPreparation {
            environment_review,
            compiled,
            runtime_secret_items,
            target_fingerprint,
            current,
            previous_dynamic_config,
        })
    }

    /// Refuses owned files whose owner Docker's `userns-remap` hides.
    async fn owners_hold_under_remap(
        &self,
        selection: &WorkspaceSelection,
        compiled: &crate::app::stacks::DeploymentStackCompilation,
    ) -> Result<(), MmError> {
        if compiled.selection.runtime != crate::domain::runtime::RuntimeKind::DockerCompose {
            return Ok(());
        }
        let owned = self
            .stacks
            .files_outside_host_namespace(selection, compiled)?;
        if owned.is_empty() || !self.runtime.probe().await.remaps_user_ids() {
            return Ok(());
        }
        Err(MmError::Remote {
            code: "MM_USER_NAMESPACE",
            message: format!(
                "Docker on this host runs with userns-remap, so its containers do not have the host's user ids, and {} would belong to an id they do not have. Give each of these entries uid \"0\", gid \"0\" and a mode every user can read, such as 0o444, run the service in the host's user namespace (`userns_mode: host` in its Compose file), or turn off `userns-remap` in /etc/docker/daemon.json and restart Docker",
                owned.join(", ")
            ),
        })
    }

    async fn plan_deployment_routes(
        &self,
        compiled: &crate::app::stacks::DeploymentStackCompilation,
        current: &crate::domain::ingress::IngressRouteSet,
        runtime_secret_items: &mut [crate::domain::runtime_secret::RuntimeSecretPlanItem],
    ) -> Result<DeploymentRoutePlanning, MmError> {
        let target = &compiled.target;
        let mut desired_route_claims = compiled.route_claims.clone();
        let mut desired_middlewares = compiled.middlewares.clone();
        desired_route_claims.sort_by(canonical_claim_order);
        validate_desired_claims(
            target.definition.id,
            compiled.project_id,
            &compiled.environment,
            &desired_route_claims,
        )?;

        let (mut previous_owner_claims, mut complete_route_claims): (Vec<_>, Vec<_>) =
            current.claims.iter().cloned().partition(|claim| {
                claim.project_id == compiled.project_id && claim.environment == compiled.environment
            });
        preserve_gate_attachments(
            &previous_owner_claims,
            &current.middlewares,
            &mut desired_route_claims,
            &mut desired_middlewares,
        )?;
        complete_route_claims.extend(desired_route_claims.iter().cloned());
        complete_route_claims.sort_by(canonical_claim_order);
        previous_owner_claims.sort_by(canonical_claim_order);
        let (previous_deployment, adopted_route_claims) = self.previous_deployment(
            compiled,
            current,
            &previous_owner_claims,
            runtime_secret_items,
        )?;
        let complete_middlewares = merge_middlewares(
            &current.middlewares,
            &complete_route_claims,
            &desired_middlewares,
        );
        let previous_owner_middlewares =
            owner_middlewares(&previous_owner_claims, &current.middlewares);
        let route_state_changed = !adopted_route_claims.is_empty()
            || previous_owner_claims != desired_route_claims
            || previous_owner_middlewares != desired_middlewares;
        reject_route_conflicts(&complete_route_claims)?;

        let local_hosts = local_tls_hosts(&complete_route_claims);
        let local_certificates = if local_hosts.is_empty() {
            Vec::new()
        } else {
            self.local_ca.issue(&local_hosts)?
        };
        let rendered = crate::app::render_traefik_dynamic_routes(
            target.definition.id,
            &complete_route_claims,
            &complete_middlewares,
            &local_hosts,
        )
        .map_err(|error| MmError::Target {
            message: format!("could not render complete target routes: {error}"),
        })?;
        let dynamic_config = self
            .dynamic_configs
            .publish(target.definition.id, &rendered)?;

        let static_ingress = if complete_route_claims.is_empty() {
            None
        } else {
            Some(
                self.target_ingress
                    .plan_resolved_for_runtime(target, compiled.selection.runtime)?,
            )
        };

        Ok(DeploymentRoutePlanning {
            previous_owner_claims,
            adopted_route_claims,
            desired_route_claims,
            complete_route_claims,
            desired_middlewares,
            previous_deployment,
            route_state_changed,
            local_certificates,
            dynamic_config,
            static_ingress,
        })
    }

    fn finish_deployment_plan(
        &self,
        preparation: DeploymentPreparation,
        routes: DeploymentRoutePlanning,
    ) -> Result<DeploymentPlan, MmError> {
        let DeploymentPreparation {
            environment_review,
            compiled,
            runtime_secret_items,
            target_fingerprint,
            current,
            previous_dynamic_config,
        } = preparation;
        let DeploymentRoutePlanning {
            previous_owner_claims,
            adopted_route_claims,
            desired_route_claims,
            complete_route_claims,
            desired_middlewares,
            previous_deployment,
            route_state_changed,
            local_certificates,
            dynamic_config,
            static_ingress,
        } = routes;
        let runtime_secret_plan = runtime_secret_plan(&compiled, runtime_secret_items)?;
        let kept = self.kept_by_failed_first(&compiled, previous_deployment.as_ref())?;
        let stacks = deployment_stacks(
            compiled.project_id,
            &compiled.project_name,
            compiled.target.definition.id,
            &compiled.target.name,
            &compiled.environment,
            compiled.stacks,
            compiled.project_directories,
        )?;
        let removals = deployment_removals(
            (&stacks, &compiled.excluded_services),
            (previous_deployment.as_ref(), kept),
            complete_route_claims.is_empty()
                && !self.installation_owns_ingress(compiled.target.definition.id)?,
        );
        let route_diff = route_diff(&previous_owner_claims, &desired_route_claims)?;
        let mut plan = DeploymentPlan {
            excluded_services: compiled.excluded_services,
            schema: crate::domain::deployment::DEPLOYMENT_PLAN_SCHEMA.to_owned(),
            project_id: compiled.project_id,
            project_name: compiled.project_name,
            target_name: compiled.target.name.clone(),
            target_id: compiled.target.definition.id,
            target_fingerprint,
            environment: compiled.environment,
            runtime: Some(compiled.selection.runtime),
            runtime_source: Some(compiled.selection.runtime_source.as_str().to_owned()),
            workload_intent_hash: Some(compiled.workload_intent_hash.clone()),
            networks: compiled.networks.clone(),
            expected_route_generation: current.generation,
            expected_route_config_digest: current.config_digest,
            expected_previous_plan_hash: previous_deployment.map(|previous| previous.plan_hash),
            stacks,
            removed_stacks: removals.removed_stacks,
            preserved_volumes: removals.preserved_volumes,
            removed_preserved_volumes: removals.removed_preserved_volumes,
            previous_owner_claims,
            adopted_route_claims,
            desired_route_claims,
            complete_route_claims,
            desired_middlewares,
            environment_review,
            runtime_secret_plan,
            scheduler_reconciliation: None,
            environment_bundle_root: None,
            previous_dynamic_config,
            dynamic_config,
            local_certificates,
            static_ingress,
            removed_ingress: removals.removed_ingress,
            route_diff,
            ingress: compiled.ingress,
            route_state_changed,
            build_receipt: self.receipt.as_ref().map(|receipt| receipt.origin.clone()),
            local_build: self.local_build_review(),
            registries: BTreeMap::new(),
            node: None,
            rollout: compiled.rollout,
            config_files: compiled.placement.config_files,
            runtime_changes: Vec::new(),
            unknown_config_keys: compiled.unknown_keys,
            secret_file_owners: Vec::new(),
            release: compiled.release,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Security { message })?;
        Ok(plan)
    }

    /// Without a finished deployment, the removals are stacks failed first
    /// deployments kept: those with no container left are dropped.
    async fn only_kept_that_run(&self, plan: &mut DeploymentPlan) -> Result<(), MmError> {
        if plan.expected_previous_plan_hash.is_some() || plan.removed_stacks.is_empty() {
            return Ok(());
        }
        let mut running = Vec::new();
        for stack in std::mem::take(&mut plan.removed_stacks) {
            let services = self.runtime.project_services(&stack.compose_project_name);
            if !services.await?.is_empty() {
                running.push(stack);
            }
        }
        plan.removed_stacks = running;
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Security { message })?;
        Ok(())
    }

    /// The stacks first deployments that failed left running, when no
    /// deployment ever finished: the plan removes those it does not run.
    fn kept_by_failed_first(
        &self,
        compiled: &crate::app::stacks::DeploymentStackCompilation,
        previous: Option<&DeploymentPlan>,
    ) -> Result<Vec<DeploymentStackPlan>, MmError> {
        if previous.is_some() {
            return Ok(Vec::new());
        }
        let target_id = compiled.target.definition.id;
        Ok(self
            .state
            .deployment_candidates_for_owner(compiled.project_id, &compiled.environment, target_id)?
            .into_iter()
            .filter(|(state, _)| *state == OperationState::Failed)
            .flat_map(|(_, failed)| failed.stacks)
            .collect())
    }

    /// The deployment the plan follows, and the environment's routes it did
    /// not record, which the plan takes back.
    fn previous_deployment(
        &self,
        compiled: &crate::app::stacks::DeploymentStackCompilation,
        current: &crate::domain::ingress::IngressRouteSet,
        previous_owner_claims: &[IngressRouteClaim],
        runtime_secret_items: &mut [crate::domain::runtime_secret::RuntimeSecretPlanItem],
    ) -> Result<(Option<DeploymentPlan>, Vec<IngressRouteClaim>), MmError> {
        let target_id = compiled.target.definition.id;
        let owned = |previous: &DeploymentPlan| -> Result<bool, MmError> {
            let canonical = previous
                .canonical_hash()
                .map_err(|message| MmError::State { message })?;
            Ok(previous.project_id == compiled.project_id
                && previous.target_id == target_id
                && previous.environment == compiled.environment
                && canonical == previous.plan_hash)
        };
        let mut previous_deployment = None;
        for (state, previous) in self.state.deployment_candidates_for_owner(
            compiled.project_id,
            &compiled.environment,
            target_id,
        )? {
            if state == OperationState::Running
                && previous.route_state_changed
                && previous.complete_route_claims == current.claims
                && current.config_digest.as_deref()
                    == Some(previous.dynamic_config.manifest.digest.as_str())
                && owned(&previous)?
            {
                previous_deployment = Some(previous);
                break;
            }
        }
        let recorded = if previous_deployment.is_some() {
            true
        } else {
            previous_deployment = self.state.latest_successful_deployment_for_owner(
                compiled.project_id,
                &compiled.environment,
                target_id,
            )?;
            if let Some(previous) = &previous_deployment
                && !owned(previous)?
            {
                return Err(MmError::Security {
                    message:
                        "the last finished deployment snapshot does not match the planned owner"
                            .to_owned(),
                });
            }
            let recorded = previous_deployment
                .as_ref()
                .map(|previous| previous.desired_route_claims.as_slice())
                .unwrap_or_default();
            recorded
                == claims_without_gate_attachments(previous_owner_claims, &current.middlewares)
                    .as_slice()
        };
        if let Some(previous) = &previous_deployment
            && let Some(previous_secrets) = &previous.runtime_secret_plan
        {
            for item in runtime_secret_items.iter_mut() {
                item.previous_source_revision = previous_secrets
                    .items
                    .iter()
                    .find(|previous| previous.logical_name == item.logical_name)
                    .map(|previous| previous.source_revision.clone())
                    .filter(|revision| revision != &item.source_revision);
            }
        }
        let adopted = if recorded {
            Vec::new()
        } else {
            previous_owner_claims.to_vec()
        };
        Ok((previous_deployment, adopted))
    }
}

/// The plan's config files, each with the version the last finished
/// deployment mounted at the same path of the same service.
fn config_files_since(
    mut files: Vec<crate::domain::config_file::ConfigFileUse>,
    previous: Option<&DeploymentPlan>,
) -> Vec<crate::domain::config_file::ConfigFileUse> {
    for file in &mut files {
        file.previous = previous
            .into_iter()
            .flat_map(|plan| &plan.config_files)
            .find(|earlier| earlier.same_mount(file))
            .map(|earlier| earlier.version.clone());
    }
    files.sort();
    files
}

fn runtime_secret_plan(
    compiled: &crate::app::stacks::DeploymentStackCompilation,
    items: Vec<crate::domain::runtime_secret::RuntimeSecretPlanItem>,
) -> Result<Option<crate::domain::runtime_secret::RuntimeSecretPlan>, MmError> {
    if items.is_empty() {
        return Ok(None);
    }
    let first_host_ids = compiled
        .placement
        .user_ids
        .iter()
        .map(|(service, ids)| (service.clone(), ids.first))
        .collect();
    crate::domain::runtime_secret::RuntimeSecretPlan::new(
        compiled.project_id.0.to_string(),
        compiled.target.definition.id.0.to_string(),
        compiled.environment.clone(),
        compiled.selection.runtime,
        items,
    )
    .and_then(|plan| plan.with_first_host_ids(&first_host_ids))
    .map(Some)
    .map_err(|message| MmError::Secret { message })
}

/// What the previous deployment had, or what failed first deployments
/// kept, and this one does not. The preserved
/// volumes of a module that `development:` turned off stay for its return.
fn deployment_removals(
    (stacks, excluded): (
        &[DeploymentStackPlan],
        &[crate::domain::stack::ExcludedService],
    ),
    (previous, kept): (Option<&DeploymentPlan>, Vec<DeploymentStackPlan>),
    remove_ingress: bool,
) -> DeploymentRemovals {
    let mut removed_stacks = previous
        .map(|previous| previous.stacks.clone())
        .unwrap_or_default()
        .into_iter()
        .chain(kept)
        .filter(|old| {
            !stacks
                .iter()
                .any(|desired| desired.compose_project_name == old.compose_project_name)
        })
        .collect::<Vec<_>>();
    removed_stacks.sort_by(|left, right| {
        left.stack
            .cmp(&right.stack)
            .then_with(|| left.compose_project_name.cmp(&right.compose_project_name))
    });
    removed_stacks.dedup_by(|left, right| left.compose_project_name == right.compose_project_name);
    let removed_ingress = if remove_ingress {
        previous.and_then(|plan| plan.static_ingress.clone())
    } else {
        None
    };
    let mut preserved_volumes = stacks
        .iter()
        .flat_map(|stack| stack.artifact.manifest.preserved_volumes.iter().cloned())
        .collect::<Vec<_>>();
    preserved_volumes.sort_by(|left, right| left.physical_name.cmp(&right.physical_name));
    preserved_volumes.dedup_by(|left, right| left.physical_name == right.physical_name);
    let desired = preserved_volumes
        .iter()
        .map(|volume| volume.physical_name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let kept = excluded
        .iter()
        .filter(|service| service.off_locally.is_some())
        .map(|service| service.module.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut removed_preserved_volumes = previous
        .into_iter()
        .flat_map(|plan| plan.preserved_volumes.iter())
        .filter(|volume| {
            !desired.contains(volume.physical_name.as_str())
                && !kept.contains(volume.module.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    removed_preserved_volumes.sort_by(|left, right| left.physical_name.cmp(&right.physical_name));
    removed_preserved_volumes.dedup_by(|left, right| left.physical_name == right.physical_name);
    DeploymentRemovals {
        removed_stacks,
        removed_ingress,
        preserved_volumes,
        removed_preserved_volumes,
    }
}
