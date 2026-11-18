//! `mm agent`: what AI agents may do in this project, and connecting
//! coding tools to `mm mcp serve`. Also the guard that keeps an agent from
//! running `mm` itself.

use super::*;
use crate::composition::{AgentClient, AgentSetupScope, FileAction};

#[derive(Debug, Clone, Subcommand)]
pub enum AgentCommand {
    /// Show what agents may do in each environment and whether this device
    /// trusts the project's `agents:` block.
    Status,
    /// Review the project's `agents:` block and trust it on this device.
    /// Until then, and after any edit to the block, agents can only view.
    #[command(after_help = "Examples:\n  mm agent trust\n  mm agent trust --plan HASH --yes")]
    Trust {
        /// Exact hash printed by the review of the block.
        #[arg(long, value_name = "HASH", requires = "yes")]
        plan: Option<String>,
        /// Trust the reviewed block.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
    /// Connect coding tools to `mm mcp serve`.
    #[command(
        after_help = "Examples:\n  mm agent setup\n  mm agent setup --client claude-code --yes\n  mm agent setup --user --yes\n\nWithout --yes the changes are only shown. In a project, the files are meant to be committed: the MCP server entry, instructions in AGENTS.md, and for Claude Code a rule that denies running mm directly. With --user, mm registers the server in each tool's own configuration for every project."
    )]
    Setup {
        /// Tool to connect. Repeatable. Defaults to the tools found on PATH.
        #[arg(long = "client", value_enum, value_name = "TOOL")]
        clients: Vec<AgentClientArg>,
        /// Register the server for this user instead of this project.
        #[arg(long)]
        user: bool,
        /// Make the changes shown.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentClientArg {
    #[value(name = "claude-code")]
    ClaudeCode,
    #[value(name = "opencode")]
    Opencode,
    #[value(name = "codex")]
    Codex,
    #[value(name = "cursor")]
    Cursor,
}

impl From<AgentClientArg> for AgentClient {
    fn from(value: AgentClientArg) -> Self {
        match value {
            AgentClientArg::ClaudeCode => Self::ClaudeCode,
            AgentClientArg::Opencode => Self::Opencode,
            AgentClientArg::Codex => Self::Codex,
            AgentClientArg::Cursor => Self::Cursor,
        }
    }
}

/// Refuses every command an AI agent runs directly. An agent works through
/// `mm mcp serve`, where the project's `agents:` block applies; the user
/// runs everything else in their own terminal. Detection relies on the
/// environment variables coding tools set, so it stops an agent that
/// follows its instructions, not one that hides.
pub(super) fn refuse_direct_agent_use(cli: &Cli) -> Result<(), MmError> {
    if matches!(
        cli.command,
        Some(
            Command::Mcp { .. }
                | Command::Generate { .. }
                | Command::Relay { .. }
                | Command::DashboardSchema
                | Command::InstallIngress { .. }
                | Command::UninstallIngress { .. }
        )
    ) {
        return Ok(());
    }
    let variables = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect::<Vec<_>>();
    let Some(marker) = crate::domain::agents::detect_agent(
        variables
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    ) else {
        return Ok(());
    };
    Err(MmError::Security {
        message: format!(
            "mm does not run for an AI agent ({marker} is set). Use the mm MCP tools instead: start with mm_access. If a tool returns MM_AGENT_HANDOFF, or no tool covers this, show the user the command and ask them to run it in their own terminal"
        ),
    })
}

pub(super) fn run(
    command: AgentCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        AgentCommand::Status => {
            let status = composition::agent_status(&invocation.start, invocation.home())?;
            write_status(&status, invocation.format, None)?;
            Ok(0)
        }
        AgentCommand::Trust { plan, .. } => {
            let Some(reviewed) = plan else {
                let status = composition::agent_status(&invocation.start, invocation.home())?;
                let next = format!("mm agent trust --plan {} --yes", status.policy_hash);
                write_status(&status, invocation.format, Some(&next))?;
                return Ok(0);
            };
            let status =
                composition::trust_agents(&invocation.start, invocation.home(), Some(&reviewed))?;
            write_status(&status, invocation.format, None)?;
            Ok(0)
        }
        AgentCommand::Setup { clients, user, yes } => setup(invocation, &clients, user, yes),
    }
}

fn write_status(
    status: &composition::AgentStatus,
    format: OutputFormat,
    next: Option<&str>,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        // A review names its plan hash and the command that trusts it, as
        // the text does.
        let Some(next) = next else {
            return output::write_json(status);
        };
        let mut value = serde_json::to_value(status).map_err(|error| MmError::State {
            message: format!("serializing the agent status: {error}"),
        })?;
        if let Some(object) = value.as_object_mut() {
            object.insert("plan_hash".to_owned(), status.policy_hash.clone().into());
            object.insert("next".to_owned(), next.into());
        }
        return output::write_json(&value);
    }
    crate::outln!(
        "Agents in {}",
        output::terminal_safe(&status.project_root.display().to_string())
    );
    if !status.declared {
        crate::outln!("  no agents: block; agents can view every environment and nothing else");
    }
    for environment in &status.environments {
        let note = if environment.granted == environment.in_force {
            String::new()
        } else {
            format!("  (in force now: {})", environment.in_force)
        };
        crate::outln!(
            "  {:<16} {}{}",
            output::terminal_safe(&environment.environment),
            output::terminal_safe(&environment.granted),
            output::terminal_safe(&note),
        );
    }
    crate::outln!(
        "  block {}: {}",
        &status.policy_hash[..12.min(status.policy_hash.len())],
        if status.trusted {
            "trusted on this device"
        } else {
            "not trusted on this device; agents can only view"
        }
    );
    match &status.agent_key {
        Some(fingerprint) => crate::outln!("  agent key: {}", output::terminal_safe(fingerprint)),
        None => crate::outln!(
            "  agent key: none; remote environments need `mm key create --agent` and `mm team join --agent`"
        ),
    }
    if let Some(path) = &status.private_limits {
        crate::outln!(
            "  private limits: {}",
            output::terminal_safe(&path.display().to_string())
        );
    }
    match next {
        Some(next) => crate::outln!("Trust this block: {next}"),
        None if !status.trusted => crate::outln!("Review the block, then run `mm agent trust`."),
        None => {}
    }
    Ok(())
}

fn setup(
    invocation: &super::dispatch::Invocation,
    clients: &[AgentClientArg],
    user: bool,
    yes: bool,
) -> Result<u8, MmError> {
    let scope = if user {
        AgentSetupScope::User
    } else {
        AgentSetupScope::Project
    };
    let requested = clients
        .iter()
        .copied()
        .map(AgentClient::from)
        .collect::<Vec<_>>();
    let (clients, plan) = composition::plan_agent_setup(&invocation.start, scope, &requested)?;
    let failures = if yes {
        composition::apply_agent_setup(&plan)?
    } else {
        Vec::new()
    };
    if invocation.format == OutputFormat::Json {
        output::write_json(&serde_json::json!({
            "clients": clients,
            "applied": yes,
            "plan": plan,
            "failures": failures,
        }))?;
        return Ok(u8::from(!failures.is_empty()));
    }
    if clients.is_empty() {
        crate::outln!(
            "No supported coding tool found on PATH (claude, opencode, codex, cursor-agent). Name one with --client."
        );
        return Ok(0);
    }
    for change in &plan.files {
        crate::outln!(
            "  {} {}: {}",
            match (yes, change.action) {
                (true, FileAction::Create) => "created",
                (true, FileAction::Update) => "updated",
                (false, FileAction::Create) => "would create",
                (false, FileAction::Update) => "would update",
            },
            output::terminal_safe(&change.path.display().to_string()),
            output::terminal_safe(&change.summary),
        );
    }
    for step in &plan.commands {
        crate::outln!(
            "  {} `{} {}`: {}",
            if yes { "ran" } else { "would run" },
            output::terminal_safe(&step.program),
            output::terminal_safe(&step.arguments.join(" ")),
            output::terminal_safe(&step.summary),
        );
    }
    for note in &plan.notes {
        crate::outln!("  note: {}", output::terminal_safe(note));
    }
    for failure in &failures {
        crate::outln!("  failed: {}", output::terminal_safe(failure));
    }
    if plan.is_empty() {
        crate::outln!("Nothing to change.");
    } else if !yes {
        crate::outln!("Run again with --yes to make these changes.");
    } else if !user {
        crate::outln!(
            "Commit these files. Agents can only view until you run `mm agent trust`; `mm agent status` shows what they may do."
        );
    }
    Ok(u8::from(!failures.is_empty()))
}
