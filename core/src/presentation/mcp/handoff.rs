//! Refusals an agent can act on: what it may not do itself, why, and the
//! exact commands the user runs instead.

use super::*;
use crate::domain::mcp::{McpDenial, McpDenialReason};

/// What the user runs when the agent may not.
#[derive(Debug, Clone, Default)]
pub(super) struct UserCommands {
    pub(super) commands: Vec<String>,
    pub(super) note: Option<String>,
}

impl UserCommands {
    pub(super) fn of(commands: impl IntoIterator<Item = String>) -> Self {
        Self {
            commands: commands.into_iter().collect(),
            note: None,
        }
    }

    pub(super) fn with_note(mut self, note: &str) -> Self {
        self.note = Some(note.to_owned());
        self
    }
}

/// `mm --environment <environment> <words…>`, quoted for a POSIX shell.
pub(super) fn mm_command(environment: &str, words: &[&str]) -> String {
    let mut command = format!("mm --environment {}", shell_quote(environment));
    for word in words {
        command.push(' ');
        command.push_str(&shell_quote(word));
    }
    command
}

pub(super) fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':' | b'=')
        })
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

const WAIT_FOR_USER: &str = "Show the user these commands and ask them to run them in their own terminal, outside this agent session. Do not run mm, mmd, docker, podman, or ssh yourself, and do not work around the refusal. Wait for the user before continuing.";

/// The tool result for a refused action.
pub(super) fn handoff_result(denial: &McpDenial, requested: UserCommands) -> CallToolResult {
    let (message, user) = match denial.reason {
        McpDenialReason::NotGranted => (
            format!(
                "Agents have '{}' in environment '{}'; this needs {}. The user can run it instead.",
                denial.access.summary(),
                denial.environment,
                denial.action.requirement(),
            ),
            requested,
        ),
        McpDenialReason::PolicyNotTrusted => (
            format!(
                "The project's agents: block changed since the user last trusted it, so agents have '{}' in every environment until the user reviews it.",
                denial.access.summary(),
            ),
            UserCommands::of(["mm agent trust".to_owned()])
                .with_note("After the user trusts the block, call the tool again."),
        ),
        McpDenialReason::AgentKeyMissing => (
            format!(
                "Environment '{}' is remote and this device has no agent key. Agents reach remote hosts only with their own key, which the host limits.",
                denial.environment,
            ),
            UserCommands::of([
                "mm key create --agent".to_owned(),
                "mm team join --agent".to_owned(),
                mm_command(&denial.environment, &["team", "sync"]),
            ])
            .with_note("`mm team join --agent` edits the users file: the user commits it, has it merged, then syncs or deploys the environment."),
        ),
        McpDenialReason::AccessEdit => (
            "Users and agent access are never edited through MCP; they go through their own review.".to_owned(),
            requested,
        ),
    };
    CallToolResult::structured_error(serde_json::json!({
        "schema_version": MCP_TOOL_SCHEMA_VERSION,
        "error": {
            "code": "MM_AGENT_HANDOFF",
            "message": message,
            "retryable": false,
            "mutation_state": "not_started",
            "next_action": WAIT_FOR_USER,
            "request_id": denial.request_id,
            "handoff": {
                "reason": denial.reason,
                "environment": denial.environment,
                "action": denial.action,
                "service": denial.service,
                "granted": denial.access.summary(),
                "requires": denial.action.requirement(),
                "user_commands": user.commands,
                "note": user.note,
            },
        }
    }))
}

/// The user's commands for a build: review, then apply the reviewed hash.
pub(super) fn build_commands(environment: &str, publish: bool) -> UserCommands {
    let publish = if publish {
        vec!["--publish"]
    } else {
        Vec::new()
    };
    let mut plan = vec!["build"];
    plan.extend(&publish);
    plan.push("plan");
    let mut apply = vec!["build"];
    apply.extend(&publish);
    apply.extend(["apply", "--plan", "HASH", "--yes"]);
    UserCommands::of([
        mm_command(environment, &plan),
        mm_command(environment, &apply),
    ])
    .with_note("HASH is the plan hash the first command prints.")
}

pub(super) fn deploy_commands(
    environment: &str,
    kind: crate::domain::mcp::McpDeployPlanKind,
) -> UserCommands {
    use crate::domain::mcp::McpDeployPlanKind as K;
    let named = match kind {
        K::Rollback => Some("rollback"),
        K::Redeploy => Some("redeploy"),
        K::Checkout => None,
    };
    if let Some(named) = named {
        return UserCommands::of([mm_command(environment, &["deploy", named])])
            .with_note("It prints the plan and asks before it applies it.");
    }
    UserCommands::of([
        mm_command(environment, &["deploy", "plan"]),
        mm_command(environment, &["deploy", "apply", "--plan", "HASH", "--yes"]),
    ])
    .with_note("HASH is the plan hash the first command prints.")
}

pub(super) fn job_commands(environment: &str, action: &McpJobAction) -> UserCommands {
    let run_id;
    let words: Vec<&str> = match action {
        McpJobAction::Reconcile => {
            return UserCommands::of([
                mm_command(environment, &["job", "schedule", "reconcile"]),
                mm_command(
                    environment,
                    &["job", "schedule", "reconcile", "--plan", "HASH", "--yes"],
                ),
            ])
            .with_note("HASH is the plan hash the first command prints.");
        }
        McpJobAction::Enqueue { name } => vec!["job", "run", name],
        McpJobAction::RunSchedule { name } => vec!["job", "schedule", "run-now", name],
        McpJobAction::Cancel { run_id: id } => {
            run_id = id.to_string();
            vec!["job", "cancel", &run_id]
        }
        McpJobAction::Retry { run_id: id } => {
            run_id = id.to_string();
            vec!["job", "retry", &run_id]
        }
        McpJobAction::SetScheduleEnabled { name, enabled } => vec![
            "job",
            "schedule",
            if *enabled { "enable" } else { "disable" },
            name,
        ],
    };
    UserCommands::of([mm_command(environment, &words)])
}

pub(super) fn exec_commands(request: &McpExecRequest) -> UserCommands {
    let timeout;
    let mut words = vec!["service", "exec"];
    let workdir = request
        .workdir
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    for (flag, value) in [
        ("--stack", request.stack.as_deref()),
        ("--user", request.user.as_deref()),
        ("--workdir", workdir.as_deref()),
    ] {
        if let Some(value) = value {
            words.extend([flag, value]);
        }
    }
    if request.timeout_seconds != 30 {
        // The CLI's own default is 30 seconds.
        timeout = request.timeout_seconds.to_string();
        words.extend(["--timeout", &timeout]);
    }
    words.push(&request.service);
    words.push("--");
    words.extend(request.argv.iter().map(String::as_str));
    UserCommands::of([mm_command(&request.scope.environment, &words)])
}

pub(super) fn config_commands() -> UserCommands {
    UserCommands::default().with_note(
        "Show the user the planned diff from mm_config_plan. They can make the same edit to the configuration file, or grant agents `deploy` in this environment.",
    )
}

pub(super) fn stage_commands(request: &crate::domain::mcp::McpStageValueRequest) -> UserCommands {
    use crate::domain::mcp::McpStageAction as A;
    let group = match request.kind {
        crate::domain::staged::ValueKind::Secret => "secret",
        crate::domain::staged::ValueKind::Variable => "env",
    };
    let action = match request.action {
        A::Set => "set",
        A::Unset => "unset",
        A::Discard => "discard",
    };
    let commands = UserCommands::of([mm_command(
        &request.scope.environment,
        &[group, action, &request.key],
    )]);
    if request.action == A::Set {
        commands.with_note("It asks for the value without showing it.")
    } else {
        commands
    }
}

pub(super) fn development_commands(
    request: &crate::domain::mcp::McpDevelopmentUnitRequest,
) -> UserCommands {
    let verb = if request.on { "enable" } else { "disable" };
    UserCommands::of([mm_command(
        &request.scope.environment,
        &["dev", verb, &request.unit],
    )])
}
