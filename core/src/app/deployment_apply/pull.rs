//! A rollout's images pulled on a node before any node is replaced.

use futures_util::StreamExt as _;

use super::*;

/// How many stacks a node pulls the images of at once.
const STACKS_AT_ONCE: usize = 2;

impl DeploymentService {
    /// Pulls the images every stack of `plan`, this node's reviewed part
    /// of a rollout, runs and the node lacks, each stack within the plan's
    /// `deploy.pull-timeout`. No container changes.
    pub async fn pull_planned(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
    ) -> Result<(), MmError> {
        let (_, _, target) = self.selected_target(selection)?;
        validate_target(plan, &target)?;
        if plan.stacks.is_empty() {
            return Ok(());
        }
        let mut environment = self
            .prepare_environment(selection, plan, OperationId::new())
            .await?;
        let targets = plan
            .stacks
            .iter()
            .map(|stack| stack_runtime_target(plan, &target, stack, environment.bundle(), false))
            .collect::<Vec<_>>();
        let mut pulling = Vec::with_capacity(targets.len());
        for runtime_target in &targets {
            pulling.push(self.pull_images(plan, runtime_target, None));
        }
        let pulled = futures_util::stream::iter(pulling)
            .buffered(STACKS_AT_ONCE)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<()>, MmError>>();
        environment.dispose()?;
        pulled.map(drop)
    }
}
