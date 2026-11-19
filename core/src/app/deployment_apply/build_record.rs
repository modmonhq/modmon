use super::*;

impl DeploymentService {
    /// Deploys the images of a build made elsewhere, such as CI, instead of
    /// this machine's latest published build.
    #[must_use]
    pub fn with_receipt(mut self, receipt: crate::domain::build::PromotedReceipt) -> Self {
        self.planner = self.planner.with_receipt(receipt);
        self
    }

    /// Deploys the environment's current local build, built first when the
    /// plan says it is stale (`mm dev up`).
    #[must_use]
    pub fn with_local_build(mut self, build: crate::app::LocalBuild) -> Self {
        self.planner = self.planner.with_local_build(build);
        self
    }

    /// Runs the local build the reviewed plan `expected_plan_hash` starts
    /// with; nothing when it has none or this machine already ran it.
    pub async fn build_reviewed(
        &self,
        selection: &WorkspaceSelection,
        expected_plan_hash: &str,
    ) -> Result<Option<crate::domain::build::ImageBuildResult>, MmError> {
        let Some(build) = self.planner.local_build() else {
            return Ok(None);
        };
        if build.review().is_none() {
            return Ok(None);
        }
        let preview = self.plan(selection).await?;
        require_plan_hash(&preview, expected_plan_hash)?;
        build.run(selection).await
    }

    /// Records the receipt's build as this workspace's latest published
    /// build, so status, logs, and later plans of the same checkout resolve
    /// the digests this deployment runs. A receipt already recorded is kept.
    pub(super) fn record_receipt(
        &self,
        workspace: &crate::domain::workspace::WorkspaceRecord,
        now: i64,
    ) -> Result<(), MmError> {
        let Some(receipt) = self.planner.receipt().map(|promoted| &promoted.receipt) else {
            return Ok(());
        };
        receipt
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let plan = &receipt.plan;
        let recorded = self.ports.state.latest_successful_image_build(
            plan.project_id,
            &workspace.root,
            &plan.environment,
        )?;
        if recorded.is_some_and(|(recorded, result)| {
            recorded.plan_hash == plan.plan_hash && result.images == receipt.result.images
        }) {
            return Ok(());
        }
        let id = OperationId::new();
        self.ports.state.begin_operation(&NewOperation {
            id,
            workspace_id: workspace.id,
            kind: OperationKind::Build,
            environment: plan.environment.clone(),
            requested_at_unix_ms: now,
            plan_hash: Some(plan.plan_hash.clone()),
        })?;
        let mut result = receipt.result.clone();
        result.operation_id = id;
        let saved = self
            .ports
            .state
            .save_image_build_plan(id, plan)
            .and_then(|()| self.ports.state.save_image_build_result(id, &result));
        let (state, code) = match &saved {
            Ok(()) => (OperationState::Succeeded, None),
            Err(error) => (OperationState::Failed, Some(error.code())),
        };
        self.ports
            .state
            .finish_operation(id, state, self.ports.clock.now_unix_ms(), code)?;
        saved
    }
}
