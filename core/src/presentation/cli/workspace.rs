//! `mm workspace`: the project folders mm knows on this machine.

use super::*;
use crate::domain::workspace_registry::{
    FORGET_STALE_COMMAND, RegisteredWorkspace, WorkspaceForgetPlan, WorkspaceSelector,
};

#[derive(Debug, Clone, Subcommand)]
pub enum WorkspaceCommand {
    /// List the project folders mm has run in on this machine, when each
    /// was last used, and which are gone.
    List,
    /// Forget registered project folders through a reviewed plan.
    ///
    /// mm drops their operation history and `mm env set` values on this
    /// machine. No file is deleted.
    #[command(
        after_help = "Examples:\n  mm workspace forget --stale\n  mm workspace forget --stale --plan HASH --yes\n  mm workspace forget ~/src/old-checkout\n\nA folder that still exists is registered again the next time mm runs there."
    )]
    Forget {
        /// A registered folder, or a project or workspace ID from
        /// `mm workspace list --output json`.
        #[arg(
            value_name = "ROOT|ID",
            required_unless_present = "stale",
            conflicts_with = "stale"
        )]
        workspace: Option<String>,
        /// Forget every registered workspace whose folder or mm.yaml is gone.
        #[arg(long)]
        stale: bool,
        #[command(flatten)]
        review: WorkspaceReview,
    },
}

#[derive(Debug, Clone, Args)]
pub struct WorkspaceReview {
    /// Exact hash printed by an earlier review of the same change.
    #[arg(long, value_name = "HASH", requires = "yes")]
    plan: Option<String>,
    /// Apply the exact reviewed plan.
    #[arg(long, requires = "plan")]
    yes: bool,
}

pub(super) fn run(
    command: WorkspaceCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        WorkspaceCommand::List => {
            let workspaces = composition::workspace_registry(invocation.home(), false)?.list()?;
            write_list(&workspaces, invocation.format).map(|()| 0)
        }
        WorkspaceCommand::Forget {
            workspace,
            stale,
            review,
        } => {
            let selector = selector(workspace, stale)?;
            let registry =
                composition::workspace_registry(invocation.home(), review.plan.is_some())?;
            let Some(reviewed) = review.plan else {
                let plan = registry.forget_plan(&selector)?;
                return write_plan(&plan, &selector, invocation).map(|()| 0);
            };
            let result = registry.forget(&selector, &reviewed)?;
            if invocation.format == OutputFormat::Json {
                return output::write_json(&result).map(|()| 0);
            }
            crate::outln!(
                "Forgot {} and {}. No file was deleted.",
                count(result.forgotten.len() as u64, "workspace"),
                count(result.operations, "operation")
            );
            Ok(0)
        }
    }
}

fn selector(workspace: Option<String>, stale: bool) -> Result<WorkspaceSelector, MmError> {
    let Some(given) = workspace.filter(|_| !stale) else {
        return Ok(WorkspaceSelector::Stale);
    };
    let path = match std::fs::canonicalize(&given) {
        Ok(path) => path,
        Err(_) => std::path::absolute(&given).map_err(|source| MmError::Io {
            context: format!("resolving the workspace root {given}"),
            source,
        })?,
    };
    let given = if uuid::Uuid::parse_str(given.trim()).is_ok() {
        given
    } else {
        path.display().to_string()
    };
    Ok(WorkspaceSelector::One { given, path })
}

fn count(amount: u64, noun: &str) -> String {
    format!("{amount} {noun}{}", if amount == 1 { "" } else { "s" })
}

fn write_list(workspaces: &[RegisteredWorkspace], format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(&serde_json::json!({ "workspaces": workspaces }));
    }
    if workspaces.is_empty() {
        crate::outln!(
            "No project folder is registered on this machine yet; mm registers one when it runs there."
        );
        return Ok(());
    }
    crate::outln!(
        "{:<19}  {:<20}  {:<36}  ROOT",
        "STATE",
        "LAST USED",
        "PROJECT"
    );
    for workspace in workspaces {
        crate::outln!(
            "{:<19}  {:<20}  {:<36}  {}",
            workspace.presence.describe(),
            crate::domain::rollout::utc_time(workspace.last_used_at_unix_ms),
            workspace.project_id.0,
            output::terminal_safe(&workspace.root.display().to_string())
        );
    }
    let stale = workspaces
        .iter()
        .filter(|workspace| workspace.stale)
        .count();
    if stale > 0 {
        crate::outln!(
            "\n{} of {} registered folders {} gone. Forget {}: {FORGET_STALE_COMMAND}",
            stale,
            workspaces.len(),
            if stale == 1 { "is" } else { "are" },
            if stale == 1 { "it" } else { "them" }
        );
    }
    Ok(())
}

fn write_plan(
    plan: &WorkspaceForgetPlan,
    selector: &WorkspaceSelector,
    invocation: &super::dispatch::Invocation,
) -> Result<(), MmError> {
    if invocation.format == OutputFormat::Json {
        return output::write_json(plan);
    }
    if plan.workspaces.is_empty() {
        crate::outln!("Nothing to forget: every registered workspace is on disk.");
        return Ok(());
    }
    crate::outln!(
        "Forget {}:",
        count(plan.workspaces.len() as u64, "registered workspace")
    );
    for workspace in &plan.workspaces {
        crate::outln!(
            "- {} (project {}, {}, last used {}, {})",
            output::terminal_safe(&workspace.root.display().to_string()),
            workspace.project_id.0,
            workspace.presence.describe(),
            crate::domain::rollout::utc_time(workspace.last_used_at_unix_ms),
            count(workspace.operations, "operation")
        );
    }
    crate::outln!(
        "\nmm deletes their operation history and their `mm env set` values from its state on this machine. No file is deleted."
    );
    if plan.workspaces.iter().any(|workspace| !workspace.stale) {
        crate::outln!(
            "A folder that still exists is registered again the next time mm runs there."
        );
    }
    crate::outln!(
        "Apply: {}",
        output::terminal_safe(&output::display_command(&apply_command(
            plan, selector, invocation
        )))
    );
    Ok(())
}

fn apply_command(
    plan: &WorkspaceForgetPlan,
    selector: &WorkspaceSelector,
    invocation: &super::dispatch::Invocation,
) -> Vec<String> {
    let mut argv = vec!["mm".to_owned()];
    for pair in invocation.recovery_context.arguments.chunks(2) {
        if pair.first().is_some_and(|flag| flag == "--mm-home") {
            argv.extend(pair.iter().cloned());
        }
    }
    argv.extend(["workspace".to_owned(), "forget".to_owned()]);
    argv.push(match selector {
        WorkspaceSelector::Stale => "--stale".to_owned(),
        WorkspaceSelector::One { given, .. } => given.clone(),
    });
    argv.extend([
        "--plan".to_owned(),
        plan.plan_hash.clone(),
        "--yes".to_owned(),
    ]);
    argv
}
