use super::*;

/// Starts, stops, and edits the selected workspace on behalf of the TUI.
pub(super) struct WorkspaceControl {
    pub(super) invocation: super::super::dispatch::Invocation,
}

impl WorkspaceControl {
    fn remote(&self) -> Result<Option<crate::app::RemoteEnvironment>, MmError> {
        let invocation = &self.invocation;
        composition::remote_environment(
            &invocation.selection,
            invocation.home(),
            invocation.runtime_kind,
        )
    }

    /// The selection a deployment from the TUI plans and applies: this
    /// checkout's commit, with no name or inputs given.
    fn released(&self) -> crate::app::WorkspaceSelection {
        let selection = &self.invocation.selection;
        selection.with_release(Some(composition::local_release(
            &selection.start,
            Default::default(),
        )))
    }

    fn deployment(&self) -> Result<crate::app::DeploymentService, MmError> {
        let invocation = &self.invocation;
        invocation
            .services
            .deployment_service(&invocation.selection, invocation.home())
    }

    /// The deployment a start applies, as `mm dev up` does: stale local
    /// images are built first.
    async fn start_deployment(&self) -> Result<crate::app::DeploymentService, MmError> {
        let invocation = &self.invocation;
        invocation
            .services
            .development_deployment(
                &invocation.selection,
                invocation.home(),
                invocation.runtime_kind,
            )
            .await
    }

    fn environment(&self) -> Result<crate::app::EnvironmentManager, MmError> {
        let invocation = &self.invocation;
        Ok(composition::environment_services(
            &invocation.start,
            invocation.home(),
            Arc::clone(&invocation.clock),
        )?
        .manager)
    }
}

fn review(plan: &crate::domain::deployment::DeploymentPlan) -> tui::StartReview {
    tui::StartReview {
        plan_hash: plan.plan_hash.clone(),
        target: plan.target_name.clone(),
        stacks: plan
            .stacks
            .iter()
            .map(|stack| stack.stack.clone())
            .collect(),
        routes: plan.desired_route_claims.len(),
        remote: false,
        nodes: Vec::new(),
        lines: output::local_plan_lines(plan),
        rollback: None,
    }
}

/// The review of a remote plan: what every node section starts, together,
/// and the plan as `mm deploy plan` prints it.
fn remote_review(
    plan: &crate::domain::remote::ClusterDeploymentPlan,
    cluster: bool,
) -> tui::StartReview {
    let deployments = plan
        .nodes
        .iter()
        .filter_map(|section| section.plan.plan.as_ref())
        .collect::<Vec<_>>();
    let mut stacks = Vec::new();
    for deployment in &deployments {
        for stack in &deployment.stacks {
            if !stacks.contains(&stack.stack) {
                stacks.push(stack.stack.clone());
            }
        }
    }
    tui::StartReview {
        plan_hash: plan.plan_hash.clone(),
        target: plan.target.clone(),
        stacks,
        routes: plan.route_count(),
        remote: true,
        nodes: plan
            .nodes
            .iter()
            .filter_map(|section| section.node.clone())
            .collect(),
        lines: output::remote_plan_lines(plan, cluster),
        rollback: None,
    }
}

fn remote_started(
    report: &crate::domain::remote::ClusterApplyReport<crate::domain::deployment::DeploymentResult>,
) -> String {
    let results = report
        .nodes
        .iter()
        .filter_map(|node| node.result.as_ref())
        .collect::<Vec<_>>();
    match results.as_slice() {
        [result] if report.nodes.len() == 1 => result.started_on(&report.target),
        _ => format!(
            "Applied {} on {} node(s) of {}.",
            report.plan_hash.get(..12).unwrap_or(&report.plan_hash),
            report.nodes.len(),
            report.target
        ),
    }
}

#[async_trait::async_trait]
impl tui::WorkspaceControl for WorkspaceControl {
    async fn review_start(&self) -> Result<tui::StartReview, MmError> {
        if let Some(environment) = self.remote()? {
            // The same plan `mm deploy plan` makes: every node, one hash.
            let commit = composition::resolve_commit(&self.invocation.selection.start, None)?;
            let preview = environment.plan_deployment(&commit, None, None).await?;
            return Ok(remote_review(&preview, environment.is_cluster()));
        }
        let plan = self
            .start_deployment()
            .await?
            .plan(&self.released())
            .await?;
        Ok(review(&plan))
    }

    async fn review_rollback(&self) -> Result<tui::StartReview, MmError> {
        let Some(environment) = self.remote()? else {
            return Err(MmError::Environment {
                message: "a rollback returns a remote environment to what its hosts ran before, and this environment runs on this machine. To return to an earlier version here, check out its commit and press u".to_owned(),
            });
        };
        // The same plan `mm deploy rollback` makes.
        let (target, preview) = environment.plan_rollback(None).await?;
        let mut review = remote_review(&preview, environment.is_cluster());
        review.rollback = Some(output::rollback_line(&target));
        Ok(review)
    }

    async fn start(
        &self,
        plan_hash: String,
        progress: tui::ProgressSink,
    ) -> Result<String, MmError> {
        if let Some(environment) = self.remote()? {
            let report = environment
                .apply_deployment(&plan_hash, &|step| progress(step))
                .await?;
            if let Some(error) = report.stopped("mm deploy plan") {
                return Err(error);
            }
            return Ok(remote_started(&report));
        }
        let result = self
            .start_deployment()
            .await?
            .apply_reviewed(&self.released(), &plan_hash)
            .await?;
        Ok(result.started_on(&result.target_name))
    }

    async fn stop(&self) -> Result<String, MmError> {
        if self.remote()?.is_some() {
            return Err(MmError::RuntimeResponse {
                message: "stopping a remote target is not supported; stop it on the remote host"
                    .to_owned(),
            });
        }
        let stopped = self.deployment()?.stop(&self.invocation.selection).await?;
        Ok(if stopped.is_empty() {
            "No stacks are configured; nothing to stop.".to_owned()
        } else {
            format!(
                "Stopped {}. Volumes, routes, and overrides were kept.",
                stopped.join(", ")
            )
        })
    }

    fn set_override(&self, key: &str, value: &str) -> Result<(), MmError> {
        self.environment()?
            .set_override(&self.invocation.selection, key, value, None)
            .map(|_| ())
    }

    fn unset_override(&self, key: &str) -> Result<bool, MmError> {
        self.environment()?
            .unset_override(&self.invocation.selection, key)
    }

    fn secret_for_key(&self, key: &str) -> Result<Option<String>, MmError> {
        let document = composition::project_documents().load_from(&self.invocation.start)?;
        let environment = crate::domain::config::resolve_environment(
            &document,
            self.invocation.selection.environment.as_deref(),
        )?;
        Ok(
            crate::domain::config_resolve::effective_binding(&document.config, &environment, key)
                .and_then(crate::domain::config::EnvironmentBinding::secret_name)
                .map(str::to_owned),
        )
    }
}
