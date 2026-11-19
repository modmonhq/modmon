use super::*;
use crate::domain::host_policy::{AccessLevel, ServiceSelection};

#[test]
fn private_limits_validate_and_fail_closed() {
    let valid = McpServerConfig::default();
    assert_eq!(valid.validate(), Ok(()));

    let mutations: [fn(&mut McpServerConfig); 9] = [
        |value| value.schema_version += 1,
        |value| value.principal.clear(),
        |value| value.principal = "x".repeat(256),
        |value| value.principal = "line\nbreak".to_owned(),
        |value| {
            value.ceiling.insert(
                "bad name".to_owned(),
                crate::domain::agents::AgentAccessConfig::Level(AccessLevel::View),
            );
        },
        |value| {
            value.ceiling.insert(
                "*".to_owned(),
                crate::domain::agents::AgentAccessConfig::Level(AccessLevel::Admin),
            );
        },
        |value| value.maximum_page_size = 0,
        |value| value.maximum_response_bytes = MCP_MAX_RESPONSE_BYTES + 1,
        |value| value.request_timeout_ms = MCP_MAX_REQUEST_TIMEOUT_MS + 1,
    ];
    for mutation in mutations {
        let mut candidate = valid.clone();
        mutation(&mut candidate);
        assert!(candidate.validate().is_err());
    }
    let mut newer = valid.clone();
    newer.schema_version = 2;
    assert!(newer.validate().is_err());
}

#[test]
fn a_private_ceiling_only_narrows_the_project_grant() -> Result<(), serde_json::Error> {
    let config: McpServerConfig = serde_json::from_value(serde_json::json!({
        "schema_version": MCP_CONFIG_SCHEMA_VERSION,
        "ceiling": {"*": "view", "local": {"level": "operate", "exec": ["api"]}}
    }))?;
    assert_eq!(config.validate(), Ok(()));
    assert_eq!(config.principal, "agent");
    let granted = AgentAccess {
        level: AccessLevel::Deploy,
        exec: ServiceSelection::All(true),
        secrets: false,
    };
    assert_eq!(
        config.limit("local", &granted).summary(),
        "operate +exec[api]"
    );
    assert_eq!(config.limit("production", &granted), AgentAccess::VIEW);
    // A ceiling above the grant adds nothing.
    assert_eq!(config.limit("local", &AgentAccess::VIEW), AgentAccess::VIEW);
    // No entry at all: the project's grant stands.
    assert_eq!(McpServerConfig::default().limit("local", &granted), granted);
    Ok(())
}

#[test]
fn an_authorized_context_still_checks_each_action() {
    let context = McpRequestContext {
        request_id: uuid::Uuid::from_u128(1),
        principal: "agent".to_owned(),
        source: MCP_SOURCE.to_owned(),
        project_root: PathBuf::from("/workspace"),
        scope: McpSelectedScope {
            environment: "staging".to_owned(),
            target: "prod-eu".to_owned(),
            runtime: RuntimeKind::DockerCompose,
        },
        remote: true,
        access: AgentAccess {
            level: AccessLevel::Operate,
            exec: ServiceSelection::Only(["api".to_owned()].into()),
            secrets: false,
        },
    };
    assert!(context.require(AgentAction::Logs, None).is_ok());
    assert!(context.require(AgentAction::JobRun, None).is_ok());
    assert!(context.require(AgentAction::Exec, Some("api")).is_ok());
    assert!(
        context
            .require(AgentAction::Exec, Some("postgres"))
            .is_err()
    );
    assert!(context.require(AgentAction::DeployApply, None).is_err());
    let mut forged = context;
    forged.source = "forged-source".to_owned();
    assert!(forged.require(AgentAction::Read, None).is_err());
}

#[test]
fn job_actions_map_to_operate_or_deploy() {
    assert_eq!(
        McpJobAction::Enqueue {
            name: "job".to_owned()
        }
        .agent_action(),
        AgentAction::JobRun
    );
    assert_eq!(
        McpJobAction::Reconcile.agent_action(),
        AgentAction::ScheduleManage
    );
    assert_eq!(
        McpJobAction::SetScheduleEnabled {
            name: "nightly".to_owned(),
            enabled: false
        }
        .agent_action(),
        AgentAction::ScheduleManage
    );
}

#[test]
fn sensitive_fields_are_rejected_at_any_depth() {
    let safe = serde_json::json!({"nested": [{"token_hint": "public"} ]});
    assert_eq!(reject_model_visible_sensitive_fields(&safe), Ok(()));
    for key in ["password", "private-key", "SESSION_TOKEN", "refresh_token"] {
        let value = serde_json::json!({"nested": [{key: "sentinel"}]});
        assert!(reject_model_visible_sensitive_fields(&value).is_err());
    }
}

#[test]
fn serde_defaults_are_bounded() -> Result<(), serde_json::Error> {
    let config: McpServerConfig = serde_json::from_value(serde_json::json!({
        "schema_version": MCP_CONFIG_SCHEMA_VERSION
    }))?;
    assert_eq!(config.maximum_page_size, 50);
    assert_eq!(config.maximum_response_bytes, 512 * 1024);
    assert_eq!(config.request_timeout_ms, 15_000);

    // Only the environment is required in a scope.
    let request: McpReadRequest = serde_json::from_value(serde_json::json!({
        "scope": {"environment": "local"},
        "kind": "project"
    }))?;
    assert_eq!(request.limit, 50);
    assert_eq!(request.scope.target, None);
    let log: McpLogRequest = serde_json::from_value(serde_json::json!({
        "scope": request.scope,
        "service": "web"
    }))?;
    assert_eq!(log.tail, 100);
    let exec: McpExecRequest = serde_json::from_value(serde_json::json!({
        "scope": {"environment": "local", "runtime": "docker-compose"},
        "service": "web",
        "argv": ["ls", "-la"]
    }))?;
    assert_eq!(exec.timeout_seconds, 30);
    assert!(
        serde_json::from_value::<McpExecRequest>(serde_json::json!({
            "scope": {"environment": "local"},
            "service": "web",
            "argv": ["ls"],
            "tty": true
        }))
        .is_err()
    );
    Ok(())
}
