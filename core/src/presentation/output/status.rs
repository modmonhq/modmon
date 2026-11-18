//! `mm service status`: each service once, with its containers on every
//! node, the deployment they were started from, and what differs from it.

use super::*;
use crate::domain::rollout::utc_time;
use crate::domain::runtime::{ContainerObservation, ServiceOnNode, ServiceSnapshot};
use crate::domain::workspace::AppliedDeployment;

pub(crate) fn write_status(
    snapshot: &WorkspaceSnapshot,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(snapshot);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{} [{}] -> {}",
        terminal_safe(&snapshot.project_name),
        terminal_safe(&snapshot.environment),
        terminal_safe(&snapshot.target)
    )
    .map_err(output_error)?;
    if let (Some(kind), Some(source)) = (snapshot.runtime_kind, snapshot.runtime_source.as_deref())
    {
        writeln!(writer, "  selected:    {kind:?} (from {source})").map_err(output_error)?;
    }
    write_runtime(&mut writer, &snapshot.runtime)?;
    for line in deployment_lines(snapshot) {
        writeln!(writer, "  {line}").map_err(output_error)?;
    }
    writeln!(writer).map_err(output_error)?;
    for line in service_table(snapshot) {
        writeln!(writer, "  {}", line.trim_end()).map_err(output_error)?;
    }
    for line in super::scaling::scale_lines(snapshot) {
        writeln!(writer, "  {line}").map_err(output_error)?;
    }
    for line in super::restarts::restart_lines(snapshot) {
        writeln!(writer, "  {line}").map_err(output_error)?;
    }
    if let Some(names) = &snapshot.names {
        super::names::write_names(&mut writer, names)?;
    }
    for line in node_notes(snapshot) {
        writeln!(writer, "  {line}").map_err(output_error)?;
    }
    for unit in snapshot.optional.iter().filter(|unit| !unit.on) {
        writeln!(
            writer,
            "  off locally (optional): {}; turn it on: mm dev enable {}",
            terminal_safe(&unit.name),
            terminal_safe(&unit.name)
        )
        .map_err(output_error)?;
    }
    write_ingress_statuses_to(&mut writer, &snapshot.ingress)?;
    write_validation_report(&mut writer, &snapshot.validation)?;
    write_diagnostics(&mut writer, &snapshot.runtime.diagnostics)
}

/// The receipt of what runs: one line when every host runs the same
/// release, one per node when they differ.
pub(crate) fn deployment_lines(snapshot: &WorkspaceSnapshot) -> Vec<String> {
    if let Some(deployment) = &snapshot.deployment {
        return vec![format!("deployed:    {}", deployment_text(deployment))];
    }
    snapshot
        .nodes
        .iter()
        .filter(|node| node.reachable)
        .map(|node| {
            format!(
                "deployed:    {} on {}",
                node.deployment
                    .as_ref()
                    .map_or_else(|| "nothing yet".to_owned(), deployment_text),
                terminal_safe(&node.name)
            )
        })
        .collect()
}

fn short(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// `commit 3f2c9a1e5b7d  2026-11-14T05:11:02Z  by alice  plan 8a41c0de91f2`.
pub(crate) fn deployment_text(deployment: &AppliedDeployment) -> String {
    let mut text = format!(
        "commit {}  {}  by {}",
        terminal_safe(short(&deployment.commit)),
        utc_time(deployment.deployed_at_unix_ms),
        terminal_safe(&deployment.deployed_by),
    );
    if let Some(plan) = &deployment.plan_hash {
        text.push_str(&format!("  plan {}", terminal_safe(short(plan))));
    }
    if let Some(receipt) = &deployment.build_receipt {
        text.push_str(&format!(
            "  build receipt {}",
            terminal_safe(short(receipt))
        ));
    }
    text
}

/// One row of the service table: a service's first row names it, the rows
/// after it list its other containers.
struct Row {
    stack: String,
    service: String,
    state: String,
    node: String,
    container: String,
    health: String,
    status: String,
    image: String,
}

/// The table as lines: a header, then every service once with one row per
/// container, node by node on a cluster target.
pub fn service_table(snapshot: &WorkspaceSnapshot) -> Vec<String> {
    let cluster = !snapshot.nodes.is_empty();
    let mut rows = vec![Row {
        stack: "STACK".to_owned(),
        service: "SERVICE".to_owned(),
        state: "STATE".to_owned(),
        node: "NODE".to_owned(),
        container: "CONTAINER".to_owned(),
        health: "HEALTH".to_owned(),
        status: "STATUS".to_owned(),
        image: "IMAGE".to_owned(),
    }];
    // The nodes whose containers nobody could list.
    let silent = snapshot
        .nodes
        .iter()
        .filter(|node| !node.reachable)
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>();
    for stack in &snapshot.stacks {
        for service in &stack.services {
            rows.extend(service_rows(&stack.name, service, &silent));
        }
    }
    if rows.len() == 1 {
        return vec![
            "STACK  SERVICE  STATE".to_owned(),
            "no configured services found".to_owned(),
        ];
    }
    let width = |column: fn(&Row) -> &String| {
        rows.iter()
            .map(|row| column(row).chars().count())
            .max()
            .unwrap_or(0)
    };
    let (stack, service, state, node, container, health, status) = (
        width(|row| &row.stack),
        width(|row| &row.service),
        width(|row| &row.state),
        width(|row| &row.node),
        width(|row| &row.container),
        width(|row| &row.health),
        width(|row| &row.status),
    );
    rows.iter()
        .map(|row| {
            let node_column = if cluster {
                format!("{:<node$}  ", row.node)
            } else {
                String::new()
            };
            format!(
                "{:<stack$}  {:<service$}  {:<state$}  {node_column}{:<container$}  {:<health$}  {:<status$}  {}",
                row.stack, row.service, row.state, row.container, row.health, row.status, row.image
            )
        })
        .collect()
}

/// The word and the count under STATE: `running 3/3`, `starting 2/3`. The
/// count is the containers that run and pass their health check, of the
/// ones the service should have.
pub(crate) fn service_state_text(service: &ServiceSnapshot) -> String {
    if service.state == ServiceState::JobOnly {
        return service_state(service.state).to_owned();
    }
    format!(
        "{} {}/{}",
        service_state_word(service),
        service.healthy_count,
        service.desired_count
    )
}

/// The word under STATE: the service's state, or `starting` while it is
/// only still coming up.
pub(crate) fn service_state_word(service: &ServiceSnapshot) -> &'static str {
    if service.starting {
        "starting"
    } else {
        service_state(service.state)
    }
}

/// The same word and count for one node's share of a service.
pub(crate) fn node_state_text(share: &ServiceOnNode) -> String {
    if share.state == ServiceState::JobOnly {
        return service_state(share.state).to_owned();
    }
    let word = if share.starting {
        "starting"
    } else {
        service_state(share.state)
    };
    format!("{word} {}/{}", share.healthy_count, share.desired_count)
}

/// What stands in place of a container on a node that has none of the
/// service: that it has none, or that the node did not say.
pub(crate) const fn no_container_text(answered: bool) -> &'static str {
    if answered {
        "no container"
    } else {
        "no answer"
    }
}

/// A service's rows: its current containers, node by node; a node where
/// it is placed and has none says so, and a node in `silent`, which did
/// not answer, says that.
fn service_rows(stack: &str, service: &ServiceSnapshot, silent: &[&str]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut add = |node: &str, container: Option<&ContainerObservation>| {
        let nothing = no_container_text(!silent.contains(&node));
        let first = rows.is_empty();
        rows.push(Row {
            stack: if first {
                terminal_safe(stack)
            } else {
                String::new()
            },
            service: if first {
                terminal_safe(&service.name)
            } else {
                String::new()
            },
            state: if first {
                service_state_text(service)
            } else {
                String::new()
            },
            node: terminal_safe(node),
            container: container
                .map_or_else(|| "-".to_owned(), |found| container_name(service, found)),
            health: container.map_or_else(|| "-".to_owned(), container_health),
            status: container.map_or_else(|| nothing.to_owned(), container_status),
            image: container.map_or_else(|| "-".to_owned(), |found| terminal_safe(&found.image)),
        });
    };
    // One host has no node names; a cluster lists its nodes in order.
    let nodes = if service.nodes.is_empty() {
        vec![None]
    } else {
        service
            .nodes
            .iter()
            .map(|share| Some(share.node.as_str()))
            .collect()
    };
    for node in nodes {
        let current = current_containers_on(service, node);
        if current.is_empty() {
            add(node.unwrap_or_default(), None);
        }
        for container in current {
            add(node.unwrap_or_default(), Some(container));
        }
    }
    rows
}

/// The service's containers on `node` that count: not the ones a newer
/// container replaced.
pub(crate) fn current_containers_on<'a>(
    service: &'a ServiceSnapshot,
    node: Option<&str>,
) -> Vec<&'a ContainerObservation> {
    service
        .containers
        .iter()
        .filter(|container| !container.history && container.node.as_deref() == node)
        .collect()
}

/// `web-1` for the first container of `web`, as Compose names it in its
/// own output; the engine's name when it does not follow that pattern.
pub(crate) fn container_name(
    service: &ServiceSnapshot,
    container: &ContainerObservation,
) -> String {
    let name = container.name.as_str();
    let short = name
        .rmatch_indices(service.name.as_str())
        .map(|(start, _)| &name[start..])
        .find(|label| {
            label[service.name.len()..]
                .strip_prefix(['-', '_'])
                .is_some_and(|number| {
                    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
                })
        })
        .unwrap_or(name);
    terminal_safe(short)
}

/// The health check's verdict, or the engine's state for a container
/// that has no health check or does not run.
pub(crate) fn container_health(container: &ContainerObservation) -> String {
    if !container.is_running() {
        return terminal_safe(&container.state);
    }
    if container.is_leaving() {
        return "leaving".to_owned();
    }
    terminal_safe(container.health.as_deref().unwrap_or("running"))
}

/// How long the container has been up, in the engine's words (`Up 2
/// hours`), or why it is not: `Exited (1) 3 minutes ago`, `waits for
/// shared storage`.
pub(crate) fn container_status(container: &ContainerObservation) -> String {
    if container.waits_for_shared_storage() {
        return "waits for shared storage".to_owned();
    }
    // The health is a column of its own.
    let status = container
        .status
        .split(" (")
        .next()
        .filter(|_| container.is_running())
        .unwrap_or(&container.status);
    let mut text = terminal_safe(status.trim());
    if text.is_empty() {
        text = terminal_safe(&container.state);
    }
    match container.restart_count {
        Some(1) => text.push_str(", 1 restart"),
        Some(count) if count > 1 => text.push_str(&format!(", {count} restarts")),
        _ => {}
    }
    text
}

/// What a cluster status says about its nodes under the service rows: why
/// a node did not answer, and what differs on a node from the deployment
/// it last applied.
pub(crate) fn node_notes(snapshot: &WorkspaceSnapshot) -> Vec<String> {
    let mut notes = Vec::new();
    for node in snapshot.nodes.iter().filter(|node| !node.reachable) {
        notes.push(format!(
            "node {}: {}",
            terminal_safe(&node.name),
            terminal_safe(node.problem.as_deref().unwrap_or("no reason given"))
        ));
    }
    let fix = format!(
        "`mm deploy plan -e {}` shows the repair and applying it puts the node right",
        terminal_safe(&snapshot.environment)
    );
    for node in &snapshot.nodes {
        if !node.drift.is_empty() {
            notes.push(format!(
                "node {}: differs from its last deployment: {}. {fix}",
                terminal_safe(&node.name),
                terminal_safe(&node.drift.join("; ")),
            ));
        }
        if !node.network_drift.is_empty() {
            notes.push(format!(
                "node {}: cluster networks differ from the last deployment: {}. {fix}",
                terminal_safe(&node.name),
                terminal_safe(&node.network_drift.join("; ")),
            ));
        }
        if let Some(tunnel) = &node.tunnel {
            notes.push(format!(
                "node {}: the tunnel connector {}",
                terminal_safe(&node.name),
                match tunnel.connections {
                    None => "does not answer; `mm doctor` says what to do".to_owned(),
                    Some(0) =>
                        "holds no connection to Cloudflare; `mm doctor` says what to do".to_owned(),
                    Some(1) => "holds 1 connection to Cloudflare".to_owned(),
                    Some(count) => format!("holds {count} connections to Cloudflare"),
                }
            ));
        }
    }
    // A single remote host reports its own.
    if !snapshot.drift.is_empty() {
        notes.push(format!(
            "differs from the last deployment: {}. {fix}",
            terminal_safe(&snapshot.drift.join("; ")),
        ));
    }
    notes
}
