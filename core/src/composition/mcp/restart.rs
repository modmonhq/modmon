//! `mm_service_restart_plan` and `_apply`: `mm service restart` for an
//! agent, through the same gateway, as the agent on a remote host.

use super::*;
use crate::domain::mcp::McpServiceRestartPlan;
use crate::domain::observability::{RestartOutcome, RestartRequest};

impl McpApplicationAdapter {
    async fn restart_plan(
        &self,
        context: &McpRequestContext,
        gateway: &crate::app::ServiceGateway,
        service: &str,
    ) -> Result<McpServiceRestartPlan, MmError> {
        let snapshot = gateway.status(&self.approval()).await?;
        McpServiceRestartPlan::of(&context.scope, context.remote, &snapshot, service)
    }

    pub(super) async fn plan_restart(
        &self,
        context: &McpRequestContext,
        service: &str,
    ) -> Result<McpServiceRestartPlan, MmError> {
        context.require(AgentAction::Plan, None)?;
        let runtime = self.runtime();
        let gateway = self.gateway(&self.selection(context), context, &runtime)?;
        self.restart_plan(context, &gateway, service).await
    }

    pub(super) async fn apply_restart(
        &self,
        context: &McpRequestContext,
        service: &str,
        plan_hash: &str,
    ) -> Result<RestartOutcome, MmError> {
        context.require(AgentAction::ServiceRestart, None)?;
        let runtime = self.runtime();
        let gateway = self.gateway(&self.selection(context), context, &runtime)?;
        if self
            .restart_plan(context, &gateway, service)
            .await?
            .plan_hash
            != plan_hash
        {
            return Err(MmError::Security {
                message: format!(
                    "service '{service}' changed since the restart was reviewed; review the restart again with mm_service_restart_plan"
                ),
            }
            .in_own_words());
        }
        gateway
            .restart(RestartRequest {
                selector: ServiceSelector {
                    stack: None,
                    module: None,
                    service: service.to_owned(),
                    container: None,
                },
                approval: self.approval(),
            })
            .await
    }
}
