//! Planning a return to what the hosts ran before.

use super::RemoteEnvironment;
use crate::domain::error::MmError;
use crate::domain::remote::ClusterDeploymentPlan;
use crate::domain::rollout::{RollbackTarget, redeploy_target, rollback_target};

impl RemoteEnvironment {
    /// Plans a return to what the hosts ran before: before a rollout that
    /// was left unfinished, or else before the last finished deployment.
    /// The hosts plan the earlier commit from their own copy of the
    /// repository, with the build that deployment used; the result is an
    /// ordinary plan, applied with its hash like any other.
    pub async fn plan_rollback(
        &self,
        only: Option<&[String]>,
    ) -> Result<(RollbackTarget, ClusterDeploymentPlan), MmError> {
        self.plan_rollback_reporting(only, &crate::app::ports::NoProgress)
            .await
    }

    /// As [`Self::plan_rollback`], saying what each node is doing.
    pub async fn plan_rollback_reporting(
        &self,
        only: Option<&[String]>,
        progress: &dyn crate::app::ports::ProgressReporter,
    ) -> Result<(RollbackTarget, ClusterDeploymentPlan), MmError> {
        if let Some(only) = only {
            self.select(Some(only))?;
        }
        let status = crate::app::ports::progress_step(
            progress,
            ("history", "Reading what every node ran before"),
            async { Ok::<_, MmError>(self.deployment_status().await) },
            |_| Some("Read what every node ran before".to_owned()),
        )
        .await?;
        let target =
            rollback_target(&status, only).map_err(|message| MmError::Environment { message })?;
        let nodes = (self.is_cluster() && !target.every_node).then_some(target.nodes.as_slice());
        let plan = self
            .plan_deployment_reporting(
                (&target.commit, target.receipt.as_ref(), &target.release),
                nodes,
                progress,
            )
            .await?;
        Ok((target, plan))
    }

    /// Plans the commit and build the hosts run now again, so that what was
    /// staged on them goes live. An ordinary reviewed plan: rolling and
    /// drained like any other.
    pub async fn plan_redeploy_reporting(
        &self,
        only: Option<&[String]>,
        progress: &dyn crate::app::ports::ProgressReporter,
    ) -> Result<(RollbackTarget, ClusterDeploymentPlan), MmError> {
        if let Some(only) = only {
            self.select(Some(only))?;
        }
        let status = crate::app::ports::progress_step(
            progress,
            ("history", "Reading what every node runs"),
            async { Ok::<_, MmError>(self.deployment_status().await) },
            |_| Some("Read what every node runs".to_owned()),
        )
        .await?;
        let target =
            redeploy_target(&status, only).map_err(|message| MmError::Environment { message })?;
        let nodes = (self.is_cluster() && !target.every_node).then_some(target.nodes.as_slice());
        let plan = self
            .plan_deployment_reporting(
                (&target.commit, target.receipt.as_ref(), &target.release),
                nodes,
                progress,
            )
            .await?;
        Ok((target, plan))
    }
}
