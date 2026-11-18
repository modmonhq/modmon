//! `mm_read` of a cluster's nodes, mesh and deployments, and of what runs.

use super::*;

impl McpApplicationAdapter {
    /// What `mm target nodes` and `mm target mesh status` print, through
    /// the same reads.
    pub(super) async fn read_cluster(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let Some(environment) = self.remote(selection, context)? else {
            return Err(MmError::Environment {
                message: format!(
                    "environment '{}' runs on this machine; nodes, their mesh, and their deployment records belong to a remote target",
                    context.scope.environment
                ),
            }
            .in_own_words());
        };
        match kind {
            McpReadKind::Deployments => json_value(environment.deployment_report().await),
            McpReadKind::Mesh if environment.is_cluster() => {
                json_value(environment.mesh_report().await)
            }
            McpReadKind::Mesh => Err(MmError::Environment {
                message: format!(
                    "target '{}' names one host, so it has no mesh; a mesh connects the hosts a target lists under nodes:",
                    environment.target()
                ),
            }
            .in_own_words()),
            _ => {
                let (nodes, addresses) =
                    tokio::join!(environment.node_reports(), environment.address_table());
                json_value(serde_json::json!({
                    "target": environment.target(),
                    "nodes": nodes,
                    "addresses": addresses,
                }))
            }
        }
    }

    pub(super) async fn read_runtime_snapshot(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        (cursor, limit): (Option<&str>, usize),
        runtime: &RuntimeComposition,
        selection: &WorkspaceSelection,
    ) -> Result<serde_json::Value, MmError> {
        let snapshot = self
            .gateway(selection, context, runtime)?
            .status(&self.approval())
            .await?;
        match kind {
            McpReadKind::Services => json_value(snapshot.stacks),
            McpReadKind::Routes => json_value(snapshot.ingress),
            McpReadKind::Operations => {
                let offset = decode_cursor(cursor)?;
                page_value(
                    snapshot
                        .recent_operations
                        .into_iter()
                        .map(|value| serde_json::to_value(value).map_err(state_json))
                        .collect::<Result<Vec<_>, _>>()?,
                    offset,
                    limit.min(self.config.maximum_page_size),
                )
            }
            McpReadKind::Health => json_value(serde_json::json!({
                "runtime": snapshot.runtime, "validation": snapshot.validation,
            })),
            McpReadKind::Names => json_value(snapshot.names.unwrap_or_default()),
            _ => Err(MmError::State {
                message: "unreachable MCP read category".to_owned(),
            }),
        }
    }
}
