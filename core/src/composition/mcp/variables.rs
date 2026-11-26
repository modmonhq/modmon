//! `mm_read` kinds `env` and `secrets`: what `mm env status`, `mm env
//! explain KEY` and `mm secret status` print, through the same services,
//! never with a secret's or a sensitive variable's value.

use super::*;

impl McpApplicationAdapter {
    pub(super) async fn read_variables(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        key: Option<&str>,
    ) -> Result<serde_json::Value, MmError> {
        context.require(AgentAction::Read, None)?;
        let selection = self.selection(context);
        let runtime = self.runtime();
        let manager = || {
            crate::composition::environment_services(
                &context.project_root,
                Some(&self.mm_home),
                runtime.clock(),
            )
            .map(|services| services.manager)
        };
        match kind {
            McpReadKind::Env => {
                let snapshot = manager()?.status(&selection, true).await?;
                let snapshot = match key {
                    Some(key) => snapshot.only(key).map_err(MmError::in_own_words)?,
                    None => snapshot,
                };
                json_value(snapshot.without_sensitive_values())
            }
            McpReadKind::Secrets => match self.remote(&selection, context)? {
                Some(environment) => {
                    let mut values = environment.host_values().await?;
                    if let Some(key) = key {
                        for node in &mut values.nodes {
                            node.values.retain(|value| value.key == key);
                        }
                    }
                    json_value(values)
                }
                None => {
                    let status = manager()?.secret_status(&selection, key).await?;
                    json_value(serde_json::json!({
                        "project_name": status.project_name,
                        "environment": status.environment,
                        "bindings": status.secrets,
                        "runtime_delivery": status.runtime_delivery,
                        "diagnostics": status.diagnostics,
                    }))
                }
            },
            _ => Err(MmError::State {
                message: "unreachable MCP variable read".to_owned(),
            }),
        }
    }
}
