//! `mm target setup` when the environment's repo target lists `nodes:`.

use std::sync::Arc;

use super::OutputFormat;
use crate::composition;
use crate::domain::cluster_setup::{ClusterSetupPlan, ClusterSetupResult};
use crate::domain::error::MmError;
use crate::presentation::inline::{self, Progress};
use crate::presentation::output;

use super::target::TargetBootstrapCommand;

/// Cluster setup's flags, and the release flags every setup reads. Pass the
/// same ones to plan and apply.
#[derive(Debug, Clone, Default, clap::Args)]
#[command(next_help_heading = "Cluster targets (nodes:)")]
pub struct ClusterSetupArgs {
    /// Accept SSH host keys that differ from the ones the repo records, such
    /// as after rebuilding a node. The plan shows each key's fingerprint.
    #[arg(long)]
    trust_new_host_keys: bool,
    /// Commit every node syncs the users file from (default: HEAD).
    #[arg(long, value_name = "SHA")]
    commit: Option<String>,
    /// Set up only these nodes, for example while another one cannot be
    /// reached. The others are not contacted and keep the version they run.
    #[arg(long, value_name = "NODE,...", value_delimiter = ',')]
    nodes: Vec<String>,
    #[command(flatten)]
    pub(super) release: ReleaseArgs,
}

/// Which mm and mmd setup may replace on the hosts. Pass the same flags to
/// plan and apply.
#[derive(Debug, Clone, Copy, Default, clap::Args)]
#[command(next_help_heading = "Releases (pass the same flags to plan and apply)")]
pub struct ReleaseArgs {
    /// Install this release on hosts that run a newer one.
    #[arg(long)]
    pub(super) downgrade: bool,
    /// Install this mm and mmd on hosts that run another build of it or
    /// an older release. Without it, setup refuses to replace what the
    /// hosts run, unless deploy.mm-version pins this release.
    #[arg(long)]
    pub(super) upgrade: bool,
}

/// Runs the command in cluster mode when the environment's target lists
/// nodes; `None` leaves it to single-host setup.
pub(super) async fn route(
    command: &TargetBootstrapCommand,
    invocation: &super::dispatch::Invocation,
) -> Result<Option<u8>, MmError> {
    let (environment, cluster) = composition::cluster_environment(
        &invocation.start,
        invocation.selection.environment.as_deref(),
    )?;
    match command {
        TargetBootstrapCommand::DeployKey => {
            let services = services(invocation, false)?;
            let (public_key, origin) = services
                .service
                .ensure_deploy_key(&invocation.start, Some(&environment), &services.secrets)
                .await?;
            write_deploy_key(&environment, &public_key, &origin, invocation.format)?;
            Ok(Some(0))
        }
        TargetBootstrapCommand::Record => {
            let (file, changed) =
                services(invocation, false)?.record(&invocation.start, Some(&environment))?;
            write_record(&file, &changed, invocation.format)?;
            Ok(Some(0))
        }
        TargetBootstrapCommand::Plan {
            access,
            cluster: args,
        } if cluster => {
            let progress = Progress::new(invocation.format);
            let request = request(invocation, &environment, args, access.host_binaries())?;
            let plan = services(invocation, true)?
                .service
                .plan_reporting(&request, &progress)
                .await?;
            let surface = progress.surface();
            progress.finish();
            show_plan(&plan, surface, invocation.format)?;
            Ok(Some(0))
        }
        TargetBootstrapCommand::Apply {
            plan,
            yes,
            access,
            cluster: args,
        } if cluster => {
            let request = request(invocation, &environment, args, access.host_binaries())?;
            apply(invocation, &request, plan, *yes).await.map(Some)
        }
        TargetBootstrapCommand::Plan { .. } | TargetBootstrapCommand::Apply { .. } => Ok(None),
    }
}

/// `steps`: the command runs setup's steps, which need Ansible here.
fn services(
    invocation: &super::dispatch::Invocation,
    steps: bool,
) -> Result<composition::ClusterSetupServices, MmError> {
    composition::cluster_setup_services_for(
        &invocation.start,
        invocation.home(),
        invocation.services.process(),
        Arc::clone(&invocation.clock),
        steps,
    )
}

fn request(
    invocation: &super::dispatch::Invocation,
    environment: &str,
    args: &ClusterSetupArgs,
    host_binaries: Option<std::path::PathBuf>,
) -> Result<crate::app::ClusterSetupRequest, MmError> {
    Ok(crate::app::ClusterSetupRequest {
        start: invocation.start.clone(),
        environment: Some(environment.to_owned()),
        trust_new_host_keys: args.trust_new_host_keys,
        binaries: host_binaries,
        access_commit: composition::pushed_commit(
            &invocation.start,
            environment,
            args.commit.as_deref(),
        )?,
        explicit_commit: args.commit.is_some(),
        nodes: args.nodes.clone(),
        downgrade: args.release.downgrade,
        upgrade: args.release.upgrade,
    })
}

async fn apply(
    invocation: &super::dispatch::Invocation,
    request: &crate::app::ClusterSetupRequest,
    plan: &str,
    yes: bool,
) -> Result<u8, MmError> {
    if invocation.format == OutputFormat::Json && !yes {
        return Err(MmError::Terminal {
            message: "JSON cluster setup requires --yes".to_owned(),
        });
    }
    let services = services(invocation, true)?;
    let preview = if yes {
        None
    } else {
        let progress = Progress::new(invocation.format);
        let preview = services.service.plan_reporting(request, &progress).await?;
        let surface = progress.surface();
        progress.finish();
        if preview.plan_hash != plan {
            return Err(changed_plan(plan, &preview.plan_hash));
        }
        if !surface.is_live() {
            write_plan(&preview, invocation.format)?;
        }
        if !super::terminal::confirm(&format!(
            "Apply cluster setup plan {} to {}?",
            output::terminal_safe(inline::short(plan)),
            inline::count(preview.nodes.len(), "node", "nodes")
        ))? {
            return Ok(1);
        }
        Some(preview)
    };
    let started = std::time::Instant::now();
    let progress = Progress::new(invocation.format);
    if let Some(preview) = &preview {
        for step in &preview.steps {
            let (running, _) = crate::app::cluster_setup_step_text(*step, preview);
            progress.plan(step.label(), &running);
        }
    }
    let result = services.service.apply(request, plan, &progress).await?;
    let surface = progress.surface();
    progress.finish();
    services.save_result(&result)?;
    inline::print_card(
        surface,
        || output::setup_result_card(&result, started.elapsed()),
        || write_result(&result, invocation.format),
    )?;
    Ok(0)
}

fn changed_plan(expected: &str, current: &str) -> MmError {
    MmError::State {
        message: format!(
            "cluster setup plan changed (expected {expected}, current {current}); review the new plan"
        ),
    }
}

/// The plan as a card on a live terminal, else as the text scripts read.
fn show_plan(
    plan: &ClusterSetupPlan,
    surface: inline::Surface,
    format: OutputFormat,
) -> Result<(), MmError> {
    inline::print_card(
        surface,
        || output::setup_plan_card(plan),
        || write_plan(plan, format),
    )
}

fn write_record(
    file: &std::path::Path,
    changed: &[String],
    format: OutputFormat,
) -> Result<(), MmError> {
    write_json_or_text(
        format,
        &serde_json::json!({"file": file, "changed-nodes": changed}),
        || {
            if changed.is_empty() {
                format!(
                    "{} already records every node's keys and address",
                    file.display()
                )
            } else {
                format!(
                    "Recorded keys and addresses for {} in {}. Review and commit it.",
                    changed.join(", "),
                    file.display()
                )
            }
        },
    )
}

fn write_deploy_key(
    environment: &str,
    public_key: &str,
    origin: &crate::app::DeployKeyOrigin,
    format: OutputFormat,
) -> Result<(), MmError> {
    use crate::app::DeployKeyOrigin;
    let copied_from = match origin {
        DeployKeyOrigin::Copied(from) => Some(from.as_str()),
        DeployKeyOrigin::Existing | DeployKeyOrigin::Created => None,
    };
    write_json_or_text(
        format,
        &serde_json::json!({
            "public-key": public_key,
            "created": *origin == DeployKeyOrigin::Created,
            "copied-from": copied_from,
        }),
        || match copied_from {
            Some(from) => format!(
                "Copied the target's deploy key from {from} into {environment}'s secrets:\n  {public_key}\n\nEvery environment on the target uses this one key, so the git host knows it already.\nNext: mm -e {environment} target setup plan"
            ),
            None => format!(
                "{} deploy key for {environment}:\n  {public_key}\n\nEvery environment on the target uses this one key. Add it to the git host as a read-only deploy key, for example:\n  gh repo deploy-key add - --title 'mm {environment}' <<<'{public_key}'\nand set setup.deploy-key.public-key to it.",
                if *origin == DeployKeyOrigin::Created {
                    "Created the"
                } else {
                    "The existing"
                }
            ),
        },
    )
}

fn write_plan(plan: &ClusterSetupPlan, format: OutputFormat) -> Result<(), MmError> {
    write_json_or_text(format, plan, || {
        let mut text = format!(
            "Cluster setup plan {}\n  project:     {} ({})\n  environment: {}\n  target:      {}\n  runtime:     {}\n  mesh:        {}\n  storage:     {}\n  access sync: {}\n\nNodes:\n",
            plan.plan_hash,
            plan.project_name,
            plan.project_id.0,
            plan.environment,
            plan.target,
            plan.runtime.name(),
            mesh_summary(&plan.cluster.mesh),
            storage_summary(plan),
            plan.access_commit,
        );
        for node in &plan.nodes {
            text.push_str(&format!(
                "  {:<20} {}:{}  private {}  {} ({})\n",
                node.name,
                node.host,
                node.port,
                node.private_address,
                node.host_key_fingerprint,
                match node.host_key_origin {
                    crate::domain::cluster_setup::HostKeyOrigin::FirstUse => "first use",
                    crate::domain::cluster_setup::HostKeyOrigin::Recorded => "recorded",
                    crate::domain::cluster_setup::HostKeyOrigin::Replaced => "REPLACED",
                }
            ));
            text.push_str(&format!("  {:<20} {}\n", "", node_binaries(plan, node)));
        }
        text.push_str(&binaries_text(&plan.binaries));
        if let Some(mesh) = &plan.mesh {
            text.push_str(&output::mesh_plan_text(mesh));
        }
        text.push_str(&format!("\nSecrets pushed: {}\n", plan.secrets.len()));
        if plan.pushes() {
            text.push_str(&format!(
                "Delivery:       {}\n",
                crate::domain::delivery::PUSH_SETUP_LINE
            ));
        } else {
            text.push_str(&format!(
                "Deploy key:     {} ({})\n",
                plan.deploy_key.secret, plan.deploy_key.public_key
            ));
        }
        if !plan.unset_secrets.is_empty() {
            text.push_str(&format!(
                "Unset here (not pushed): {}\n",
                plan.unset_secrets.join(", ")
            ));
        }
        for notice in &plan.notices {
            text.push_str(&format!("Note: {notice}\n"));
        }
        text.push_str("\nSteps: ");
        text.push_str(
            &plan
                .steps
                .iter()
                .map(|step| step.label())
                .collect::<Vec<_>>()
                .join(", "),
        );
        text.push_str(&format!(
            "\n\nApply with:\n  {} --yes\n",
            plan.apply_command()
        ));
        text
    })
}

fn write_result(result: &ClusterSetupResult, format: OutputFormat) -> Result<(), MmError> {
    write_json_or_text(format, result, || {
        let mut text = format!(
            "Set up {} for {} ({} secret(s) changed, users from {}).\n\n",
            result.target, result.environment, result.secrets_changed, result.access_commit
        );
        for node in &result.nodes {
            text.push_str(&format!(
                "  {:<20} private {}  mm {}  daemon {}\n",
                node.name, node.private_address, node.version, node.daemon_fingerprint
            ));
        }
        text.push_str(&format!(
            "\nWrite the nodes' keys and addresses to the repo with:\n  {}\n",
            result.record_command
        ));
        text
    })
}

/// Which node serves shared volumes, and where.
fn storage_summary(plan: &ClusterSetupPlan) -> String {
    if !plan
        .steps
        .contains(&crate::domain::cluster_setup::ClusterSetupStep::ServeStorage)
        && matches!(
            plan.cluster.mesh,
            crate::domain::repo_target::MeshConfig::Interface(_)
        )
    {
        return "not served (shared volumes need the WireGuard mesh mm creates)".to_owned();
    }
    let address = plan
        .nodes
        .iter()
        .find(|node| node.name == plan.storage)
        .map(|node| format!(", on its mesh address {} only", node.private_address))
        .unwrap_or_default();
    format!(
        "{} keeps shared volumes in {} and serves them to the other nodes{address}",
        plan.storage,
        crate::domain::shared_storage::STORAGE_ROOT
    )
}

fn mesh_summary(mesh: &crate::domain::repo_target::MeshConfig) -> String {
    use crate::domain::repo_target::{MESH_INTERFACE, MeshConfig};
    match mesh {
        MeshConfig::Wireguard(wireguard) => format!(
            "WireGuard {MESH_INTERFACE} owned by mmd, {} UDP port {} mtu {}",
            wireguard.subnet, wireguard.port, wireguard.mtu
        ),
        MeshConfig::Interface(interface) => format!("existing interface {interface}"),
    }
}

/// JSON as-is, or the text with each line made terminal-safe.
fn write_json_or_text<T: serde::Serialize>(
    format: OutputFormat,
    value: &T,
    text: impl FnOnce() -> String,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return output::write_json(value);
    }
    for line in text().lines() {
        crate::outln!("{}", output::terminal_safe(line));
    }
    Ok(())
}

/// Which `mm` and `mmd` a node gets, and where they come from.
fn node_binaries(
    plan: &ClusterSetupPlan,
    node: &crate::domain::cluster_setup::ClusterNodePlan,
) -> String {
    match (node.cpu.as_deref(), plan.binaries_for(node).as_slice()) {
        (Some(cpu), [set]) => format!("mm and mmd for {cpu}: {}", set.source),
        _ => format!(
            "mm and mmd: CPU not read yet, so setup stages {} and the host takes its own",
            plan.binaries
                .iter()
                .map(|set| set.architecture.as_str())
                .collect::<Vec<_>>()
                .join(" and ")
        ),
    }
}

/// Each set of host binaries with its source and digests.
pub(super) fn binaries_text(binaries: &[crate::domain::target::HostBinaries]) -> String {
    let mut text = String::from("\nHost binaries:\n");
    for set in binaries {
        text.push_str(&format!(
            "  {:<8} {}\n  {:<8} mm {}\n  {:<8} mmd {}\n",
            set.architecture, set.source, "", set.mm_digest, "", set.mmd_digest
        ));
    }
    text
}
