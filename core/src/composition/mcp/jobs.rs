//! `mm_read` kinds `jobs` and `schedules`: what the configuration
//! declares and what ran, from the hosts of a remote environment through
//! the same path as `mm job history`. A project that declares none reads as
//! empty lists without asking anyone. Kind `deploy_logs` reads what
//! `mm deploy logs` prints, through the same path.

use super::*;

impl McpApplicationAdapter {
    pub(super) async fn read_jobs(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        limit: usize,
        document: &ConfigDocument,
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let runtime = self.runtime();
        let gateway = crate::composition::job_gateway_as(
            selection,
            Some(&self.mm_home),
            context.scope.runtime,
            &runtime,
            crate::composition::RemoteIdentity::Agent,
        )?;
        let request = crate::domain::scheduler::JobRunsRequest {
            job: None,
            run: None,
            limit: u32::try_from(limit.min(self.config.maximum_page_size)).unwrap_or(u32::MAX),
        };
        let runs = if document.config.jobs.is_empty() {
            Vec::new()
        } else {
            gateway.runs(request, &self.approval()).await?
        };
        let local: Option<(&crate::app::JobSchedulerService, &WorkspaceSelection)> = match &gateway
        {
            crate::app::JobGateway::Local {
                scheduler,
                selection: local_selection,
            } => Some((scheduler.as_ref(), local_selection.as_ref())),
            crate::app::JobGateway::Remote { .. } => None,
        };
        if kind == McpReadKind::Jobs {
            let reconciled = match local {
                Some((scheduler, local_selection)) => serde_json::to_value(
                    scheduler.list_jobs(local_selection, &self.approval())?.jobs,
                )
                .map_err(state_json)?,
                // A remote host takes its definitions from the deployed
                // commit; what it runs is in `recent_runs`.
                None => serde_json::Value::Null,
            };
            return json_value(serde_json::json!({
                "configured": &document.config.jobs,
                "reconciled_jobs": reconciled,
                "recent_runs": runs,
            }));
        }
        let reconciled = match local {
            Some((scheduler, local_selection)) if !document.config.schedules.is_empty() => {
                scheduler.list_schedules(local_selection, &self.approval())?
            }
            _ => Vec::new(),
        };
        json_value(serde_json::json!({
            "configured": &document.config.schedules,
            "reconciled": reconciled,
            "recent_runs": runs.into_iter().filter(|run| run.schedule.is_some()).collect::<Vec<_>>(),
        }))
    }

    /// `mm_read` kind `deploy_logs`: what `mm deploy logs` prints, a page of
    /// the runs whose output shares the MCP log bound, each keeping its end.
    pub(super) async fn read_deploy_logs(
        &self,
        context: &McpRequestContext,
        (cursor, limit): (Option<&str>, usize),
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let runtime = self.runtime();
        let runs = crate::composition::job_gateway_as(
            selection,
            Some(&self.mm_home),
            context.scope.runtime,
            &runtime,
            crate::composition::RemoteIdentity::Agent,
        )?
        .deployment_runs(&self.approval())
        .await?;
        let offset = decode_cursor(cursor)?;
        let limit = limit.min(self.config.maximum_page_size);
        let shown = runs
            .len()
            .min(offset.saturating_add(limit))
            .saturating_sub(offset);
        let share = MCP_MAX_LOG_BYTES / shown.max(1);
        let items = runs
            .into_iter()
            .map(|mut run| {
                run.keep_output_end(share);
                serde_json::to_value(run).map_err(state_json)
            })
            .collect::<Result<Vec<_>, _>>()?;
        page_value(items, offset, limit)
    }

    /// Schedules of a remote environment change with its deployment.
    pub(super) fn refuse_remote_schedules(
        &self,
        context: &McpRequestContext,
        action: &McpJobAction,
    ) -> Result<(), MmError> {
        let schedules = matches!(
            action,
            McpJobAction::Reconcile
                | McpJobAction::RunSchedule { .. }
                | McpJobAction::SetScheduleEnabled { .. }
        );
        if schedules && self.remote(&self.selection(context), context)?.is_some() {
            return Err(crate::domain::scheduler::remote_schedules(
                &context.scope.environment,
            ));
        }
        Ok(())
    }
}
