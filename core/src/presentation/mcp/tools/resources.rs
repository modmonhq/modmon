use super::*;

#[tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        info.server_info = Implementation::new("mm-mcp", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(SERVER_INSTRUCTIONS.to_owned());
        info
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        if request.and_then(|value| value.cursor).is_some() {
            return Err(rmcp::ErrorData::invalid_params(
                "resource cursor is invalid",
                None,
            ));
        }
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new("mm://context", "mm-context")
                .with_description("Server version, transport, and bounds"),
            Resource::new("mm://access", "mm-agent-access")
                .with_description("What you may do in each environment"),
            Resource::new("mm://schema/project", "mm-project-schema")
                .with_description("Versioned JSON Schema for mm.yaml"),
            Resource::new("mm://schema/tools", "mm-tool-schemas")
                .with_description("Versioned MCP tool contract snapshot"),
            Resource::new("mm://docs/workflow", "mm-agent-workflow")
                .with_description("Safe inspect, review, and apply workflow"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, rmcp::ErrorData> {
        let mime_type = if request.uri == "mm://docs/workflow" {
            "text/markdown"
        } else {
            "application/json"
        };
        let value = match request.uri.as_str() {
            "mm://context" => serde_json::to_string_pretty(&serde_json::json!({
                "tool_schema_version": MCP_TOOL_SCHEMA_VERSION,
                "resource_schema_version": MCP_RESOURCE_SCHEMA_VERSION,
                "mm_version": env!("CARGO_PKG_VERSION"),
                "project_schema_version": MCP_PROJECT_SCHEMA_VERSION,
                "transport": "stdio",
                "principal": self.config.principal,
                "maximum_page_size": self.config.maximum_page_size,
                "maximum_response_bytes": self.config.maximum_response_bytes,
                "request_timeout_ms": self.config.request_timeout_ms,
                "access": "mm://access or the mm_access tool",
                "shell": false,
                "model_content_authority": false,
            })),
            "mm://access" => {
                let overview = self.use_cases.access().await.map_err(|error| {
                    rmcp::ErrorData::internal_error(safe_error_message(&error).0, None)
                })?;
                serde_json::to_string_pretty(&overview)
            }
            "mm://schema/project" => serde_json::to_string_pretty(&schemars::schema_for!(
                crate::domain::config::MmConfig
            )),
            "mm://schema/tools" => serde_json::to_string_pretty(&self.schema_snapshot()),
            "mm://docs/workflow" => Ok(WORKFLOW_DOC.to_owned()),
            _ => {
                return Err(rmcp::ErrorData::resource_not_found(
                    "resource not found",
                    Some(serde_json::json!({ "uri": request.uri })),
                ));
            }
        }
        .map_err(|_| rmcp::ErrorData::internal_error("resource serialization failed", None))?;
        if value.len() > self.config.maximum_response_bytes {
            return Err(rmcp::ErrorData::invalid_params(
                "resource exceeds configured response bound",
                None,
            ));
        }
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(value, request.uri).with_mime_type(mime_type),
        ])
        .into())
    }
}
