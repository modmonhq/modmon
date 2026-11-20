use super::*;

impl DeploymentService {
    #[must_use]
    pub fn new(planner: DeploymentManager, ports: DeploymentExecutionPorts) -> Self {
        Self {
            planner,
            ports,
            jobs: None,
        }
    }

    /// The jobs a deployment runs before a service is replaced, and the
    /// clean-up of what runs that were cut off left.
    #[must_use]
    pub fn with_jobs(
        mut self,
        jobs: std::sync::Arc<dyn crate::app::ports::DeploymentJobs>,
    ) -> Self {
        self.jobs = Some(jobs);
        self
    }

    /// On a cluster node: the host's cluster networks (see
    /// [`DeploymentManager::with_cluster_networks`]).
    #[must_use]
    pub fn with_cluster_networks(
        mut self,
        networks: std::sync::Arc<dyn crate::app::ports::ClusterNetworkHost>,
    ) -> Self {
        self.planner = self.planner.with_cluster_networks(networks);
        self
    }

    /// On a cluster node: the node's name resolver (see
    /// [`DeploymentManager::with_names`]).
    #[must_use]
    pub fn with_names(
        mut self,
        names: std::sync::Arc<dyn crate::app::ports::ServiceNames>,
    ) -> Self {
        self.planner = self.planner.with_names(names);
        self
    }

    /// On a cluster node: how containers leave, and the node's ingress
    /// proxy (see [`DeploymentManager::with_node_host`]).
    #[must_use]
    pub fn with_node_host(
        mut self,
        leaving: std::sync::Arc<dyn crate::app::ports::ContainerLeaving>,
        ingress: std::sync::Arc<dyn crate::app::ports::IngressHost>,
    ) -> Self {
        self.planner = self.planner.with_node_host(leaving, ingress);
        self
    }

    /// On a cluster node: the count of each scaled service (see
    /// [`DeploymentManager::with_scale_state`]).
    #[must_use]
    pub fn with_scale_state(
        mut self,
        scale: std::sync::Arc<dyn crate::app::ports::ScaleState>,
    ) -> Self {
        self.planner = self.planner.with_scale_state(scale);
        self
    }

    pub async fn plan(&self, selection: &WorkspaceSelection) -> Result<DeploymentPlan, MmError> {
        let mut plan = self.planner.plan(selection).await?;
        let (_, scheduler) = self.scheduler_reconciliation(selection, &plan)?;
        plan.scheduler_reconciliation = Some(scheduler);
        if let (Some(user), Some(secrets)) = (
            self.ports.runtime_secrets.capability().files_kept_for,
            &plan.runtime_secret_plan,
        ) {
            plan.secret_file_owners = secrets.kept_for(user);
        }
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Security { message })?;
        Ok(plan)
    }

    pub async fn apply_reviewed(
        &self,
        selection: &WorkspaceSelection,
        expected_plan_hash: &str,
    ) -> Result<DeploymentResult, MmError> {
        self.build_reviewed(selection, expected_plan_hash).await?;
        let ReviewedApply {
            plan,
            target,
            operation,
            guard: _guard,
        } = self.begin_reviewed(selection, expected_plan_hash).await?;
        let operation_id = operation.id;
        let result: Result<DeploymentResult, MmError> = async {
            self.record_phase(operation_id, OperationPhase::Validate)?;
            self.record_phase(operation_id, OperationPhase::Apply)?;
            let result = self
                .apply_with_environment(
                    selection,
                    &plan,
                    &target,
                    operation_id,
                    operation.previous.as_ref(),
                )
                .await?;
            Ok(result)
        }
        .await;
        let closed = self.close_operation(selection, &plan, operation_id, result);
        if closed.is_ok() {
            self.prune_superseded_versions(&plan).await;
        }
        closed
    }

    /// Everything a reviewed apply does before it changes a workload:
    /// checks the plan is still the reviewed one, takes the target's lock,
    /// returns an interrupted deployment to what ran before it, and opens
    /// the operation in the journal.
    pub(super) async fn begin_reviewed(
        &self,
        selection: &WorkspaceSelection,
        expected_plan_hash: &str,
    ) -> Result<ReviewedApply, MmError> {
        // Both plans are made again here: a refusal says it met an apply.
        let replanned = crate::app::environment::refused_at_apply;
        let preview = self.plan(selection).await.map_err(replanned)?;
        require_plan_hash(&preview, expected_plan_hash)?;
        let guard = self.ports.locks.acquire(preview.target_id)?;
        self.reconcile_environment_artifacts()?;

        let plan = self.plan(selection).await.map_err(replanned)?;
        require_plan_hash(&plan, expected_plan_hash)?;
        let target = self.ports.targets.resolve_target(&plan.target_name)?;
        validate_target(&plan, &target)?;
        self.require_runtime(&plan).await?;
        self.planner.check_prerequisites(selection).await?;
        let document = self.ports.documents.load_from(&selection.start)?;
        let now = self.ports.clock.now_unix_ms();
        let workspace = self.ports.state.register_workspace(
            &WorkspaceRegistration {
                project_id: plan.project_id,
                root: document.root,
                config_path: document.path,
            },
            now,
        )?;
        self.recover_stale_operations(selection, workspace.id, &plan, &target, now)
            .await?;
        self.record_receipt(&workspace, now)?;
        self.reconcile_environment_artifacts()?;
        self.reconcile_runtime_secrets(&plan, true).await?;
        self.preflight_runtime(&plan, &target).await?;
        let operation = self.begin_operation(selection, &plan)?;
        Ok(ReviewedApply {
            plan,
            target,
            operation,
            guard,
        })
    }

    /// Closes the operation of an apply that ran to its end, one way or
    /// the other.
    pub(super) fn close_operation(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
        operation_id: OperationId,
        result: Result<DeploymentResult, MmError>,
    ) -> Result<DeploymentResult, MmError> {
        match result {
            Ok(result) => {
                if let Err(scheduler) = self.apply_scheduler_reconciliation(selection, plan) {
                    completed_mutation_journal(
                        operation_id,
                        self.finish_operation(operation_id, OperationState::Succeeded, None),
                    )?;
                    return Err(MmError::operation_failure(
                        operation_id.0.to_string(),
                        crate::domain::error::MutationStatus::Applied,
                        scheduler,
                        Vec::new(),
                        "the deployment converged; repair scheduler storage and retry the reviewed deployment",
                    ));
                }
                completed_mutation_journal(
                    operation_id,
                    self.finish_operation(operation_id, OperationState::Succeeded, None),
                )?;
                Ok(result)
            }
            Err(error) => {
                if let Err(journal_error) =
                    self.finish_operation(operation_id, OperationState::Failed, Some(error.code()))
                {
                    return Err(MmError::operation_failure(
                        operation_id.0.to_string(),
                        crate::domain::error::MutationStatus::Unknown,
                        error,
                        vec![journal_error.to_string()],
                        "inspect target state and repair the running operation journal before retrying",
                    ));
                }
                Err(error)
            }
        }
    }

    /// Stops the project's containers without touching volumes, routes, or deployment records.
    ///
    /// The next reviewed apply starts every stack again from the same artifacts.
    pub async fn stop(&self, selection: &WorkspaceSelection) -> Result<Vec<String>, MmError> {
        let plan = self.plan(selection).await?;
        let _guard = self.ports.locks.acquire(plan.target_id)?;
        let target = self.ports.targets.resolve_target(&plan.target_name)?;
        validate_target(&plan, &target)?;
        self.require_runtime(&plan).await?;
        let mut stopped = Vec::with_capacity(plan.stacks.len());
        for stack in &plan.stacks {
            self.ports
                .runtime
                .remove(&stack_runtime_target(&plan, &target, stack, None, false))
                .await?;
            stopped.push(stack.stack.clone());
        }
        Ok(stopped)
    }

    pub(super) fn scheduler_reconciliation(
        &self,
        selection: &WorkspaceSelection,
        deployment: &DeploymentPlan,
    ) -> Result<
        (
            crate::domain::scheduler::DesiredDefinitions,
            crate::domain::scheduler::ReconciliationPlan,
        ),
        MmError,
    > {
        let document = self.ports.documents.load_from(&selection.start)?;
        let environment = crate::domain::config::resolve_environment(
            &document,
            selection.environment.as_deref(),
        )?;
        if document.config.project.id != deployment.project_id
            || environment != deployment.environment
        {
            return Err(MmError::Security {
                message: "scheduler reconciliation does not match the deployment owner".to_owned(),
            });
        }
        let desired = crate::app::desired_definitions(&document, &environment)?;
        let stored = self
            .ports
            .scheduler
            .definition_hashes(document.config.project.id, &environment)?;
        let plan = crate::domain::scheduler::reconciliation_plan(
            document.config.project.id,
            environment,
            document.source_hash,
            &desired,
            &stored,
        )
        .map_err(|message| MmError::State { message })?;
        Ok((desired, plan))
    }

    pub(super) fn apply_scheduler_reconciliation(
        &self,
        selection: &WorkspaceSelection,
        deployment: &DeploymentPlan,
    ) -> Result<(), MmError> {
        let reviewed = deployment
            .scheduler_reconciliation
            .as_ref()
            .ok_or_else(|| MmError::Security {
                message: "reviewed deployment is missing scheduler reconciliation".to_owned(),
            })?;
        let (desired, current) = self.scheduler_reconciliation(selection, deployment)?;
        if &current != reviewed {
            return Err(MmError::Security {
                message: "scheduler definitions changed after deployment review".to_owned(),
            });
        }
        self.ports
            .scheduler
            .reconcile(&crate::domain::scheduler::ReconciliationCommit {
                plan: current,
                desired,
                now_unix_ms: self.ports.clock.now_unix_ms(),
                principal: "deployment".to_owned(),
                source: "deployment".to_owned(),
            })
    }

    pub(super) fn begin_operation(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
    ) -> Result<DeploymentOperation, MmError> {
        let document = self.ports.documents.load_from(&selection.start)?;
        if document.config.project.id != plan.project_id {
            return Err(MmError::Security {
                message: "workspace identity changed after the reviewed deployment plan".to_owned(),
            });
        }
        let now = self.ports.clock.now_unix_ms();
        let workspace = self.ports.state.register_workspace(
            &WorkspaceRegistration {
                project_id: plan.project_id,
                root: document.root,
                config_path: document.path,
            },
            now,
        )?;
        let previous = self.ports.state.latest_successful_deployment_for_owner(
            plan.project_id,
            &plan.environment,
            plan.target_id,
        )?;
        if previous.is_none() && plan.previous_owner_claims != plan.adopted_route_claims {
            return Err(MmError::State {
                message: format!(
                    "no finished deployment of environment '{}' records the routes it holds on target '{}', and the reviewed plan does not take them back; run `mm deploy plan` again and apply the new plan",
                    plan.environment, plan.target_name
                ),
            });
        }
        if previous.as_ref().is_some_and(|previous| {
            previous.project_id != plan.project_id
                || previous.target_id != plan.target_id
                || previous.environment != plan.environment
        }) {
            return Err(MmError::Security {
                message: "previous deployment snapshot does not match the reviewed deployment"
                    .to_owned(),
            });
        }
        if previous
            .as_ref()
            .map(|previous| previous.plan_hash.as_str())
            != plan.expected_previous_plan_hash.as_deref()
        {
            return Err(MmError::State {
                message: "the last finished deployment changed after the reviewed plan; run `mm deploy plan` again and apply the new plan"
                    .to_owned(),
            });
        }
        let operation_id = OperationId::new();
        self.ports.state.begin_operation(&NewOperation {
            id: operation_id,
            workspace_id: workspace.id,
            kind: OperationKind::Deploy,
            environment: plan.environment.clone(),
            requested_at_unix_ms: now,
            plan_hash: Some(plan.plan_hash.clone()),
        })?;
        let mut persisted = plan.clone();
        persisted.environment_bundle_root = Some(
            self.ports
                .environment_artifacts
                .as_ref()
                .bundle_root(operation_id),
        );
        if let Err(error) = self
            .ports
            .state
            .save_deployment_plan(operation_id, &persisted)
        {
            if let Err(journal) = self.ports.state.finish_operation(
                operation_id,
                OperationState::Failed,
                now,
                Some(error.code()),
            ) {
                return Err(MmError::operation_failure(
                    operation_id.0.to_string(),
                    crate::domain::error::MutationStatus::NotStarted,
                    error,
                    vec![journal.to_string()],
                    "repair the incomplete operation journal before retrying",
                ));
            }
            return Err(error);
        }
        Ok(DeploymentOperation {
            id: operation_id,
            previous,
        })
    }

    /// Returns every deployment that began and never finished to what ran
    /// before it: those of this checkout, and those of the same project
    /// and environment made from another checkout (a host keeps one
    /// checkout per commit). The caller holds the target's lock, and, on a
    /// host that rolls out in steps, the environment's deployment lock.
    pub(super) async fn recover_stale_operations(
        &self,
        selection: &WorkspaceSelection,
        workspace_id: WorkspaceId,
        current: &DeploymentPlan,
        target: &NamedTarget,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        let mut stale = self
            .ports
            .state
            .running_operations(workspace_id)?
            .into_iter()
            .filter(|operation| operation.kind == OperationKind::Deploy)
            .map(|operation| operation.id)
            .collect::<Vec<_>>();
        for (operation_id, _) in self.ports.state.running_deployments_for_owner(
            current.project_id,
            &current.environment,
            current.target_id,
        )? {
            if !stale.contains(&operation_id) {
                stale.push(operation_id);
            }
        }
        for operation_id in stale {
            self.recover_stale_operation(selection, operation_id, current, target, now_unix_ms)
                .await?;
        }
        Ok(())
    }

    async fn recover_stale_operation(
        &self,
        selection: &WorkspaceSelection,
        operation_id: OperationId,
        current: &DeploymentPlan,
        target: &NamedTarget,
        now_unix_ms: i64,
    ) -> Result<(), MmError> {
        let snapshot = self.ports.state.deployment_plan(operation_id)?;
        if snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.target_id != current.target_id || snapshot.environment != current.environment
        }) {
            return Ok(());
        }
        if snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.runtime != current.runtime)
        {
            return Err(MmError::Security {
                message: "an interrupted deployment used another runtime; select that runtime to recover it"
                    .to_owned(),
            });
        }
        let committed = if let Some(snapshot) = &snapshot
            && snapshot.route_state_changed
        {
            let routes = self.ports.routes.route_set(current.target_id)?;
            routes.claims == snapshot.complete_route_claims
                && routes.config_digest.as_deref()
                    == Some(snapshot.dynamic_config.manifest.digest.as_str())
        } else {
            false
        };
        if !committed && let Some(snapshot) = &snapshot {
            self.recover_interrupted_deployment(selection, operation_id, snapshot, current, target)
                .await?;
        }
        if let Some(root) = snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.environment_bundle_root.as_deref())
        {
            dispose_bundle_root(self.ports.environment_artifacts.as_ref(), root)?;
        }
        self.ports.state.finish_operation(
            operation_id,
            if committed {
                OperationState::Succeeded
            } else {
                OperationState::Interrupted
            },
            now_unix_ms,
            if committed {
                None
            } else {
                Some("MM_INTERRUPTED_RECOVERED")
            },
        )
    }

    async fn recover_interrupted_deployment(
        &self,
        selection: &WorkspaceSelection,
        operation_id: OperationId,
        snapshot: &DeploymentPlan,
        current: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<(), MmError> {
        let previous = self.ports.state.latest_successful_deployment_for_owner(
            snapshot.project_id,
            &snapshot.environment,
            snapshot.target_id,
        )?;
        let root = snapshot
            .environment_bundle_root
            .clone()
            .unwrap_or_else(|| self.ports.environment_artifacts.bundle_root(operation_id));
        dispose_bundle_root(self.ports.environment_artifacts.as_ref(), &root)?;
        // Environment values are resolved from current configuration, so bind recovery
        // to the newly reviewed plan rather than the interrupted plan's stale metadata.
        let mut environments = self
            .prepare_environment(selection, current, operation_id)
            .await?;
        let recovery = self
            .rollback_runtime(
                snapshot,
                target,
                previous.as_ref(),
                environments.bundle(),
                None,
            )
            .await;
        let cleanup = environments.dispose();
        match (recovery, cleanup) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(rollback), Ok(())) => Err(MmError::Security {
                message: format!(
                    "interrupted deployment {} could not be recovered: {}",
                    operation_id.0, rollback
                ),
            }),
            (Ok(()), Err(cleanup)) => Err(cleanup),
            (Err(rollback), Err(cleanup)) => Err(MmError::Security {
                message: format!(
                    "interrupted deployment {} recovery failed with {} and cleanup failed with {}",
                    operation_id.0,
                    rollback.code(),
                    cleanup.code()
                ),
            }),
        }
    }
}

pub(super) fn completed_mutation_journal(
    operation_id: OperationId,
    journal: Result<(), MmError>,
) -> Result<(), MmError> {
    journal.map_err(|journal| {
        MmError::operation_failure(
            operation_id.0.to_string(),
            crate::domain::error::MutationStatus::Applied,
            journal,
            Vec::new(),
            "the deployment converged; repair the operation journal before another mutation",
        )
    })
}
