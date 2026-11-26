//! Jobs a deployment runs: a job runs in a fresh container with what the
//! deployment gives its service (the deployment's own rendered files,
//! environment, and secrets), before the service is started or replaced.
//! A job that fails stops the deployment there. And what a run that was
//! cut off left behind is cleaned up when the next deployment begins.

use super::*;
use crate::domain::rollout::DeployJob;
use crate::domain::scheduler::JobRun;

impl DeploymentService {
    /// The step of a rollout that runs job `job` of `stack` on this host.
    /// Its run is filed under `label` (`domain::scheduler::deployment_label`).
    pub async fn roll_job(
        &self,
        selection: &WorkspaceSelection,
        (plan_hash, label): (&str, &str),
        stack: &str,
        job: &str,
    ) -> Result<JobRun, MmError> {
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
        let ran = self
            .run_deploy_job(
                selection,
                (&running.plan, &running.target, label),
                stack,
                job,
                environment.bundle(),
            )
            .await;
        environment.dispose()?;
        ran
    }

    /// Before a stack is applied in one call: the plan's jobs of the stack,
    /// in order. A job that does not succeed is the failure of the stack.
    pub(super) async fn run_stack_jobs(
        &self,
        selection: &WorkspaceSelection,
        (plan, target): (&DeploymentPlan, &NamedTarget),
        stack: &str,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    ) -> Result<Option<crate::domain::rollout::StepFailure>, MmError> {
        let jobs = plan
            .rollout
            .jobs
            .iter()
            .filter(|job| {
                job.stack == stack && job.run == crate::domain::config::JobWhen::BeforeDeploy
            })
            .collect::<Vec<&DeployJob>>();
        if jobs.is_empty() {
            return Ok(None);
        }
        if let Some(stack_plan) = plan
            .stacks
            .iter()
            .find(|candidate| candidate.stack == stack)
            && let Err(failure) = self
                .apply_ahead(plan, target, stack_plan, environments)
                .await?
        {
            return Ok(Some(failure));
        }
        for job in jobs {
            let run = self
                .run_deploy_job(
                    selection,
                    (plan, target, &plan.plan_hash),
                    stack,
                    &job.job,
                    environments,
                )
                .await?;
            if run.state != crate::domain::scheduler::JobRunState::Succeeded {
                return Ok(Some(crate::domain::rollout::job_failure(
                    stack,
                    &run,
                    plan.rollout.failure_log_lines(),
                )));
            }
        }
        Ok(None)
    }

    async fn run_deploy_job(
        &self,
        selection: &WorkspaceSelection,
        (plan, target, label): (&DeploymentPlan, &NamedTarget, &str),
        stack: &str,
        job: &str,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    ) -> Result<JobRun, MmError> {
        let jobs = self.jobs.as_ref().ok_or_else(|| MmError::State {
            message: format!("this host cannot run job '{job}' in a deployment"),
        })?;
        let stack_plan = plan
            .stacks
            .iter()
            .find(|candidate| candidate.stack == stack)
            .ok_or_else(|| MmError::State {
                message: format!(
                    "plan {} has no stack '{stack}' on this host",
                    plan.plan_hash
                ),
            })?;
        let runtime_target = stack_runtime_target(plan, target, stack_plan, environments, false);
        jobs.run(selection, job, runtime_target, label).await
    }

    /// Starts, or replaces, the services of a stack that its
    /// `before-deploy` jobs' services depend on, and waits until they are
    /// healthy. Returns them.
    pub(super) async fn apply_ahead(
        &self,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        stack: &DeploymentStackPlan,
        environments: Option<&crate::domain::environment::PreparedEnvironmentBundle>,
    ) -> Result<Result<Vec<String>, crate::domain::rollout::StepFailure>, MmError> {
        let services = plan
            .rollout
            .ahead
            .iter()
            .filter(|placed| placed.stack == stack.stack)
            .map(|placed| placed.service.clone())
            .collect::<Vec<_>>();
        if services.is_empty() {
            return Ok(Ok(services));
        }
        let runtime_target = stack_runtime_target(plan, target, stack, environments, false);
        self.pull_images(plan, &runtime_target, Some(&services))
            .await?;
        // What the step replaces leaves first, as in a stack's apply.
        let others = stack
            .artifact
            .manifest
            .services
            .iter()
            .map(|service| service.service.clone())
            .filter(|service| !services.contains(service))
            .collect::<Vec<_>>();
        self.retire_outdated(plan, &runtime_target, stack, &others)
            .await?;
        // A scaled service keeps its count, as in a stack's apply; one the
        // node runs no container of has nothing to wait for.
        self.await_scaled_leaves(plan, &stack.stack).await;
        let counts = self.apply_counts(plan, &runtime_target).await?;
        self.ports
            .runtime
            .apply_services(&runtime_target, &services, &counts)
            .await?;
        let idle = self.idle_services(plan, &stack.stack);
        let awaited = services
            .iter()
            .filter(|service| !idle.contains(service))
            .cloned()
            .collect::<Vec<_>>();
        if awaited.is_empty() {
            return Ok(Ok(services));
        }
        Ok(
            match self
                .await_health(plan, &runtime_target, Some(&awaited))
                .await?
            {
                Some(failure) => Err(failure),
                None => Ok(services),
            },
        )
    }

    /// Removes what runs that were cut off left behind: job containers of
    /// this environment no run is waiting for any more.
    pub(super) async fn sweep_jobs(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
        target: &NamedTarget,
    ) -> Result<(), MmError> {
        let Some(jobs) = &self.jobs else {
            return Ok(());
        };
        let projects = plan
            .stacks
            .iter()
            .map(|stack| stack_runtime_target(plan, target, stack, None, false))
            .collect::<Vec<_>>();
        for removed in jobs.sweep(selection, &projects).await? {
            tracing::info!(container = %removed, "removed a job container a cut-off run left");
        }
        Ok(())
    }
}
