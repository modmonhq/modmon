//! `mm_read` kind `plans`.

use super::*;

impl McpApplicationAdapter {
    pub(super) async fn read_plans(
        &self,
        context: &McpRequestContext,
        runtime: &RuntimeComposition,
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let deployment = runtime
            .deployment_service(selection, Some(&self.mm_home))?
            .plan(&crate::composition::released(
                selection,
                Some(Default::default()),
            ))
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
