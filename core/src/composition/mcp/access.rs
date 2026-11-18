//! What an agent may do: the project's `agents:` block, the user's trust in
//! it, and this device's private ceiling.

use super::*;

impl McpApplicationAdapter {
    fn trusted(&self, document: &ConfigDocument) -> bool {
        // No block grants `view`, the floor; there is nothing to trust.
        document.config.agents.is_empty()
            || AgentTrustStore::new(&self.mm_home).is_trusted(
                &document.root,
                document.config.project.id,
                &document.config.agents,
            )
    }

    fn grant(&self, document: &ConfigDocument, environment: &str) -> Grant {
        let trusted = self.trusted(document);
        let declared = self.config.limit(
            environment,
            &agent_access_in(&document.config.agents, environment),
        );
        Grant {
            access: if trusted {
                declared.clone()
            } else {
                AgentAccess::VIEW
            },
            declared,
            trusted,
        }
    }

    /// The target and runtime a scope selects, and whether the target is a
    /// remote host. A project without a usable target still resolves, so
    /// reads and configuration plans work before `mm dev setup`.
    fn select(
        &self,
        document: &ConfigDocument,
        scope: &crate::domain::mcp::McpScope,
    ) -> Result<(McpSelectedScope, bool), MmError> {
        let home = MmHome::resolve(Some(&self.mm_home))?;
        let state = SqliteStateStore::open(&home.state_database)?;
        let targets: Arc<dyn TargetStore> =
            Arc::new(FilesystemTargetStore::new(home.targets_file.clone()));
        let (environment_pref, project_pref) =
            crate::app::load_selection_preferences(Some(&state), document, &scope.environment)?;
        let resolved = crate::app::resolve_operation_context(
            document,
            &scope.environment,
            scope.runtime,
            scope.target.as_deref(),
            environment_pref.as_ref(),
            project_pref.as_ref(),
            targets.as_ref(),
        );
        Ok(match resolved {
            Ok(context) => (
                McpSelectedScope {
                    environment: scope.environment.clone(),
                    target: context.target.name.clone(),
                    runtime: context.runtime,
                },
                matches!(
                    context.transport,
                    crate::domain::target::TargetTransport::Ssh { .. }
                ),
            ),
            // The target or runtime is not usable yet. Keep the target the
            // environment names, never another one, so reads of the project
            // still work and anything that needs the target fails on it.
            Err(MmError::Target { .. } | MmError::Environment { .. }) => {
                let (target, _) = crate::app::resolve_target_name(
                    document,
                    &scope.environment,
                    scope.target.as_deref(),
                    environment_pref.as_ref(),
                    project_pref.as_ref(),
                )?;
                let remote = targets.resolve_target(&target).map_or_else(
                    |_| document.config.targets.contains_key(&target),
                    |named| {
                        matches!(
                            named.definition.transport,
                            crate::domain::target::TargetTransport::Ssh { .. }
                        )
                    },
                );
                let runtime = scope
                    .runtime
                    .or(document.config.runtime.driver)
                    .unwrap_or(RuntimeKind::PodmanCompose);
                (
                    McpSelectedScope {
                        environment: scope.environment.clone(),
                        target,
                        runtime,
                    },
                    remote,
                )
            }
            Err(error) => return Err(error),
        })
    }

    pub(super) fn authorize_request(
        &self,
        request: McpAuthorizationRequest,
    ) -> Result<McpRequestContext, McpAuthorizationError> {
        let request_id = uuid::Uuid::new_v4();
        let document = ConfigLoader.load_from(&self.start)?;
        let environment = request.scope.environment.clone();
        if !document.config.environments.contains_key(&environment) {
            return Err(MmError::Environment {
                message: format!(
                    "environment '{environment}' is not configured; mm_access lists the environments"
                ),
            }
            .in_own_words()
            .into());
        }
        let grant = self.grant(&document, &environment);
        let deny = |reason| {
            McpAuthorizationError::Denied(Box::new(McpDenial {
                request_id,
                reason,
                project_root: document.root.clone(),
                environment: environment.clone(),
                action: request.action,
                service: request.service.clone(),
                access: grant.access.clone(),
            }))
        };
        let service = request.service.as_deref();
        if !grant.access.permits(request.action, service) {
            return Err(deny(
                if !grant.trusted && grant.declared.permits(request.action, service) {
                    McpDenialReason::PolicyNotTrusted
                } else {
                    McpDenialReason::NotGranted
                },
            ));
        }
        let (scope, remote) = self.select(&document, &request.scope)?;
        let home = MmHome::resolve(Some(&self.mm_home))?;
        if remote && !crate::composition::agent_key_path(&home).is_file() {
            return Err(deny(McpDenialReason::AgentKeyMissing));
        }
        Ok(McpRequestContext {
            request_id,
            principal: self.config.principal.clone(),
            source: MCP_SOURCE.to_owned(),
            project_root: document.root,
            scope,
            remote,
            access: grant.access,
        })
    }

    pub(super) fn access_overview(&self) -> Result<McpAccessOverview, MmError> {
        let document = ConfigLoader.load_from(&self.start)?;
        let environments = document
            .config
            .environments
            .keys()
            .map(|environment| {
                let access = self.grant(&document, environment).access;
                McpEnvironmentAccess {
                    environment: environment.clone(),
                    access: access.summary(),
                    level: access.level,
                    exec: access.exec,
                }
            })
            .collect();
        let policy_trusted = self.trusted(&document);
        Ok(McpAccessOverview {
            schema_version: MCP_RESOURCE_SCHEMA_VERSION.to_owned(),
            project_root: document.root,
            policy_trusted,
            default_environment: document.config.default_environment,
            environments,
            outside_the_grant: if policy_trusted {
                "A tool call outside these grants returns MM_AGENT_HANDOFF with the exact command. Show it to the user, ask them to run it in their own terminal, and wait. Never run mm, docker, or podman yourself.".to_owned()
            } else {
                "The project's agents: block changed since the user trusted it, so every environment is view. Ask the user to review it and run `mm agent trust` in their own terminal.".to_owned()
            },
        })
    }
}
