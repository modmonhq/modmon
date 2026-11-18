use super::*;
use crate::domain::runtime::{ContainerObservation, ServiceSnapshot};
use crate::presentation::output::service_state_word;
use crate::presentation::tui::model::ServiceRow;
use crate::presentation::tui::ops::ServiceTarget;
use crate::presentation::tui::panel::ServiceAction;
use crate::presentation::tui::service_nodes::{self, NodeRow};

/// Content width at which the selected service's details open beside the table.
const DETAIL_BREAKPOINT: u16 = 110;

pub(super) fn render_services(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let [control, services] =
        Layout::vertical([Constraint::Length(4), Constraint::Min(0)]).areas(area);
    if model.remote().is_some() {
        render_target(frame, control, model);
    } else {
        render_control_plane(frame, control, model);
    }
    let rows = model.service_rows();
    let selected = rows.get(model.scroll).copied();
    let anchor = if services.width >= DETAIL_BREAKPOINT
        && let Some(row) = selected
    {
        let [table, _, detail] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(42),
        ])
        .areas(services);
        let anchor = render_service_table(frame, table, model, &rows);
        render_service_detail(frame, detail, model, row);
        anchor
    } else {
        render_service_table(frame, services, model, &rows)
    };
    // Drawn last so the menu lies over the table and the details.
    if let Some(menu) = &model.menu {
        render_menu(frame, menu, anchor);
    }
}

/// Where a remote environment runs and what the host lets you do there. It
/// replaces the control plane card, which describes this machine.
fn render_target(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let access = &model.access;
    let mut place = badge(
        Tone::Info,
        format!("remote · {}", terminal_safe(model.remote().unwrap_or(""))),
    );
    place.push(Span::styled(
        format!("   {}", runtime_label(model.snapshot.runtime.kind)),
        theme.muted(),
    ));
    let mut identity = Vec::new();
    match (&access.identity, &access.identity_error) {
        (Some(who), _) => {
            identity.push(Span::styled("you are ", theme.muted()));
            identity.push(Span::styled(terminal_safe(&who.principal), theme.strong()));
            identity.push(Span::styled(
                format!(
                    "  {}",
                    terminal_safe(
                        &who.access
                            .as_ref()
                            .map_or_else(|| "no access here".to_owned(), |access| access.summary())
                    )
                ),
                theme.text(),
            ));
            identity.push(Span::styled(
                format!(
                    "   deployed {}",
                    who.deployed_commit
                        .as_deref()
                        .map_or("nothing yet", short_commit)
                ),
                theme.muted(),
            ));
        }
        (None, Some(error)) => {
            identity.extend(badge(Tone::Warn, format!("host: {}", terminal_safe(error))))
        }
        (None, None) => identity.push(Span::styled(
            "the host has not said who you are yet",
            theme.muted(),
        )),
    }
    let access_key = Tab::Access.digit().map(String::from).unwrap_or_default();
    frame.render_widget(
        Paragraph::new(vec![Line::from(place), Line::from(identity)]).block(with_meta(
            card("Target", false),
            hints(&[(access_key.as_str(), "access")]),
        )),
        area,
    );
}

fn render_control_plane(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let part = crate::presentation::tui::loading::Part::SharedServices;
    if model.loading.is_pending(part) {
        frame.render_widget(
            Paragraph::new(reading_line(model, part)).block(card("Control plane", false)),
            area,
        );
        return;
    }
    let services = &model.installation_services;
    let mut components = Vec::new();
    for component in &services.components {
        let tone = if component.healthy {
            Tone::Ok
        } else if component.running {
            Tone::Warn
        } else {
            Tone::Bad
        };
        components.extend(badge(tone, component_label(component.kind)));
        components.push(Span::styled(
            format!(" {}    ", terminal_safe(&component.version)),
            theme.muted(),
        ));
    }
    if components.is_empty() {
        components.push(Span::styled("No shared services installed", theme.muted()));
    }
    let smtp = services.host_smtp_port.map_or_else(
        || terminal_safe(&services.internal_smtp),
        |port| {
            format!(
                "{} → 127.0.0.1:{port}",
                terminal_safe(&services.internal_smtp)
            )
        },
    );
    let mut details = vec![Span::styled(terminal_safe(&services.route), theme.accent())];
    details.push(Span::styled(format!("   smtp {smtp}"), theme.muted()));
    if let Some(runtime) = services.runtime {
        details.push(Span::styled(
            format!("   {}", runtime_label(runtime)),
            theme.muted(),
        ));
    }
    let meta = if services.installed {
        badge(Tone::Ok, "installed")
    } else {
        badge(Tone::Warn, "not installed · run mm dev setup")
    };
    frame.render_widget(
        Paragraph::new(vec![Line::from(components), Line::from(details)])
            .block(with_meta(card("Control plane", false), meta)),
        area,
    );
}

/// What the Services card says beside its title: how many services, and on
/// a cluster, how many nodes have answered so far and which do not answer.
fn services_meta(model: &Model) -> Vec<Span<'static>> {
    let theme = theme();
    let services = model.services().len();
    let mut meta = vec![Span::styled(plural(services, "service"), theme.muted())];
    let nodes = &model.snapshot.nodes;
    let waiting = nodes.iter().filter(|node| node.is_pending()).count();
    if waiting > 0 {
        // Rows of the nodes that answered are here; the others follow.
        meta.insert(0, Span::raw("   "));
        meta.insert(
            0,
            Span::styled(
                format!(
                    " {} of {} nodes answered",
                    nodes.len() - waiting,
                    nodes.len()
                ),
                theme.muted(),
            ),
        );
        meta.insert(0, Span::styled(spinner(model.tick), theme.accent()));
    }
    let silent = nodes
        .iter()
        .filter(|node| !node.reachable && !node.is_pending())
        .map(|node| terminal_safe(&node.name))
        .collect::<Vec<_>>();
    if !silent.is_empty() {
        let mut marked = badge(Tone::Bad, format!("{} not answering", silent.join(", ")));
        marked.push(Span::raw("   "));
        marked.append(&mut meta);
        meta = marked;
    }
    if services > 0 {
        meta.push(Span::raw("   "));
        meta.extend(hints(&[("enter", "actions")]));
    }
    meta
}

/// The rows of the Services table as drawn, and which of them are the
/// selected row and the rows beneath it.
struct TableLines {
    rows: Vec<Row<'static>>,
    /// The line of the selected row.
    selected: usize,
    /// The last line that belongs to the selected row: on a service that
    /// is unfolded, its last container or node.
    tail: usize,
}

fn table_lines(model: &Model, rows: &[ServiceRow<'_>], stacks: bool) -> TableLines {
    let node_width = node_name_width(model);
    let span = if stacks { 4 } else { 3 };
    let mut lines: Vec<Row<'static>> = rows
        .iter()
        .map(|row| {
            let mut cells = match row {
                ServiceRow::Service { service, expanded } => {
                    service_cells(model, service, *expanded)
                }
                ServiceRow::Container {
                    container, last, ..
                } => container_cells(model, container, *last),
                ServiceRow::Node { service, row } => {
                    return Row::new(node_cells(model, service, row, node_width, span));
                }
            };
            // The stack column earns its width only when there is more than one stack.
            if !stacks {
                cells.remove(2);
            }
            Row::new(cells)
        })
        .collect();
    lines.extend(optional_lines(model, stacks));
    let selected = model.scroll.min(lines.len().saturating_sub(1));
    let beneath = rows
        .iter()
        .skip(selected + 1)
        .take_while(|row| !matches!(row, ServiceRow::Service { .. }))
        .count();
    let tail = if matches!(rows.get(selected), Some(ServiceRow::Service { .. })) {
        selected + beneath
    } else {
        selected
    };
    TableLines {
        rows: lines,
        selected,
        tail,
    }
}

/// Draws the table and returns where the selected row's name starts, for the
/// action menu to open beside it.
fn render_service_table(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    rows: &[ServiceRow<'_>],
) -> Option<(u16, u16)> {
    let block = with_meta(card("Services", true), services_meta(model));
    if rows.is_empty() && model.optional_rows().is_empty() {
        empty_state(
            frame,
            area,
            block,
            Tone::Idle,
            "No services running",
            if model.remote().is_some() {
                "Press u to review and deploy this commit."
            } else {
                "Press u to review and start the stack."
            },
        );
        return None;
    }
    let stacks = model
        .services()
        .iter()
        .map(|service| service.stack.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1;
    let (widths, header): (&[Constraint], &[&str]) = if stacks {
        (
            &[
                Constraint::Length(12),
                Constraint::Min(22),
                Constraint::Length(12),
                Constraint::Length(19),
                Constraint::Length(8),
            ],
            &["STATE", "SERVICE", "STACK", "REPLICAS", "RESTARTS"],
        )
    } else {
        (
            &[
                Constraint::Length(12),
                Constraint::Min(22),
                Constraint::Length(19),
                Constraint::Length(8),
            ],
            &["STATE", "SERVICE", "REPLICAS", "RESTARTS"],
        )
    };
    let lines = table_lines(model, rows, stacks);
    let total = lines.rows.len();
    // The borders and the header leave the rest to the rows. The nodes of
    // the selected service stay in view with it.
    let viewport = usize::from(area.height.saturating_sub(3));
    let offset = (lines.tail + 1)
        .saturating_sub(viewport)
        .min(lines.selected);
    let mut state = TableState::default()
        .with_selected(Some(lines.selected))
        .with_offset(offset);
    frame.render_stateful_widget(
        selectable_table(
            Table::new(lines.rows, widths.iter().copied())
                .header(table_header(header))
                .block(block),
        ),
        area,
        &mut state,
    );
    scrollbar(frame, area, total, state.offset());
    // Border, header row, then the lines above the selected one.
    let row = u16::try_from(lines.selected.saturating_sub(state.offset())).unwrap_or(u16::MAX);
    // Beside the service name, moved left when that keeps the menu inside the table.
    let column = (area.x + 16)
        .min(area.right().saturating_sub(MENU_WIDTH + 1))
        .max(area.x + 1);
    let anchor = (column, area.y.saturating_add(2).saturating_add(row));
    (anchor.1 < area.bottom().saturating_sub(1)).then_some(anchor)
}

fn restarting(model: &Model, label: &str) -> bool {
    model.restarting.iter().any(|current| current == label)
}

fn restarting_badge(model: &Model) -> Vec<Span<'static>> {
    let style = theme().tone(Tone::Info);
    vec![
        Span::styled(spinner(model.tick), style),
        Span::styled(" restarting", style),
    ]
}

fn restart_count(restarts: u64) -> Cell<'static> {
    let theme = theme();
    Cell::from(Span::styled(
        restarts.to_string(),
        if restarts > 0 {
            theme.tone(Tone::Warn)
        } else {
            theme.muted()
        },
    ))
}

fn service_cells<'a>(model: &Model, service: &ServiceSnapshot, expanded: bool) -> Vec<Cell<'a>> {
    let theme = theme();
    let mut replicas = pips(
        service.desired_count,
        service.running_count,
        service.healthy_count,
    );
    replicas.push(Span::styled(
        format!("  {}/{}", service.healthy_count, service.desired_count),
        theme.muted(),
    ));
    let fold = match (service_nodes::foldable(service), expanded) {
        (false, _) => "  ",
        (true, true) => "▾ ",
        (true, false) => "▸ ",
    };
    let state = if restarting(model, &service.name) {
        restarting_badge(model)
    } else {
        badge(service_tone(service.state), service_state_word(service))
    };
    vec![
        Cell::from(Line::from(state)),
        Cell::from(Line::from(vec![
            Span::styled(fold, theme.muted()),
            Span::styled(terminal_safe(&service.name), theme.strong()),
            Span::styled(after_name(service), theme.muted()),
        ])),
        Cell::from(Span::styled(terminal_safe(&service.stack), theme.soft())),
        Cell::from(Line::from(replicas)),
        restart_count(service.restart_count.unwrap_or(0)),
    ]
}

/// How a container is doing: its tone and a short word for it.
fn container_health(container: &ContainerObservation) -> (Tone, String) {
    match (container.state.as_str(), container.health.as_deref()) {
        ("running", None | Some("healthy")) => (Tone::Ok, "running".to_owned()),
        ("running", Some(health)) => (Tone::Warn, terminal_safe(health)),
        (state, _) => (Tone::Bad, terminal_safe(state)),
    }
}

fn container_cells<'a>(
    model: &Model,
    container: &ContainerObservation,
    last: bool,
) -> Vec<Cell<'a>> {
    let theme = theme();
    let (tone, label) = container_health(container);
    let state = if restarting(model, &container.name) {
        restarting_badge(model)
    } else {
        vec![
            Span::raw(" "),
            Span::styled(tone.glyph(), theme.tone(tone)),
            Span::raw(" "),
            Span::styled(label, theme.tone(tone)),
        ]
    };
    // The state column already says whether a running container is healthy.
    let status = match container.status.split_once(" (") {
        Some((uptime, _)) if container.state == "running" => uptime,
        _ => container.status.as_str(),
    };
    vec![
        Cell::from(Line::from(state)),
        Cell::from(Line::from(vec![
            Span::styled(if last { "  └ " } else { "  ├ " }, theme.border()),
            Span::styled(terminal_safe(&container.name), theme.soft()),
        ])),
        Cell::from(""),
        Cell::from(Span::styled(terminal_safe(status), theme.muted())),
        restart_count(container.restart_count.unwrap_or(0)),
    ]
}

fn render_service_detail(frame: &mut Frame<'_>, area: Rect, model: &Model, row: ServiceRow<'_>) {
    match row {
        ServiceRow::Service { service, .. } => service_detail(frame, area, model, service),
        ServiceRow::Container {
            service, container, ..
        } => container_detail(frame, area, (service, None), container),
        ServiceRow::Node {
            service,
            row:
                NodeRow {
                    node,
                    container: Some(container),
                    ..
                },
        } => container_detail(frame, area, (service, Some(node)), container),
        ServiceRow::Node { service, row } => node_detail(frame, area, model, service, row.node),
    }
}

/// A node a service is placed on that runs no container of it.
fn node_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    service: &ServiceSnapshot,
    node: &str,
) {
    let width = 10;
    let lines = vec![
        Line::from(badge(
            Tone::Idle,
            missing_container_text(model, service, node),
        )),
        Line::raw(""),
        kv_text("Service", &service.name, width),
        kv_text("Node", node, width),
        Line::raw(""),
        Line::from(hints(&[("l", "logs of this node")])),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(terminal_safe(node), false))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The keys that act on the selected row, shown at the bottom of its details.
fn action_lines(model: &Model, target: &ServiceTarget) -> Vec<Line<'static>> {
    let theme = theme();
    let mut lines = vec![Line::raw(""), section("Actions")];
    for (key, label, action) in [
        ("l", "logs", ServiceAction::Logs),
        ("x", "run a command", ServiceAction::Run),
        ("r", "restart", ServiceAction::Restart),
        ("v", "variables", ServiceAction::Variables),
    ] {
        let available = model.action_unavailable(action, target).is_none();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{key}  "),
                if available {
                    theme.key()
                } else {
                    theme.muted()
                },
            ),
            Span::styled(
                label,
                if available {
                    theme.soft()
                } else {
                    theme.muted()
                },
            ),
            Span::styled(if available { "" } else { "  unavailable" }, theme.muted()),
        ]));
    }
    lines
}

/// On a cluster target: what the service's name resolves to on each of its
/// networks, in the words of `mm service status`.
/// An autoscaled service's count and its last change, from the same status
/// as `mm service status`.
fn scale_lines(service: &ServiceSnapshot, width: usize) -> Vec<Line<'static>> {
    let Some(scale) = &service.scale else {
        return Vec::new();
    };
    let mut lines = vec![kv_text(
        "Scale",
        &format!(
            "{} running, {} wanted ({}..{})",
            scale.running, scale.wanted, scale.min, scale.max
        ),
        width,
    )];
    if let Some(change) = &scale.last_change {
        lines.push(kv_text(
            "Changed",
            &format!("{} -> {}: {}", change.from, change.to, change.reason),
            width,
        ));
    }
    lines
}

/// What mm did about the service's unhealthy containers, from the same
/// status as `mm service status`.
fn restart_lines(service: &ServiceSnapshot, width: usize) -> Vec<Line<'static>> {
    let Some(restarts) = &service.restarts else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    if let Some(last) = restarts.restarts.last() {
        lines.push(kv_text(
            "Restarted",
            &format!(
                "{} by mm, the last {}: {}",
                restarts.restarts.len(),
                crate::presentation::output::restarts::where_and_when(last),
                last.reason
            ),
            width,
        ));
    }
    for event in &restarts.limited {
        lines.push(kv_text(
            "Given up",
            &format!(
                "{}: {} ({})",
                crate::presentation::output::restarts::where_and_when(event),
                event.reason,
                crate::domain::healing::RESTART_LIMIT
            ),
            width,
        ));
    }
    lines
}

fn name_lines(model: &Model, service: &ServiceSnapshot, width: usize) -> Vec<Line<'static>> {
    model
        .snapshot
        .names
        .iter()
        .flat_map(|names| &names.names)
        .filter(|name| name.name == service.name)
        .map(|name| {
            kv_text(
                "Resolves",
                &format!(
                    "{}: {}",
                    name.network,
                    crate::presentation::output::names::resolution(name)
                ),
                width,
            )
        })
        .collect()
}

fn service_detail(frame: &mut Frame<'_>, area: Rect, model: &Model, service: &ServiceSnapshot) {
    let theme = theme();
    let width = 10;
    let mut lines = vec![
        Line::from(badge(
            service_tone(service.state),
            service_state_word(service),
        )),
        Line::raw(""),
        kv_text("Stack", &service.stack, width),
    ];
    lines.extend(placement_lines(service, width));
    lines.extend([
        kv_text("Module", service.module.as_deref().unwrap_or("—"), width),
        kv_text("Image", service.image.as_deref().unwrap_or("—"), width),
        kv_text(
            "Revision",
            &service.revision.as_deref().map_or_else(
                || "—".to_owned(),
                |revision| revision.chars().take(12).collect(),
            ),
            width,
        ),
        kv_text(
            "Healthy",
            &format!("{}/{}", service.healthy_count, service.desired_count),
            width,
        ),
    ]);
    if let Some(route) = model
        .snapshot
        .ingress
        .iter()
        .filter(|status| status.stack == service.stack && status.service == service.name)
        .find_map(|status| status.url.as_deref())
    {
        lines.push(kv(
            "Route",
            vec![Span::styled(terminal_safe(route), theme.accent())],
            width,
        ));
    }
    lines.extend(scale_lines(service, width));
    lines.extend(restart_lines(service, width));
    lines.extend(name_lines(model, service, width));
    lines.extend(drift_lines(model, service, width));
    if let Some(code) = service.exit_code {
        lines.push(kv(
            "Exit",
            vec![Span::styled(code.to_string(), theme.tone(Tone::Bad))],
            width,
        ));
    }
    if let Some(error) = service
        .error
        .as_deref()
        .or(service.diagnostic_code.as_deref())
    {
        lines.push(kv(
            "Problem",
            vec![Span::styled(terminal_safe(error), theme.tone(Tone::Bad))],
            width,
        ));
    }
    lines.extend(action_lines(
        model,
        &ServiceRow::Service {
            service,
            expanded: false,
        }
        .target(),
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(terminal_safe(&service.name), false))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn container_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    (service, node): (&ServiceSnapshot, Option<&str>),
    container: &ContainerObservation,
) {
    let theme = theme();
    let width = 10;
    let (tone, label) = container_health(container);
    let mut lines = vec![
        Line::from(badge(tone, label)),
        Line::raw(""),
        kv_text("Service", &service.name, width),
    ];
    if let Some(node) = node {
        lines.push(kv_text("Node", node, width));
    }
    lines.extend([
        kv_text("Status", &container.status, width),
        kv_text("Image", &container.image, width),
        kv_text(
            "ID",
            &container.id.chars().take(12).collect::<String>(),
            width,
        ),
        kv_text(
            "Restarts",
            &container.restart_count.unwrap_or(0).to_string(),
            width,
        ),
    ]);
    if let Some(code) = container.exit_code {
        lines.push(kv(
            "Exit",
            vec![Span::styled(code.to_string(), theme.tone(Tone::Bad))],
            width,
        ));
    }
    if let Some(error) = container
        .error
        .as_deref()
        .or(container.diagnostic_code.as_deref())
    {
        lines.push(kv(
            "Problem",
            vec![Span::styled(terminal_safe(error), theme.tone(Tone::Bad))],
            width,
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(hints(&[(
        "enter",
        "actions for this container",
    )])));
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(terminal_safe(&container.name), false))
            .wrap(Wrap { trim: false }),
        area,
    );
}
