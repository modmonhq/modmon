//! A stack a rollout step finds already running what the plan would start
//! is left alone (`domain::rollout::unchanged`).

use super::*;

impl DeploymentService {
    /// Whether `stack` on this cluster node runs exactly what applying it
    /// would start. A look that fails says no: the step then applies the
    /// stack as usual.
    pub(super) async fn unchanged_here(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        stack: &DeploymentStackPlan,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    ) -> bool {
        if plan.node.is_none() {
            return false;
        }
        let runtime_target = stack_runtime_target(plan, target, stack, environments, false);
        let looked = async {
            let counts = self.apply_counts(plan, &runtime_target).await?;
            self.ports
                .runtime
                .stack_unchanged(&runtime_target, &counts)
                .await
        };
        match looked.await {
            Ok(unchanged) => unchanged,
            Err(error) => {
                tracing::warn!(stack = %stack.stack, %error, "could not tell whether the stack changed; applying it");
                false
            }
        }
    }
}
