//! `mm_dev_unit` and `mm_read` of `optional`: the service `mm dev enable`,
//! `mm dev disable` and the terminal UI use, as the agent.

use super::*;

impl McpApplicationAdapter {
    pub(super) fn development_unit_as_agent(
        &self,
        context: &McpRequestContext,
        request: crate::domain::mcp::McpDevelopmentUnitRequest,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::DevelopmentUnit, None)?;
        let document = self.document(context)?;
        let development = document.config.project.development_environment.as_deref();
        if development != Some(context.scope.environment.as_str()) {
            return Err(MmError::Environment {
                message: match development {
                    Some(development) => format!(
                        "optional units turn on and off in the development environment '{development}'; '{}' runs every unit it lists",
                        context.scope.environment
                    ),
                    None => "this project names no development environment, where optional units apply".to_owned(),
                },
            }
            .in_own_words());
        }
        json_value(crate::composition::development_units().set(
            &context.project_root,
            &[request.unit],
            request.on,
        )?)
    }

    pub(super) fn read_optional(
        &self,
        context: &McpRequestContext,
    ) -> Result<serde_json::Value, MmError> {
        json_value(crate::composition::development_units().list(&context.project_root)?)
    }
}
