//! `mm_read` kind `plans`, and the release a deployment on this machine
//! plans: this checkout's commit, with the name and inputs asked for.

use super::*;

pub(super) fn released(
    selection: &WorkspaceSelection,
    release: &crate::domain::release_facts::ReleaseRequest,
) -> WorkspaceSelection {
    selection.with_release(Some(crate::composition::local_release(
        &selection.start,
        release.clone(),
    )))
}

impl McpApplicationAdapter {
    pub(super) async fn read_plans(
        &self,
        context: &McpRequestContext,
        runtime: &RuntimeComposition,
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let deployment = runtime
            .deployment_service(selection, Some(&self.mm_home))?
            .plan(&released(selection, &Default::default()))
            .await?;
        let scheduler = job_scheduler_service(
            &context.project_root,
            Some(&self.mm_home),
            context.scope.runtime,
            runtime,
        )?
        .plan_reconciliation(selection, &self.approval())?;
        json_value(serde_json::json!({
            "deployment": deployment,
            "scheduler": scheduler,
        }))
    }
}
