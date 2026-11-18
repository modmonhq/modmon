//! What a cluster target adds to the views: the nodes beneath a service in
//! the Services table, which the selection rests on like a service's
//! containers on one host, where a service runs in its details, and the
//! release the environment runs.

use super::*;
use crate::domain::runtime::{ContainerObservation, ServiceSnapshot};
use crate::presentation::output::{
    container_health, container_name, container_status, deployment_lines, no_container_text,
    node_state_text,
};
use crate::presentation::tui::service_nodes::{NodeRow, node_rows, only_node};

/// What follows a service's name in its row: the node of a service that runs
/// as one container on one node, how many nodes are listed beneath it, or on
/// a single host its module.
pub(super) fn after_name(service: &ServiceSnapshot) -> String {
    let label = match only_node(service) {
        Some(node) => Some(terminal_safe(node)),
        None if service.nodes.is_empty() => service.module.as_deref().map(terminal_safe),
        None => Some(plural(service.nodes.len(), "node")),
    };
    label.map_or_else(String::new, |label| format!("  {label}"))
}

/// The width of the widest node name among the services, so the rows
/// beneath every service start their containers in the same column.
pub(super) fn node_name_width(model: &Model) -> usize {
    model
        .services()
        .iter()
        .flat_map(|service| &service.nodes)
        .map(|share| terminal_safe(&share.node).chars().count())
        .max()
        .unwrap_or(0)
}

/// How a container is doing, as a tone. One that runs and is healthy is
/// calm: the row beneath a service is quiet until something is wrong.
fn health_tone(container: &ContainerObservation) -> Tone {
    match (container.is_running(), container.health.as_deref()) {
        (true, None) => Tone::Idle,
        (true, Some(health)) if health.eq_ignore_ascii_case("healthy") => Tone::Idle,
        (true, Some(_)) => Tone::Warn,
        (false, _) if container.waits_for_shared_storage() => Tone::Warn,
        (false, _) => Tone::Bad,
    }
}

/// Whether the node said what it runs of the service: it was reached, and
/// its share of the service is known.
fn answered(model: &Model, service: &ServiceSnapshot, node: &str) -> bool {
    let reached = model
        .snapshot
        .nodes
        .iter()
        .find(|status| status.name == node)
        .is_none_or(|status| status.reachable);
    let known = service
        .nodes
        .iter()
        .find(|share| share.node == node)
        .is_none_or(|share| share.state != ServiceState::Unknown);
    reached && known
}

/// One row beneath a service: the node, then its container there in the
/// words of `mm service status`, or that it has none, or that the node
/// did not answer.
fn node_line(
    model: &Model,
    service: &ServiceSnapshot,
    row: &NodeRow<'_>,
    widths: (usize, usize, usize),
) -> Line<'static> {
    let theme = theme();
    let (node, name, health) = widths;
    let mut spans = vec![
        Span::styled(if row.last { "  └ " } else { "  ├ " }, theme.border()),
        Span::styled(format!("{:<node$}", terminal_safe(row.node)), theme.soft()),
    ];
    match row.container {
        Some(container) => {
            spans.push(Span::styled(
                format!("  {:<name$}", container_name(service, container)),
                theme.muted(),
            ));
            spans.push(Span::styled(
                format!("  {:<health$}", container_health(container)),
                theme.tone(health_tone(container)),
            ));
            spans.push(Span::styled(
                format!("  {}", container_status(container)),
                theme.muted(),
            ));
        }
        None => spans.push(Span::styled(
            format!("  {}", missing_container_text(model, service, row.node)),
            theme.muted(),
        )),
    }
    Line::from(spans)
}

/// The cells of one row beneath a service on a cluster target: the node
/// and its container there, spanning the `span` columns after the state.
/// The container columns line up across the service's rows.
pub(super) fn node_cells(
    model: &Model,
    service: &ServiceSnapshot,
    row: &NodeRow<'_>,
    node_width: usize,
    span: u16,
) -> Vec<Cell<'static>> {
    let rows = node_rows(service);
    let widest = |text: fn(&ServiceSnapshot, &ContainerObservation) -> String| {
        rows.iter()
            .filter_map(|row| row.container)
            .map(|container| text(service, container).chars().count())
            .max()
            .unwrap_or(0)
    };
    let widths = (
        node_width,
        widest(container_name),
        widest(|_: &ServiceSnapshot, container: &ContainerObservation| container_health(container)),
    );
    vec![
        Cell::from(""),
        Cell::from(node_line(model, service, row, widths)).column_span(span),
    ]
}

/// What a node row without a container says: that the node has not
/// answered yet, did not answer, or has no container of the service.
pub(super) fn missing_container_text(
    model: &Model,
    service: &ServiceSnapshot,
    node: &str,
) -> String {
    let pending = model
        .snapshot
        .nodes
        .iter()
        .any(|status| status.name == node && status.is_pending());
    if pending {
        "not answered yet".to_owned()
    } else {
        no_container_text(answered(model, service, node)).to_owned()
    }
}

/// Where the service runs, for its details: each node it is placed on and
/// how it is doing there. Nothing on a single host.
pub(super) fn placement_lines(service: &ServiceSnapshot, width: usize) -> Vec<Line<'static>> {
    let node = service
        .nodes
        .iter()
        .map(|share| terminal_safe(&share.node).chars().count())
        .max()
        .unwrap_or(0);
    let label = if service.nodes.len() == 1 {
        "Node"
    } else {
        "Nodes"
    };
    service
        .nodes
        .iter()
        .enumerate()
        .map(|(index, share)| {
            let mut value = vec![Span::styled(
                format!("{:<node$}  ", terminal_safe(&share.node)),
                theme().text(),
            )];
            value.extend(badge(service_tone(share.state), node_state_text(share)));
            kv(if index == 0 { label } else { "" }, value, width)
        })
        .collect()
}

/// What a host says differs from its last deployment about this service,
/// for its details. The findings list the same for every service.
pub(super) fn drift_lines(
    model: &Model,
    service: &ServiceSnapshot,
    width: usize,
) -> Vec<Line<'static>> {
    let named = format!("service '{}'", service.name);
    let on_nodes = model.snapshot.nodes.iter().flat_map(|node| {
        node.drift
            .iter()
            .map(move |note| (note, format!("{}: {note}", node.name)))
    });
    // A single remote host reports its own.
    let on_host = model.snapshot.drift.iter().map(|note| (note, note.clone()));
    on_nodes
        .chain(on_host)
        .filter(|(note, _)| note.contains(&named))
        .map(|(_, text)| {
            kv(
                "Drift",
                vec![Span::styled(terminal_safe(&text), theme().tone(Tone::Warn))],
                width,
            )
        })
        .collect()
}

/// A labelled value too long for one line, broken between its parts (which
/// are two spaces apart), with the following lines under the value.
fn kv_parts(label: &str, text: &str, width: usize, available: usize) -> Vec<Line<'static>> {
    let room = available.saturating_sub(width).max(1);
    let mut rows: Vec<String> = Vec::new();
    for part in text
        .split("  ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        match rows.last_mut() {
            Some(row) if row.chars().count() + 2 + part.chars().count() <= room => {
                row.push_str("  ");
                row.push_str(part);
            }
            _ => rows.push(part.to_owned()),
        }
    }
    rows.iter()
        .enumerate()
        .map(|(index, row)| kv_text(if index == 0 { label } else { "" }, row, width))
        .collect()
}

/// The release the environment runs, in the words of `mm service status`:
/// one entry when every host runs the same, one for each node when they
/// differ. Nothing for an environment that keeps no deployment.
pub(super) fn deployed_lines(model: &Model, width: usize, available: usize) -> Vec<Line<'static>> {
    deployment_lines(&model.snapshot)
        .iter()
        .enumerate()
        .flat_map(|(index, line)| {
            let release = line.strip_prefix("deployed:").unwrap_or(line);
            kv_parts(
                if index == 0 { "Deployed" } else { "" },
                release,
                width,
                available,
            )
        })
        .collect()
}
