//! Starting the container of a claimed run. A container the engine could
//! not start ends the run's attempt as failed, with why in its output.

use super::*;

impl JobSchedulerService {
    /// Starts the claimed run's container and records it as running.
    pub(super) async fn start(
        &self,
        document: &ConfigDocument,
        selection: &WorkspaceSelection,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
        output_limit: usize,
    ) -> Result<(), MmError> {
        match self
            .launch_claim(document, selection, target, claim, output_limit)
            .await
        {
            Ok(runtime_id) => {
                self.repository
                    .mark_run_running(claim, &runtime_id, self.clock.now_unix_ms())?;
                Ok(())
            }
            Err(error @ (MmError::Launch { .. } | MmError::ProcessFailed { .. })) => {
                self.not_started(target, claim, error).await
            }
            Err(error) => Err(error),
        }
    }

    /// Ends the attempt of a run whose container the engine could not
    /// start, with the error's code and its message as the output. The
    /// error is the caller's when no retry follows.
    async fn not_started(
        &self,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
        error: MmError,
    ) -> Result<(), MmError> {
        self.fail_or_retry(
            target,
            claim,
            error.code(),
            JobRunState::Failed,
            (None, error.agent_message(), false),
        )
        .await?;
        if claim.run.attempt < claim.run.maximum_attempts {
            Ok(())
        } else {
            Err(error)
        }
    }

    async fn launch_claim(
        &self,
        document: &ConfigDocument,
        selection: &WorkspaceSelection,
        target: &crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
        output_limit: usize,
    ) -> Result<String, MmError> {
        let prepared = match &self.launch_targets {
            Some(targets) => {
                targets
                    .prepare(selection, &target.stack_name, claim.run.id)
                    .await?
            }
            None => None,
        };
        let (target, release) = match prepared {
            Some(prepared) => (prepared.target, Some(prepared.release)),
            None => (target.clone(), None),
        };
        let launched = self.launch_on(document, target, claim, output_limit).await;
        // The container exists or never will: what was staged for it goes.
        let released = release.map_or(Ok(()), |release| release());
        let runtime_id = launched?;
        released?;
        Ok(runtime_id)
    }

    async fn launch_on(
        &self,
        document: &ConfigDocument,
        target: crate::domain::runtime::RuntimeTarget,
        claim: &ClaimedJobRun,
        output_limit: usize,
    ) -> Result<String, MmError> {
        let environment = BTreeMap::from([
            ("MM_JOB_RUN_ID".to_owned(), claim.run.id.to_string()),
            ("MM_JOB_ATTEMPT".to_owned(), claim.run.attempt.to_string()),
            (
                "MM_JOB_FENCE_TOKEN".to_owned(),
                claim.run.fence_token.to_string(),
            ),
        ]);
        self.runtime
            .launch(JobLaunch {
                run_id: claim.run.id,
                fence_token: claim.run.fence_token,
                runtime_name: claim.run.runtime_name.clone(),
                runtime: self.application.runtime_kind(),
                target,
                job: claim.run.job.clone(),
                module: claim.job_definition.module.clone(),
                service: claim.job_definition.service.clone(),
                argv: claim.job_definition.command.clone(),
                user: claim.job_definition.user.clone(),
                timeout: effective_timeout(claim)?,
                output_limit_bytes: output_limit,
                environment: crate::domain::observability::environment_values(environment),
                redactions: self.redactions(document, &claim.run.environment).await?,
            })
            .await
    }
}
