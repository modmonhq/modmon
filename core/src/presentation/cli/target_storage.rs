//! `mm target storage`: the shared volumes of a cluster target.

use clap::Subcommand;

use super::OutputFormat;
use crate::composition;
use crate::domain::error::MmError;
use crate::presentation::inline;
use crate::presentation::output;

#[derive(Debug, Clone, Subcommand)]
pub enum TargetStorageCommand {
    /// Show the node that keeps shared volumes, each volume with its size
    /// and the nodes that have it mounted, and whether every node reaches
    /// the storage node. Also lists volumes that are no longer in the
    /// configuration and still have files. Exits 3 when a node does not
    /// answer or cannot reach the storage node.
    Status,
    /// Delete a shared volume's files from the storage node. Without
    /// --plan this prints what would be removed. Refused while a container
    /// on any node uses the volume. This is the only command that removes
    /// shared data: taking a volume out of `shared-volumes` keeps it.
    Delete {
        /// The volume's name, as listed under `shared-volumes`.
        #[arg(value_name = "VOLUME")]
        volume: String,
        /// The stack the volume belongs to, when more than one stack keeps
        /// a volume of this name.
        #[arg(long, value_name = "STACK")]
        stack: Option<String>,
        /// Delete what the plan with this hash showed, as printed without
        /// --plan.
        #[arg(long, value_name = "HASH")]
        plan: Option<String>,
        /// Skip the confirmation prompt.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
}

pub(super) async fn run(
    command: TargetStorageCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        TargetStorageCommand::Status => {
            let progress = inline::Progress::new(invocation.format);
            let report = inline::step(
                &progress,
                ("storage", "Reading shared volumes on every node"),
                composition::target_storage(
                    &invocation.selection,
                    invocation.home(),
                    composition::RemoteIdentity::User,
                    true,
                ),
                |_| Some("Read shared volumes on every node".to_owned()),
            )
            .await?;
            progress.finish();
            output::write_storage_report(&report, invocation.format)?;
            Ok(if report.healthy() { 0 } else { 3 })
        }
        TargetStorageCommand::Delete {
            volume,
            stack,
            plan,
            yes,
        } => {
            delete(
                invocation,
                (stack.as_deref(), &volume),
                plan.as_deref(),
                yes,
            )
            .await
        }
    }
}

async fn delete(
    invocation: &super::dispatch::Invocation,
    named: (Option<&str>, &str),
    reviewed: Option<&str>,
    yes: bool,
) -> Result<u8, MmError> {
    let plan =
        composition::plan_volume_delete(&invocation.selection, invocation.home(), named).await?;
    let Some(reviewed) = reviewed else {
        output::write_volume_delete_plan(&plan, invocation.format)?;
        return Ok(0);
    };
    if plan.plan_hash != reviewed {
        return Err(MmError::State {
            message: format!(
                "plan {reviewed} does not name deleting shared volume {}/{} on {}; run `mm target storage delete {}` and use the hash it prints",
                plan.stack, plan.volume, plan.node, plan.volume
            ),
        });
    }
    if invocation.format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON shared volume deletion requires --yes".to_owned(),
        });
    }
    if invocation.format == OutputFormat::Human {
        output::write_volume_delete_plan(&plan, invocation.format)?;
    }
    if !yes
        && !super::terminal::confirm(&format!(
            "Delete every file of shared volume {}/{}?",
            output::terminal_safe(&plan.stack),
            output::terminal_safe(&plan.volume)
        ))?
    {
        return Ok(1);
    }
    let deleted = composition::delete_volume(
        &invocation.selection,
        invocation.home(),
        (Some(&plan.stack), &plan.volume),
        reviewed,
    )
    .await?;
    output::write_volume_deleted(&deleted, invocation.format)?;
    Ok(0)
}
