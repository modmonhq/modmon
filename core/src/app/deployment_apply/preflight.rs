use super::*;

impl DeploymentService {
    pub(super) fn reconcile_environment_artifacts(&self) -> Result<(), MmError> {
        let active = self.ports.state.running_operation_ids()?;
        self.ports.environment_artifacts.reconcile(&active)
    }

    pub(super) async fn reconcile_runtime_secrets(
        &self,
        plan: &DeploymentPlan,
        retain_previous_deployment: bool,
    ) -> Result<(), MmError> {
        let previous = self.ports.state.latest_successful_deployment_for_owner(
            plan.project_id,
            &plan.environment,
            plan.target_id,
        )?;
        let runtime_plan = plan.runtime_secret_plan.as_ref().or_else(|| {
            previous
                .as_ref()
                .and_then(|plan| plan.runtime_secret_plan.as_ref())
        });
        let Some(runtime_plan) = runtime_plan else {
            return Ok(());
        };
        let mut retained = BTreeMap::<String, std::collections::BTreeSet<String>>::new();
        if let Some(desired) = &plan.runtime_secret_plan {
            retain_runtime_plan_revisions(desired, retain_previous_deployment, &mut retained);
        }
        if retain_previous_deployment
            && let Some(previous) = previous
                .as_ref()
                .and_then(|plan| plan.runtime_secret_plan.as_ref())
        {
            retain_runtime_plan_revisions(previous, false, &mut retained);
        }
        let active = self.ports.state.running_operation_ids()?;
        self.ports
            .runtime_secrets
            .reconcile(runtime_plan, &retained, &active)
            .await?;
        Ok(())
    }

    /// Removes the config file versions and secret revisions of the
    /// environment that the deployment replaced. Only once the operation is
    /// journalled as succeeded: until then a failure or a rollback needs
    /// them. Every container now mounts the plan's versions, so a version
    /// that could not be removed changes nothing a container sees; the next
    /// deployment removes it, and a failure here is not the deployment's.
    pub(super) async fn prune_superseded_versions(&self, plan: &DeploymentPlan) {
        if let Err(error) = self.planner.prune_config_files(plan) {
            tracing::warn!(%error, "could not remove superseded config file versions");
        }
        if let Err(error) = self.reconcile_runtime_secrets(plan, false).await {
            tracing::warn!(%error, "superseded runtime secret revisions were not removed");
        }
    }

    pub(super) fn record_phase(
        &self,
        operation_id: OperationId,
        phase: OperationPhase,
    ) -> Result<(), MmError> {
        self.ports.state.append_event(
            operation_id,
            self.ports.clock.now_unix_ms(),
            &OperationEvent::PhaseChanged {
                operation_id,
                phase,
            },
        )
    }

    pub(super) fn finish_operation(
        &self,
        operation_id: OperationId,
        state: OperationState,
        error_code: Option<&str>,
    ) -> Result<(), MmError> {
        self.ports.state.finish_operation(
            operation_id,
            state,
            self.ports.clock.now_unix_ms(),
            error_code,
        )
    }

    pub(super) async fn require_runtime(&self, plan: &DeploymentPlan) -> Result<(), MmError> {
        let probe = self.ports.inspector.probe().await;
        if Some(probe.kind) != plan.runtime {
            return Err(MmError::Security {
                message: "the runtime adapter does not match the reviewed deployment plan"
                    .to_owned(),
            });
        }
        let required = probe.kind.apply_capability();
        let mut capabilities = vec![required, RuntimeCapability::ObserveServices];
        if let Some(required) = plan
            .environment_review
            .delivery
            .mode
            .required_runtime_capability()
        {
            capabilities.push(required);
        }
        if !plan.networks.is_empty()
            || plan.static_ingress.is_some()
            || plan.removed_ingress.is_some()
        {
            capabilities.push(RuntimeCapability::ManageNetworks);
        }
        if plan.static_ingress.is_some()
            || plan.removed_ingress.is_some()
            || plan.route_state_changed
        {
            capabilities.extend([
                RuntimeCapability::ManageVolumes,
                RuntimeCapability::MutateVolumeFiles,
            ]);
        }
        if !plan.preserved_volumes.is_empty() || !plan.removed_preserved_volumes.is_empty() {
            capabilities.extend([
                RuntimeCapability::BindMounts,
                RuntimeCapability::ManageVolumes,
            ]);
        } else if plan.stacks.iter().any(|stack| {
            stack
                .artifact
                .manifest
                .services
                .iter()
                .any(|service| service.development_mount.is_some())
        }) {
            capabilities.push(RuntimeCapability::BindMounts);
        }
        if plan.runtime_secret_plan.is_some() {
            capabilities.push(RuntimeCapability::RuntimeSecrets);
        }
        capabilities.sort();
        capabilities.dedup();
        // Rootless Podman is driven by its user and rootful Podman by root
        // (`mmd` on a remote host): the adapter's probe refuses anything
        // else, so it is not available here.
        if let Some(node) = &plan.node
            && let Some(late) = probe.late_engine(Some(&format!("node '{}'", node.node)))
        {
            return Err(late);
        }
        probe.require_capabilities("apply", &capabilities, Some(&plan.target_name))
    }

    /// Waits until the target's project is healthy: every service, or the
    /// ones `only` names. `Some` says which service is not, why, and what
    /// its container logged last.
    pub(super) async fn await_health(
        &self,
        plan: &DeploymentPlan,
        target: &RuntimeTarget,
        only: Option<&[String]>,
    ) -> Result<Option<crate::domain::rollout::StepFailure>, MmError> {
        let policy = ConvergencePolicy {
            timeout: plan.rollout.convergence_timeout(),
            ..ConvergencePolicy::default()
        };
        let inspector = &self.ports.inspector;
        let cancellation = ConvergenceCancellation::default();
        let outcome = match only {
            Some(services) => {
                inspector
                    .wait_for_services(target, services, policy, cancellation)
                    .await?
            }
            None => {
                inspector
                    .wait_for_convergence(target, policy, cancellation)
                    .await?
            }
        };
        if outcome.state == ConvergenceState::Succeeded {
            return Ok(None);
        }
        let mut reason = outcome.message.clone().unwrap_or_else(|| {
            format!(
                "not healthy ({:?}, diagnostic {})",
                outcome.state,
                outcome.diagnostic_code.as_deref().unwrap_or("none")
            )
        });
        if outcome.state == ConvergenceState::TimedOut {
            reason.push_str(&format!(
                "; if it only needs longer, raise environments.{}.deploy.convergence-timeout",
                plan.environment
            ));
        }
        // The container that is not well, or the newest one of the service.
        let container = outcome.service.as_ref().and_then(|service| {
            let mut members = outcome
                .observation
                .containers
                .iter()
                .filter(|container| &container.service == service)
                .collect::<Vec<_>>();
            members.sort_by_key(|container| {
                let well = container.state.eq_ignore_ascii_case("running")
                    && container
                        .health
                        .as_deref()
                        .is_none_or(|health| health.eq_ignore_ascii_case("healthy"));
                (well, std::cmp::Reverse(container.created_at.clone()))
            });
            members.first().map(|container| container.id.clone())
        });
        let log = match container.filter(|id| !id.is_empty()) {
            Some(container) => inspector
                .container_log_tail(&container, crate::domain::rollout::FAILURE_LOG_LINES)
                .await
                .unwrap_or_else(|error| {
                    vec![format!("(the container's log could not be read: {error})")]
                }),
            None => Vec::new(),
        };
        Ok(Some(crate::domain::rollout::StepFailure {
            stack: target.stack_name.clone(),
            service: outcome.service,
            reason,
            log,
            code: outcome.diagnostic_code,
        }))
    }

    pub(super) async fn preflight_runtime(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<(), MmError> {
        let resources = planned_resources(plan);
        let projects = planned_runtime_targets(plan, target);
        self.ports
            .runtime
            .preflight(target, &resources, &projects)
            .await
    }
}
