//! Plans, apply reports, and node lists for a cluster target.

use super::*;
use crate::domain::cluster_network::AddressTable;
use crate::domain::remote::{
    ClusterAccessPlan, ClusterApplyReport, ClusterDeploymentPlan, NodeApplyState, NodeReport,
    NodeState, RemotePlanBinding,
};
use crate::domain::rollout::utc_time;
use crate::domain::users::AccessChange;

fn write_plan_header(
    writer: &mut impl Write,
    title: &str,
    (plan_hash, target, commit, expires_at_unix_ms): (&str, &str, &str, i64),
    nodes: &[&str],
    skipped: &[String],
) -> Result<(), MmError> {
    writeln!(writer, "{title} {}", terminal_safe(plan_hash)).map_err(output_error)?;
    writeln!(
        writer,
        "  target:   {} ({})",
        terminal_safe(target),
        terminal_safe(&nodes.join(", "))
    )
    .map_err(output_error)?;
    writeln!(writer, "  commit:   {}", terminal_safe(commit)).map_err(output_error)?;
    writeln!(writer, "  expires:  {}", utc_time(expires_at_unix_ms)).map_err(output_error)?;
    if !skipped.is_empty() {
        writeln!(
            writer,
            "  left out: {} ({} now)",
            terminal_safe(&skipped.join(", ")),
            if skipped.len() == 1 {
                "it keeps what it runs"
            } else {
                "they keep what they run"
            }
        )
        .map_err(output_error)?;
    }
    Ok(())
}

/// The bundle a push target's nodes planned from, and who was sent it.
fn bundle_line(bundle: &crate::domain::delivery::PlanBundle) -> String {
    let sent = if bundle.sent_to.is_empty() {
        "every node already held it".to_owned()
    } else {
        format!("sent to {}", bundle.sent_to.join(", "))
    };
    format!(
        "  bundle:   {} ({} files, {}; {sent})",
        bundle.bundle_hash,
        bundle.files,
        crate::domain::delivery::human_size(bundle.bytes)
    )
}

fn write_node_header(
    writer: &mut impl Write,
    node: &str,
    binding: &RemotePlanBinding,
) -> Result<(), MmError> {
    writeln!(
        writer,
        "\nNode {}  mmd {}  section {}",
        terminal_safe(node),
        terminal_safe(&binding.daemon_version),
        terminal_safe(&binding.plan_hash)
    )
    .map_err(output_error)
}

/// One plan for a cluster: a section per node, then the hash that applies
/// them all.
pub(crate) fn write_cluster_deployment_plan(
    plan: &ClusterDeploymentPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    cluster_deployment_plan_text(&mut writer, plan)?;
    writeln!(
        writer,
        "\nApply every section within ten minutes with `mm --environment {} deploy apply --plan {} --yes`.",
        terminal_safe(&plan.environment),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

/// The text `mm deploy plan` prints for a cluster, up to how to apply it:
/// the header, then each node's section.
pub(crate) fn cluster_deployment_plan_text(
    writer: &mut impl Write,
    plan: &ClusterDeploymentPlan,
) -> Result<(), MmError> {
    let names = plan
        .nodes
        .iter()
        .map(|section| section.node.as_deref().unwrap_or(&plan.target))
        .collect::<Vec<_>>();
    write_plan_header(
        writer,
        "Cluster deployment plan",
        (
            &plan.plan_hash,
            &plan.target,
            &plan.commit,
            plan.expires_at_unix_ms,
        ),
        &names,
        &plan.skipped,
    )?;
    if let Some(bundle) = &plan.bundle {
        writeln!(writer, "{}", terminal_safe(&bundle_line(bundle))).map_err(output_error)?;
    }
    if !plan.moves.is_empty() {
        writeln!(writer, "  moves:").map_err(output_error)?;
        for line in super::placement::move_lines(&plan.moves) {
            writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
        }
    }
    if let Some(networks) = &plan.networks {
        writeln!(
            writer,
            "{}",
            terminal_safe(&super::networks::network_header_line(networks))
        )
        .map_err(output_error)?;
        writeln!(writer, "{}", super::names::names_header_line()).map_err(output_error)?;
    }
    let storage = plan
        .storage
        .as_ref()
        .map(super::storage::storage_plan_lines)
        .unwrap_or_default();
    let ingress = plan.ingress.as_ref().map(ingress_lines).unwrap_or_default();
    for line in storage
        .into_iter()
        .chain(ingress)
        .chain(super::rollout::plan_lines(plan))
    {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    for (name, section) in names.iter().zip(&plan.nodes) {
        write_node_header(writer, name, &section.plan.binding)?;
        write_receipt_lines(writer, section.plan.receipt.as_ref())?;
        write_access_change_lines(writer, &section.plan.access)?;
        if section.plan.plan.is_none() {
            writeln!(writer, "  runs nothing: no service is placed on this node")
                .map_err(output_error)?;
        }
        super::deployment::write_staged_lines(writer, &section.plan.staged)?;
        let project = section
            .plan
            .plan
            .as_ref()
            .and_then(|plan| plan.node.as_ref());
        let networks = plan
            .networks
            .iter()
            .flat_map(|networks| &networks.nodes)
            .find(|node| node.node == *name)
            .map(super::networks::node_network_lines)
            .unwrap_or_default();
        for line in project
            .map(super::placement::node_project_lines)
            .unwrap_or_default()
            .into_iter()
            .chain(networks)
        {
            writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
        }
        if let Some(deployment) = &section.plan.plan {
            deployment_plan_text(writer, deployment)?;
        }
    }
    Ok(())
}

/// What `mm deploy plan` prints for a remote environment, as lines: the
/// cluster's plan, or the one host's. The terminal UI shows these in its
/// review, so a line added to the plan shows there too.
#[must_use]
pub fn remote_plan_lines(plan: &ClusterDeploymentPlan, cluster: bool) -> Vec<String> {
    let mut text = Vec::new();
    let written = if cluster {
        cluster_deployment_plan_text(&mut text, plan)
    } else {
        match plan.nodes.first() {
            Some(host) => remote_deployment_plan_text(&mut text, &host.plan),
            None => Ok(()),
        }
    };
    if let Err(error) = written {
        tracing::debug!(%error, "could not write a plan for the review");
    }
    plan_text_lines(&text)
}

/// What `mm deploy plan` prints for an environment on this machine, as lines.
#[must_use]
pub fn local_plan_lines(plan: &DeploymentPlan) -> Vec<String> {
    let mut text = Vec::new();
    if let Err(error) = deployment_plan_text(&mut text, plan) {
        tracing::debug!(%error, "could not write a plan for the review");
    }
    plan_text_lines(&text)
}

fn plan_text_lines(text: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(text)
        .lines()
        .map(str::to_owned)
        .collect()
}

pub(crate) fn write_cluster_access_plan(
    plan: &ClusterAccessPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let names = plan
        .nodes
        .iter()
        .map(|section| section.node.as_deref().unwrap_or(&plan.target))
        .collect::<Vec<_>>();
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    write_plan_header(
        &mut writer,
        "Cluster team sync plan",
        (
            &plan.plan_hash,
            &plan.target,
            &plan.commit,
            plan.expires_at_unix_ms,
        ),
        &names,
        &plan.skipped,
    )?;
    for (name, section) in names.iter().zip(&plan.nodes) {
        write_node_header(&mut writer, name, &section.plan.binding)?;
        write_access_change_lines(&mut writer, &section.plan.access)?;
    }
    writeln!(
        writer,
        "\nApply on every node within ten minutes with `mm --environment {} team sync --plan {} --yes`.",
        terminal_safe(&plan.environment),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

/// What each node did, in apply order. Printed whether or not the apply
/// went through, so a partial apply is exact about what changed.
fn write_apply_report<T: Serialize>(
    report: &ClusterApplyReport<T>,
    format: OutputFormat,
    title: &str,
    describe: impl Fn(Option<&T>) -> String,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{title} {} on {} [{}]",
        terminal_safe(&report.plan_hash),
        terminal_safe(&report.target),
        terminal_safe(&report.environment)
    )
    .map_err(output_error)?;
    for node in &report.nodes {
        let stopped = if node.stopped.is_empty() {
            String::new()
        } else {
            format!(
                "; then stopped, now that they run elsewhere: {}",
                node.stopped
                    .iter()
                    .map(|service| format!("{}/{}", service.stack, service.service))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let detail = match node.state {
            NodeApplyState::Applied => {
                format!("applied      {}{stopped}", describe(node.result.as_ref()))
            }
            NodeApplyState::Failed => format!(
                "FAILED       {}{}",
                node.error.as_deref().unwrap_or("no reason given"),
                if node.rolled_back {
                    "; rolled back to what it ran before"
                } else if node.unchanged {
                    "; no container changed here"
                } else {
                    ""
                }
            ),
            NodeApplyState::RolledBack => "rolled back  runs what it ran before".to_owned(),
            NodeApplyState::Unfinished => format!(
                "unfinished   changed and left as it is{}",
                node.error
                    .as_deref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            ),
            NodeApplyState::NotStarted => format!(
                "not started  unchanged{}",
                node.error
                    .as_deref()
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            ),
        };
        writeln!(
            writer,
            "  {:<16} {}",
            terminal_safe(&node.node),
            terminal_safe(&detail)
        )
        .map_err(output_error)?;
        if !node.log.is_empty() {
            writeln!(
                writer,
                "  {:<16} last log lines of the failed container:",
                ""
            )
            .map_err(output_error)?;
            for line in &node.log {
                writeln!(writer, "  {:<16}   {}", "", terminal_safe(line)).map_err(output_error)?;
            }
        }
    }
    for node in &report.skipped {
        writeln!(
            writer,
            "  {:<16} left out     unchanged",
            terminal_safe(node)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_cluster_deployment_report(
    report: &ClusterApplyReport<DeploymentResult>,
    format: OutputFormat,
) -> Result<(), MmError> {
    write_apply_report(report, format, "Deployment", deployment_outcome)
}

/// What a node's deployment did, in one line.
pub(crate) fn deployment_outcome(result: Option<&DeploymentResult>) -> String {
    match result {
        None => "runs nothing".to_owned(),
        // A cluster node claims no route itself: the ingress nodes serve
        // them, and the plan lists them.
        Some(result) => format!(
            "stacks: {}{}{}",
            if result.applied_stacks.is_empty() {
                "none".to_owned()
            } else {
                result.applied_stacks.join(", ")
            },
            if result.removed_stacks.is_empty() {
                String::new()
            } else {
                format!("; removed: {}", result.removed_stacks.join(", "))
            },
            if result.route_count == 0 {
                String::new()
            } else {
                format!("; {} route(s)", result.route_count)
            }
        ),
    }
}

pub(crate) fn write_cluster_access_report(
    report: &ClusterApplyReport<AccessChange>,
    format: OutputFormat,
) -> Result<(), MmError> {
    write_apply_report(report, format, "Team sync", access_outcome)
}

/// What a node's access sync did, in one line.
pub(crate) fn access_outcome(change: Option<&AccessChange>) -> String {
    change.map_or_else(String::new, |change| {
        format!(
            "users file of {}: {} user(s), {} change(s)",
            change
                .record
                .commit
                .get(..12)
                .unwrap_or(&change.record.commit),
            change.record.users.len(),
            change.entries.len()
        )
    })
}

/// `mm target nodes`: every node, reachable or not, and why not; then the
/// addresses of the networks that span the nodes, when the target has any.
pub(crate) fn write_node_reports(
    target: &str,
    nodes: &[NodeReport],
    addresses: &AddressTable,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({
            "target": target,
            "nodes": nodes,
            "addresses": addresses,
        }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Target {}: {} node(s)",
        terminal_safe(target),
        nodes.len()
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  {:<16} {:<24} {:<15} {:<12} {:<10} DEPLOYED",
        "NODE", "ADDRESS", "MESH", "STATE", "MMD"
    )
    .map_err(output_error)?;
    for node in nodes {
        let deployed = node.deployed.as_ref().map_or_else(
            || "nothing yet".to_owned(),
            |deployed| {
                format!(
                    "{} at {}{}",
                    deployed.commit.get(..12).unwrap_or(&deployed.commit),
                    utc_time(deployed.deployed_at_unix_ms),
                    if deployed.runs_services {
                        ""
                    } else {
                        " (runs nothing)"
                    }
                )
            },
        );
        writeln!(
            writer,
            "  {:<16} {:<24} {:<15} {:<12} {:<10} {}",
            terminal_safe(&node.name),
            terminal_safe(&crate::domain::target::host_and_port(&node.host, node.port)),
            node.mesh_address
                .map_or_else(|| "-".to_owned(), |address| address.to_string()),
            match node.state {
                NodeState::Ready => "ready",
                NodeState::Unreachable => "unreachable",
                NodeState::Refused => "refused",
                NodeState::NotSetUp => "not set up",
                NodeState::WrongNode => "wrong node",
            },
            terminal_safe(node.mmd_version.as_deref().unwrap_or("-")),
            terminal_safe(if node.state == NodeState::Ready {
                &deployed
            } else {
                "-"
            })
        )
        .map_err(output_error)?;
        if let Some(repo) = &node.repo_address {
            writeln!(
                writer,
                "      this device's address; targets.yaml says {}",
                terminal_safe(repo)
            )
            .map_err(output_error)?;
        }
        if let Some(problem) = &node.problem {
            writeln!(writer, "      {}", terminal_safe(problem)).map_err(output_error)?;
        }
        if let Some(rollout) = node.rollout_summary() {
            writeln!(
                writer,
                "      {rollout}; `mm deploy status` shows where it is"
            )
            .map_err(output_error)?;
        }
    }
    for line in super::address_table::address_table_lines(addresses, nodes) {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    let members = nodes
        .iter()
        .filter_map(|node| Some((node.name.as_str(), node.secret_store.as_ref()?)))
        .collect::<Vec<_>>();
    for line in super::secret_store::store_lines(&members) {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    Ok(())
}

/// Where a tunnel's traffic comes in: the tunnel, and the hosts each
/// node's connector sends to its proxy.
fn tunnel_lines(
    ingress: &crate::domain::cluster_ingress::SealedIngress,
    tunnel: &crate::domain::cluster_ingress::SealedTunnel,
    reach: &str,
) -> Vec<String> {
    let one = ingress.nodes.len() == 1;
    let hosts = crate::domain::cluster_ingress::tunnel_hosts(&ingress.routes).join(", ");
    let mut lines = vec![format!(
        "  ingress: Cloudflare Tunnel {}, token in secret {}; {} {}, open no port, and send each request to a healthy container {reach}",
        tunnel.id,
        tunnel.token,
        ingress.nodes.join(", "),
        if one {
            "runs a connector"
        } else {
            "run a connector each"
        },
    )];
    for node in &ingress.nodes {
        let mut line = format!(
            "    {node}: the connector sends {hosts} to this node's proxy and answers 404 to any other host"
        );
        if let Some(open) = tunnel.open_ports.get(node) {
            line.push_str(&format!(
                "; ports 80 and 443 stay open here for {} (public ingress)",
                open.join(", ")
            ));
        }
        lines.push(line);
    }
    lines
}

/// The routes of a cluster plan and the nodes that take their traffic.
fn ingress_lines(ingress: &crate::domain::cluster_ingress::SealedIngress) -> Vec<String> {
    if ingress.routes.is_empty() {
        return Vec::new();
    }
    let certificate = match &ingress.certificate {
        Some(tls) => format!(
            "the certificate in secrets {} and {}",
            tls.certificate, tls.key
        ),
        None => "a certificate from ACME (Let's Encrypt) for each HTTPS host".to_owned(),
    };
    let spread = ingress
        .routes
        .iter()
        .filter(|route| !route.balance.is_local_first())
        .count();
    let reach = match spread {
        0 => "on the same node, else on any node",
        all if all == ingress.routes.len() => "on any node",
        _ => "on the same node, else on any node, or on any node where a route spreads",
    };
    let one = ingress.nodes.len() == 1;
    let mut lines = match &ingress.tunnel {
        Some(tunnel) => tunnel_lines(ingress, tunnel, reach),
        None => vec![format!(
            "  ingress: {} {} the traffic on ports 80 and 443 and {} each request to a healthy container {reach}; HTTPS uses {certificate}",
            ingress.nodes.join(", "),
            if one { "takes" } else { "take" },
            if one { "sends" } else { "send" },
        )],
    };
    lines.extend(
        crate::domain::cluster_ingress::route_lines(ingress)
            .into_iter()
            .map(|line| format!("    {line}")),
    );
    lines
}
