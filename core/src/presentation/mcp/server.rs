use super::*;

impl McpServer {
    /// Authorizes one tool call. A refusal is already a complete, audited
    /// tool result: a handoff to the user, or an error.
    pub(super) async fn authorize(
        &self,
        (operation, mutation): (&str, bool),
        scope: &McpScope,
        (action, service): (AgentAction, Option<&str>),
        user_commands: impl FnOnce() -> UserCommands,
    ) -> Result<McpRequestContext, CallToolResult> {
        let request = McpAuthorizationRequest {
            scope: scope.clone(),
            action,
            service: service.map(str::to_owned),
        };
        let outcome = tokio::time::timeout(self.timeout(), self.use_cases.authorize(request))
            .await
            .unwrap_or_else(|_| Err(McpAuthorizationError::Failed(timeout_error())));
        let (request_id, project_root, code, result) = match outcome {
            Ok(context) => return Ok(context),
            Err(McpAuthorizationError::Denied(denial)) => (
                denial.request_id,
                denial.project_root.clone(),
                "MM_AGENT_HANDOFF".to_owned(),
                handoff_result(&denial, user_commands()),
            ),
            Err(McpAuthorizationError::Failed(error)) => {
                let request_id = uuid::Uuid::new_v4();
                let (message, next) = safe_error_message(&error);
                (
                    request_id,
                    std::path::PathBuf::new(),
                    error.code().to_owned(),
                    structured_error(error.code(), &message, &next, "not_started", request_id),
                )
            }
        };
        let audit = McpAuditRecord {
            id: uuid::Uuid::new_v4(),
            request_id,
            principal: self.config.principal.clone(),
            source: MCP_SOURCE.to_owned(),
            project_root,
            environment: scope.environment.chars().take(255).collect(),
            target: String::new(),
            operation: operation.to_owned(),
            mutation,
            plan_hash: None,
            outcome_code: code,
            occurred_at_unix_ms: chrono::Utc::now().timestamp_millis(),
        };
        if self.audit.append_mcp_audit(&audit).is_err() {
            return Err(audit_unavailable(mutation, request_id));
        }
        Err(result)
    }

    pub(super) async fn bounded<T: serde::Serialize>(
        &self,
        operation: impl std::future::Future<Output = Result<T, MmError>>,
    ) -> Result<serde_json::Value, MmError> {
        match tokio::time::timeout(self.timeout(), operation).await {
            Ok(result) => result.and_then(to_json),
            Err(_) => Err(timeout_error()),
        }
    }

    /// `bounded`, by the bound `McpServerConfig::call_bound` gives the
    /// call named `name`: none for applying a deployment.
    pub(super) async fn bounded_as<T: serde::Serialize>(
        &self,
        name: &str,
        operation: impl std::future::Future<Output = Result<T, MmError>>,
    ) -> Result<serde_json::Value, MmError> {
        match self.config.call_bound(name) {
            Some(_) => self.bounded(operation).await,
            None => operation.await.and_then(to_json),
        }
    }

    /// An error for a call that never selected an environment.
    pub(super) fn plain_error(&self, error: &MmError, request_id: uuid::Uuid) -> CallToolResult {
        let (message, next) = safe_error_message(error);
        structured_error(error.code(), &message, &next, "not_started", request_id)
    }

    /// Hands an edit of the users file to the user, whatever the grant.
    pub(super) fn refuse_access_edit(
        &self,
        context: &McpRequestContext,
        operation: &str,
        mutation: &crate::domain::onboarding::ConfigMutationRequest,
    ) -> Option<CallToolResult> {
        if mutation.resource != crate::domain::onboarding::ConfigResourceKind::User {
            return None;
        }
        let denial = crate::domain::mcp::McpDenial {
            request_id: context.request_id,
            reason: crate::domain::mcp::McpDenialReason::AccessEdit,
            project_root: context.project_root.clone(),
            environment: context.scope.environment.clone(),
            action: AgentAction::ConfigApply,
            service: None,
            access: context.access.clone(),
        };
        let audit = self.audit_record(context, operation, false, None, "MM_AGENT_HANDOFF");
        if self.audit.append_mcp_audit(&audit).is_err() {
            return Some(audit_unavailable(false, context.request_id));
        }
        Some(handoff_result(
            &denial,
            UserCommands::of(["mm team join".to_owned(), "mm team add NAME --key PUBLIC_KEY --access ENV=LEVEL".to_owned()])
                .with_note("The user edits the users file with `mm team` commands and has the change reviewed."),
        ))
    }

    pub(super) fn timeout(&self) -> Duration {
        Duration::from_millis(self.config.request_timeout_ms)
    }

    /// Wraps a plan for review. When the agent's grant covers applying it,
    /// the result carries a single-use token for the apply tool; otherwise
    /// it carries the commands the user applies it with.
    pub(super) fn review(
        &self,
        context: &McpRequestContext,
        (kind, apply): (ReviewKind, AgentAction),
        (plan_hash, request_hash): (String, String),
        plan: impl serde::Serialize,
        user_commands: UserCommands,
    ) -> Result<serde_json::Value, MmError> {
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at_unix_ms = now.saturating_add(MCP_PLAN_TTL_MS);
        if !context.access.permits(apply, None) {
            return to_json(McpReviewedPlan {
                schema_version: MCP_TOOL_SCHEMA_VERSION.to_owned(),
                plan_hash,
                applied_by: crate::domain::mcp::McpPlanApplier::User,
                approval_token: None,
                expires_at_unix_ms,
                next_action: format!(
                    "Agents have '{}' in environment '{}' and may not apply this plan ({} is needed). Show the plan to the user and ask them to apply it in their own terminal. {}",
                    context.access.summary(),
                    context.scope.environment,
                    apply.requirement(),
                    user_commands.note.unwrap_or_default(),
                ),
                user_commands: user_commands.commands,
                plan: to_json(plan)?,
            });
        }
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random).map_err(|error| MmError::State {
            message: format!("generating MCP review token: {error}"),
        })?;
        let token = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut reviews = self.reviews.lock().map_err(|_| MmError::State {
            message: "MCP review store lock was poisoned".to_owned(),
        })?;
        reviews.retain(|_, binding| binding.expires_at_unix_ms > now);
        if reviews.len() >= MAX_ACTIVE_REVIEWS {
            return Err(MmError::State {
                message: "MCP active review limit reached; wait for old reviews to expire"
                    .to_owned(),
            });
        }
        reviews.insert(
            token.clone(),
            ReviewBinding {
                kind,
                plan_hash: plan_hash.clone(),
                request_hash,
                expires_at_unix_ms,
            },
        );
        to_json(McpReviewedPlan {
            schema_version: MCP_TOOL_SCHEMA_VERSION.to_owned(),
            plan_hash: plan_hash.clone(),
            applied_by: crate::domain::mcp::McpPlanApplier::Agent,
            approval_token: Some(token),
            expires_at_unix_ms,
            next_action: format!(
                "Show the plan to the user. To apply it, call the matching apply tool with this token and confirmation 'apply:{plan_hash}'."
            ),
            user_commands: Vec::new(),
            plan: to_json(plan)?,
        })
    }

    pub(super) fn consume_review(
        &self,
        kind: ReviewKind,
        plan_hash: &str,
        token: &str,
        confirmation: &str,
        request_hash: &str,
    ) -> Result<(), MmError> {
        if confirmation != format!("apply:{plan_hash}") {
            return Err(MmError::Security {
                message: "MCP mutation requires the exact apply:<plan_hash> confirmation"
                    .to_owned(),
            });
        }
        let mut reviews = self.reviews.lock().map_err(|_| MmError::State {
            message: "MCP review store lock was poisoned".to_owned(),
        })?;
        let binding = reviews.remove(token).ok_or_else(|| MmError::Security {
            message: "MCP review token is absent, expired, or already consumed".to_owned(),
        })?;
        if binding.kind != kind
            || binding.plan_hash != plan_hash
            || binding.request_hash != request_hash
            || binding.expires_at_unix_ms <= chrono::Utc::now().timestamp_millis()
        {
            return Err(MmError::Security {
                message: "MCP review does not match this exact current mutation request".to_owned(),
            });
        }
        Ok(())
    }

    pub(super) fn complete(
        &self,
        context: &McpRequestContext,
        operation: &str,
        mutation: bool,
        plan_hash: Option<&str>,
        result: Result<serde_json::Value, MmError>,
    ) -> CallToolResult {
        let (outcome, value) = match result {
            Ok(mut value) => {
                scrub_sensitive(&mut value, false);
                let size = serde_json::to_vec(&value).map_or(usize::MAX, |bytes| bytes.len());
                if size > self.config.maximum_response_bytes {
                    (
                        "MM_MCP_RESULT_TOO_LARGE".to_owned(),
                        structured_error(
                            "MM_MCP_RESULT_TOO_LARGE",
                            "Result exceeds the configured MCP response bound.",
                            "Request a smaller page or tighter log bounds.",
                            "not_started",
                            context.request_id,
                        ),
                    )
                } else {
                    ("OK".to_owned(), CallToolResult::structured(value))
                }
            }
            Err(error) => {
                let code = error.code().to_owned();
                let (message, next) = safe_error_message(&error);
                (
                    code.clone(),
                    structured_error(
                        &code,
                        &message,
                        &next,
                        if mutation { "unknown" } else { "not_started" },
                        context.request_id,
                    ),
                )
            }
        };
        let audit = self.audit_record(context, operation, mutation, plan_hash, &outcome);
        if self.audit.append_mcp_audit(&audit).is_err() {
            return audit_unavailable(mutation, context.request_id);
        }
        value
    }

    pub(super) fn begin_mutation(
        &self,
        context: &McpRequestContext,
        operation: &str,
        plan_hash: &str,
    ) -> Result<(), MmError> {
        self.audit
            .append_mcp_audit(&self.audit_record(
                context,
                operation,
                true,
                Some(plan_hash),
                "STARTED",
            ))
            .map_err(|_| MmError::State {
                message: "MCP mutation audit journal is unavailable".to_owned(),
            })
    }

    pub(super) fn audit_record(
        &self,
        context: &McpRequestContext,
        operation: &str,
        mutation: bool,
        plan_hash: Option<&str>,
        outcome_code: &str,
    ) -> McpAuditRecord {
        McpAuditRecord {
            id: uuid::Uuid::new_v4(),
            request_id: context.request_id,
            principal: context.principal.clone(),
            source: context.source.clone(),
            project_root: context.project_root.clone(),
            environment: context.scope.environment.clone(),
            target: context.scope.target.clone(),
            operation: operation.to_owned(),
            mutation,
            plan_hash: plan_hash.map(str::to_owned),
            outcome_code: outcome_code.to_owned(),
            occurred_at_unix_ms: chrono::Utc::now().timestamp_millis(),
        }
    }

    pub(crate) fn schema_snapshot(&self) -> serde_json::Value {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        serde_json::json!({
            "schema_version": MCP_TOOL_SCHEMA_VERSION,
            "mm_version": env!("CARGO_PKG_VERSION"),
            "project_schema_version": MCP_PROJECT_SCHEMA_VERSION,
            "tools": tools,
        })
    }
}
