//! Several stacks of a rollout on a node in one step, side by side
//! (`domain::rollout::waves`).

use futures_util::StreamExt as _;

use super::rollout::{StackOutcome, StackStep};
use super::*;

impl DeploymentService {
    /// Rolls out each of `stacks` of the rollout `plan_hash` began: new
    /// containers next to the old ones, then the stack, up to
    /// `STACKS_AT_ONCE` stacks at a time under one hold of the target and
    /// one prepared environment. A stack that fails says so in its outcome;
    /// the others go on.
    pub async fn roll_stacks(
        &self,
        selection: &WorkspaceSelection,
        plan_hash: &str,
        stacks: &[String],
    ) -> Result<Vec<(String, StackOutcome)>, MmError> {
        let running = self.running_rollout(selection, plan_hash)?.ok_or_else(|| {
            MmError::State {
                message: format!(
                    "this host has no rollout of plan {plan_hash} in progress; run `mm deploy plan` again"
                ),
            }
        })?;
        let mut environment = self
            .prepare_rollout_environment(selection, &running.plan, running.operation_id)
            .await?;
        let bundle = environment.bundle();
        let mut rolling = Vec::with_capacity(stacks.len());
        for stack in stacks {
            rolling.push(self.roll_whole(&running, bundle, stack));
        }
        let outcomes = futures_util::stream::iter(rolling)
            .buffered(crate::domain::rollout::STACKS_AT_ONCE)
            .collect::<Vec<_>>()
            .await;
        environment.dispose()?;
        Ok(stacks.iter().cloned().zip(outcomes).collect())
    }

    /// The start of a stack's `start-first` services, then the stack. An
    /// error is the stack's failure.
    async fn roll_whole(
        &self,
        running: &super::rollout::RunningRollout,
        bundle: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
        stack: &str,
    ) -> StackOutcome {
        let failed = |error: MmError| {
            StackOutcome::Failed(crate::domain::rollout::StepFailure {
                stack: stack.to_owned(),
                service: None,
                reason: error.to_string(),
                log: Vec::new(),
                code: Some(error.code().to_owned()),
            })
        };
        let start_unchanged = match self
            .roll_with(running, bundle, stack, StackStep::Start)
            .await
        {
            Ok(StackOutcome::Done { unchanged, .. }) => unchanged,
            Ok(failed @ StackOutcome::Failed(_)) => return failed,
            Err(error) => return failed(error),
        };
        match self
            .roll_with(running, bundle, stack, StackStep::Apply)
            .await
        {
            Ok(StackOutcome::Done {
                started: _,
                unchanged,
                addresses,
            }) => StackOutcome::Done {
                started: Vec::new(),
                unchanged: unchanged && start_unchanged,
                addresses,
            },
            Ok(failed) => failed,
            Err(error) => failed(error),
        }
    }
}
