//! What `operate` does on a host for `mmd`: restart a service's containers
//! and run the jobs the deployed configuration declares, through the same
//! services `mm` uses on this machine, as the node this host is.

use super::DaemonUseCaseAdapter;
use crate::app::ports::RemoteRequestContext;
use crate::composition::{
    RuntimeComposition, job_scheduler_service_on, runtime_operations_service_on,
};
use crate::domain::error::MmError;
use crate::domain::observability::{ApprovalContext, RestartOutcome, RestartRequest};
use crate::domain::scheduler::{JobRun, JobRunRequest, JobRunsRequest};

/// Where a request a host serves comes from, as its records say.
const RELAY_SOURCE: &str = "authenticated-ssh-relay";

impl DaemonUseCaseAdapter {
    pub(super) async fn restart_here(
        &self,
        scope: &crate::domain::remote::RemoteScope,
        root: &std::path::Path,
        request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        let selection = self.selection_at(scope, root)?;
        let mut operations = runtime_operations_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            scope.runtime,
            &RuntimeComposition::new(),
        )?;
        // A cluster node restarts without dropping a request.
        if let (Some(leaving), Some(names), Some(_)) =
            (self.node_leaving(), &self.names, &selection.node)
        {
            operations = operations.with_drained_restart(crate::app::DrainedRestart {
                leaving,
                names: std::sync::Arc::clone(names),
            });
        }
        operations.restart(&selection, request).await
    }

    pub(super) async fn run_job_here(
        &self,
        context: &RemoteRequestContext,
        request: JobRunRequest,
    ) -> Result<JobRun, MmError> {
        let selection = self.selection(context)?;
        job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &RuntimeComposition::new(),
        )?
        .run_job(&selection, &request, &approval(context))
        .await
    }

    pub(super) async fn cancel_job_runs_here(
        &self,
        context: &RemoteRequestContext,
        started: &[crate::domain::scheduler::StartedRuns],
    ) -> Result<Vec<JobRun>, MmError> {
        let selection = self.selection(context)?;
        job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &RuntimeComposition::new(),
        )?
        .cancel_started(&selection, started, &approval(context))
        .await
    }

    pub(super) fn running_job_runs_here(
        &self,
        context: &RemoteRequestContext,
    ) -> Result<Vec<crate::domain::rollout::OperationInFlight>, MmError> {
        let selection = self.selection(context)?;
        job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &RuntimeComposition::new(),
        )?
        .in_flight(&selection, &approval(context))
    }

    pub(super) async fn job_runs_here(
        &self,
        context: &RemoteRequestContext,
        request: JobRunsRequest,
    ) -> Result<Vec<JobRun>, MmError> {
        let selection = self.selection(context)?;
        job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            context.scope.runtime,
            &RuntimeComposition::new(),
        )?
        .runs(&selection, &request, &approval(context))
        .await
    }
}

/// The principal the relay authenticated, as the job records name it.
fn approval(context: &RemoteRequestContext) -> ApprovalContext {
    ApprovalContext {
        principal: context.principal.clone(),
        source: RELAY_SOURCE.to_owned(),
        approved_high_risk: false,
    }
}

impl DaemonUseCaseAdapter {
    /// Moves on the environment's runs that nobody waits for, and starts
    /// the runs its schedules are due for. Nothing is asked of the engine
    /// when there is nothing to do.
    pub(crate) async fn keep_jobs(
        &self,
        project: crate::domain::workspace::ProjectId,
        environment: &str,
        root: &std::path::Path,
        node: Option<&str>,
    ) -> Result<(), MmError> {
        let document = crate::adapters::config::ConfigLoader.load_from(root)?;
        let target = document
            .config
            .environments
            .get(environment)
            .and_then(|config| config.target.clone())
            .unwrap_or_else(|| document.config.target.clone());
        let runtime = document
            .config
            .environments
            .get(environment)
            .and_then(|config| config.runtime.as_ref())
            .and_then(|runtime| runtime.driver)
            .unwrap_or(crate::domain::runtime::RuntimeKind::DockerCompose);
        let scope = crate::domain::remote::RemoteScope {
            project_id: project,
            environment: environment.to_owned(),
            target,
            target_fingerprint: "0".repeat(64),
            runtime,
            node: node.map(str::to_owned),
        };
        let selection = self.selection_at(&scope, root)?;
        let approval = ApprovalContext {
            principal: "mmd".to_owned(),
            source: "job-keeper".to_owned(),
            approved_high_risk: false,
        };
        job_scheduler_service_on(
            selection.document_scope(),
            &selection.start,
            Some(&self.mm_home),
            runtime,
            &RuntimeComposition::new(),
        )?
        .keep(&selection, &approval)
        .await
    }
}
