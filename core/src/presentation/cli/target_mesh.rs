//! `mm target mesh`: the WireGuard mesh between a cluster target's nodes.

use std::sync::Arc;

use clap::Subcommand;

use super::OutputFormat;
use crate::composition;
use crate::domain::error::MmError;
use crate::presentation::inline;
use crate::presentation::output;

#[derive(Debug, Clone, Subcommand)]
pub enum TargetMeshCommand {
    /// Show every node's mesh interface and, per peer, its endpoint, last
    /// handshake, and traffic. Lists what is wrong with what to do about
    /// it, such as the UDP port to open between two hosts. Exits 3 when a
    /// node does not answer or the mesh has a problem.
    Status,
    /// Replace one node's WireGuard key pair. Without --plan this prints
    /// the plan: the node makes a new key and every other node replaces its
    /// one entry for it. Nothing else changes. The private key never leaves
    /// the node.
    Rotate {
        /// The node whose key is replaced.
        #[arg(value_name = "NODE")]
        node: String,
        /// Apply the plan with this hash, as printed without --plan.
        #[arg(long, value_name = "HASH")]
        plan: Option<String>,
        /// Skip the confirmation prompt.
        #[arg(long, requires = "plan")]
        yes: bool,
    },
}

pub(super) async fn run(
    command: TargetMeshCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<u8, MmError> {
    match command {
        TargetMeshCommand::Status => {
            let progress = inline::Progress::new(invocation.format);
            let report = inline::step(
                &progress,
                ("mesh", "Reading the mesh on every node"),
                composition::target_mesh(
                    &invocation.selection,
                    invocation.home(),
                    composition::RemoteIdentity::User,
                ),
                |_| Some("Read the mesh on every node".to_owned()),
            )
            .await?;
            progress.finish();
            output::write_mesh_report(&report, invocation.format)?;
            Ok(if report.healthy() { 0 } else { 3 })
        }
        TargetMeshCommand::Rotate { node, plan, yes } => {
            rotate(invocation, node, plan.as_deref(), yes).await
        }
    }
}

async fn rotate(
    invocation: &super::dispatch::Invocation,
    node: String,
    reviewed: Option<&str>,
    yes: bool,
) -> Result<u8, MmError> {
    let services = composition::cluster_setup_services(
        &invocation.start,
        invocation.home(),
        invocation.services.process(),
        Arc::clone(&invocation.clock),
    )?;
    let request = crate::app::MeshRotationRequest {
        start: invocation.start.clone(),
        environment: invocation.selection.environment.clone(),
        node,
    };
    let plan = services.service.plan_mesh_rotation(&request).await?;
    let Some(reviewed) = reviewed else {
        output::write_mesh_rotation_plan(&plan, invocation.format)?;
        return Ok(0);
    };
    if plan.plan_hash != reviewed {
        return Err(MmError::State {
            message: format!(
                "the mesh rotation plan changed (expected {reviewed}, current {}); run `mm target mesh rotate {}` and review the new plan",
                plan.plan_hash, plan.node
            ),
        });
    }
    if invocation.format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON mesh rotation requires --yes".to_owned(),
        });
    }
    if invocation.format == OutputFormat::Human {
        output::write_mesh_rotation_plan(&plan, invocation.format)?;
    }
    if !yes
        && !super::terminal::confirm(&format!(
            "Replace the mesh key of {}?",
            output::terminal_safe(&plan.node)
        ))?
    {
        return Ok(1);
    }
    let result = services
        .service
        .apply_mesh_rotation(&request, reviewed)
        .await?;
    output::write_mesh_rotation_result(&result, invocation.format)?;
    Ok(0)
}
