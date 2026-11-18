//! Where a job's container is created outside a deployment: with what the
//! environment's last deployment gave the job's service. The Compose files
//! are the ones that deployment rendered (the same compile, with every
//! overlay mm adds to a service), and the environment and secrets are
//! staged for the run as a deployment stages them, then removed once the
//! container exists.

use async_trait::async_trait;

use super::*;
use crate::app::ports::{JobLaunchTargets, PreparedJobTarget};

impl DeploymentService {
    /// The files, environment, and secrets the last deployment gave stack
    /// `stack`, staged for run `run`. `None` when this host has no
    /// finished deployment of the environment.
    pub async fn job_target(
        &self,
        selection: &WorkspaceSelection,
        stack: &str,
        run: uuid::Uuid,
    ) -> Result<Option<PreparedJobTarget>, MmError> {
        let (document, environment, target) = self.selected_target(selection)?;
        let Some(plan) = self.ports.state.latest_successful_deployment_for_owner(
            document.config.project.id,
            &environment,
            target.definition.id,
        )?
        else {
            return Ok(None);
        };
        let stack_plan = plan
            .stacks
            .iter()
            .find(|candidate| candidate.stack == stack)
            .ok_or_else(|| MmError::Environment {
                message: format!(
                    "the last deployment of '{environment}' here does not run stack '{stack}', so a job of it has nothing to start from. Deploy it first: `mm deploy plan --environment {environment}`"
                ),
            })?;
        let review = self.ports.environments.review(selection).await?;
        if review.digest() != plan.environment_review.digest() {
            return Err(MmError::State {
                message: format!(
                    "the values or secrets of '{environment}' changed since its last deployment, and a job runs with what its service runs. Deploy the change first: `mm deploy plan --environment {environment}`"
                ),
            });
        }
        let guard = self
            .prepare_environment(selection, &plan, OperationId(run))
            .await?;
        let runtime_target =
            stack_runtime_target(&plan, &target, stack_plan, guard.bundle(), false);
        Ok(Some(PreparedJobTarget {
            target: runtime_target,
            release: Box::new(move || {
                let mut guard = guard;
                guard.dispose()
            }),
        }))
    }

    /// The selection's project, environment, and target, as a deployment
    /// of it resolves them.
    pub(super) fn selected_target(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<(crate::domain::config::ConfigDocument, String, NamedTarget), MmError> {
        let document = self.ports.documents.load_from(&selection.start)?;
        let environment = crate::domain::config::resolve_environment(
            &document,
            selection.environment.as_deref(),
        )?;
        let (environment_pref, project_pref) = crate::app::load_selection_preferences(
            Some(self.ports.state.as_ref()),
            &document,
            &environment,
        )?;
        let (_, target) = crate::app::resolve_selection(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            self.ports.targets.as_ref(),
        )?;
        Ok((document, environment, target))
    }
}

#[async_trait]
impl JobLaunchTargets for DeploymentService {
    async fn prepare(
        &self,
        selection: &WorkspaceSelection,
        stack: &str,
        run: uuid::Uuid,
    ) -> Result<Option<PreparedJobTarget>, MmError> {
        self.job_target(selection, stack, run).await
    }
}
