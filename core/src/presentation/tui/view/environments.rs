use super::*;
use crate::presentation::tui::environments::{
    EnvironmentEntry, EnvironmentProbe, NodeLine, ProbeState, ServiceLine,
};

/// Content width at which the selected environment's details open beside the table.
const DETAIL_BREAKPOINT: u16 = 110;

/// What is known about one environment right now.
enum Known<'a> {
    /// The environment this session shows; its numbers are live.
    Current(EnvironmentProbe),
    Checked(&'a EnvironmentProbe, std::time::Instant),
    Checking,
    Failed(&'a str, std::time::Instant),
    /// Not checked yet, or it cannot be resolved at all.
    Unknown,
}

/// The current environment's status, read from the live snapshot.
fn current_probe(model: &Model) -> EnvironmentProbe {
    let identity = model.access.identity.as_ref();
    EnvironmentProbe {
        services: model
            .services()
            .iter()
            .map(|service| ServiceLine::from(*service))
            .collect(),
        routes: model
            .snapshot
            .ingress
            .iter()
            .filter(|status| status.url.is_some())
            .count(),
        issues: model.issue_count(),
        principal: identity.map(|identity| identity.principal.clone()),
        access: identity
            .and_then(|identity| identity.access.as_ref())
            .map(|access| access.summary()),
        deployed_commit: identity.and_then(|identity| identity.deployed_commit.clone()),
        nodes: model.environments.current_nodes(&model.snapshot),
    }
}

/// When the nodes of the environment on screen were last checked. Whether
/// each answers is live; its `mmd` version and mesh are as old as this says.
fn nodes_checked(model: &Model) -> Option<Line<'static>> {
    let theme = theme();
    if model.snapshot.nodes.len() < 2 {
        return None;
    }
    Some(
        match model.environments.probes.get(&model.snapshot.environment)? {
            ProbeState::Checking => Line::from(vec![
                Span::styled(spinner(model.tick), theme.accent()),
                Span::styled(" Checking the nodes…", theme.accent()),
            ]),
            ProbeState::Ready { at, .. } => {
                Line::styled(format!("Nodes checked {}", age(*at)), theme.muted())
            }
            ProbeState::Failed { message, .. } => Line::styled(
                format!("The node check failed: {}", terminal_safe(message)),
                theme.soft(),
            ),
        },
    )
}

/// One node: its name, then what is known about it. The parts that do not
/// fit beside the name continue beneath it, indented, so a part such as
/// `mesh 2/2 peers` is never cut in two.
fn node_lines(node: &NodeLine, columns: usize) -> Vec<Line<'static>> {
    let theme = theme();
    let tone = if node.ready { Tone::Ok } else { Tone::Bad };
    let mut lines = Vec::new();
    let mut spans = badge(tone, terminal_safe(&node.name));
    let mut used = node.name.chars().count() + 2;
    for part in node.detail.split("  ").filter(|part| !part.is_empty()) {
        let part = terminal_safe(part);
        let needed = part.chars().count() + 2;
        if used + needed > columns && used > 2 {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        spans.push(Span::styled(format!("  {part}"), theme.muted()));
        used += needed;
    }
    lines.push(Line::from(spans));
    lines
}

fn known<'a>(model: &'a Model, entry: &EnvironmentEntry) -> Known<'a> {
    if entry.name == model.snapshot.environment {
        return Known::Current(current_probe(model));
    }
    match model.environments.probes.get(&entry.name) {
        Some(ProbeState::Ready { probe, at }) => Known::Checked(probe, *at),
        Some(ProbeState::Checking) => Known::Checking,
        Some(ProbeState::Failed { message, at }) => Known::Failed(message, *at),
        None => Known::Unknown,
    }
}

fn age(at: std::time::Instant) -> String {
    match at.elapsed().as_secs() {
        0..5 => "just now".to_owned(),
        seconds @ 5..60 => format!("{seconds}s ago"),
        seconds => format!("{}m ago", seconds / 60),
    }
}

fn location(entry: &EnvironmentEntry) -> Vec<Span<'static>> {
    if entry.problem.is_some() {
        vec![Span::styled("—", theme().muted())]
    } else if entry.remote.is_some() {
        badge(Tone::Info, terminal_safe(&entry.target))
    } else {
        badge(Tone::Idle, "local")
    }
}

/// `3/4` with a tone, or why there is no count.
fn services_summary(probe: &EnvironmentProbe) -> Vec<Span<'static>> {
    let total = probe.services.len();
    let running = probe.running();
    if total == 0 {
        return badge(Tone::Idle, "nothing running");
    }
    let tone = if running == total {
        Tone::Ok
    } else if running == 0 {
        Tone::Bad
    } else {
        Tone::Warn
    };
    badge(tone, format!("{running}/{total} running"))
}

pub(super) fn render_environments(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let entries = &model.environments.entries;
    let selected = entries.get(model.scroll);
    if area.width >= DETAIL_BREAKPOINT
        && let Some(entry) = selected
    {
        let [table, _, detail] = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(46),
        ])
        .areas(area);
        render_environment_table(frame, table, model);
        render_environment_detail(frame, detail, model, entry);
    } else if area.height >= 20
        && let Some(entry) = selected
    {
        let [table, detail] =
            Layout::vertical([Constraint::Min(6), Constraint::Length(area.height / 2)]).areas(area);
        render_environment_table(frame, table, model);
        render_environment_detail(frame, detail, model, entry);
    } else {
        render_environment_table(frame, area, model);
    }
}

fn render_environment_table(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let entries = &model.environments.entries;
    let remote = entries
        .iter()
        .filter(|entry| entry.remote.is_some())
        .count();
    let block = with_meta(
        card("Environments", true),
        vec![Span::styled(
            format!("{} · {remote} remote", plural(entries.len(), "environment")),
            theme.muted(),
        )],
    );
    if entries.is_empty() {
        empty_state(
            frame,
            area,
            block,
            Tone::Idle,
            "No environments configured",
            "Add one under `environments:` in the project configuration.",
        );
        return;
    }
    let wide = area.width >= 100;
    let rows = entries
        .iter()
        .map(|entry| environment_row(model, entry, wide));
    // Narrow terminals keep what answers "where is it and is it up".
    let (widths, header): (&[Constraint], &[&str]) = if wide {
        (
            &[
                Constraint::Min(18),
                Constraint::Length(16),
                Constraint::Length(15),
                Constraint::Length(17),
                Constraint::Length(13),
                Constraint::Length(9),
            ],
            &[
                "ENVIRONMENT",
                "RUNS ON",
                "RUNTIME",
                "SERVICES",
                "DEPLOYED",
                "CHECKED",
            ],
        )
    } else {
        (
            &[
                Constraint::Min(14),
                Constraint::Length(14),
                Constraint::Length(17),
            ],
            &["ENVIRONMENT", "RUNS ON", "SERVICES"],
        )
    };
    render_table(
        frame,
        area,
        Table::new(rows, widths.iter().copied())
            .header(table_header(header))
            .block(block),
        entries.len(),
        Some(model.scroll),
    );
}

/// What the last check says about an environment: its services, its
/// deployed commit, and how old the check is.
fn checked_cells(
    model: &Model,
    entry: &EnvironmentEntry,
) -> (Vec<Span<'static>>, Option<String>, Vec<Span<'static>>) {
    let theme = theme();
    match known(model, entry) {
        Known::Current(probe) => (
            services_summary(&probe),
            probe.deployed_commit,
            badge(Tone::Ok, "live"),
        ),
        Known::Checked(probe, at) => (
            services_summary(probe),
            probe.deployed_commit.clone(),
            vec![Span::styled(age(at), theme.muted())],
        ),
        Known::Checking => (
            vec![
                Span::styled(spinner(model.tick), theme.accent()),
                Span::styled(" checking", theme.accent()),
            ],
            None,
            Vec::new(),
        ),
        Known::Failed(_, at) => (
            badge(Tone::Bad, "unreachable"),
            None,
            vec![Span::styled(age(at), theme.muted())],
        ),
        Known::Unknown if entry.problem.is_some() => {
            (badge(Tone::Bad, "not set up"), None, Vec::new())
        }
        Known::Unknown => (vec![Span::styled("—", theme.muted())], None, Vec::new()),
    }
}

fn environment_row(model: &Model, entry: &EnvironmentEntry, wide: bool) -> Row<'static> {
    let theme = theme();
    let current = entry.name == model.snapshot.environment;
    let mut name = vec![Span::styled(
        terminal_safe(&entry.name),
        if current { theme.key() } else { theme.strong() },
    )];
    if current {
        name.push(Span::styled("  current", theme.accent()));
    } else if entry.default {
        name.push(Span::styled("  default", theme.muted()));
    }
    let (services, deployed, checked) = checked_cells(model, entry);
    let mut cells = vec![
        Cell::from(Line::from(name)),
        Cell::from(Line::from(location(entry))),
        Cell::from(Line::from(services)),
    ];
    if wide {
        cells.insert(
            2,
            Cell::from(Span::styled(
                entry.runtime.map_or("—", runtime_label),
                theme.soft(),
            )),
        );
        cells.push(Cell::from(Span::styled(
            deployed
                .as_deref()
                .map_or_else(|| "—".to_owned(), |commit| short_commit(commit).to_owned()),
            theme.muted(),
        )));
        cells.push(Cell::from(Line::from(checked)));
    }
    Row::new(cells)
}

fn probe_lines(
    probe: &EnvironmentProbe,
    remote: bool,
    width: usize,
    columns: usize,
    nodes_checked: Option<Line<'static>>,
) -> Vec<Line<'static>> {
    let theme = theme();
    let mut lines = Vec::new();
    if remote {
        lines.push(match &probe.principal {
            Some(principal) => kv(
                "You are",
                vec![
                    Span::styled(terminal_safe(principal), theme.strong()),
                    Span::styled(
                        format!(
                            "  {}",
                            terminal_safe(probe.access.as_deref().unwrap_or("no access here"))
                        ),
                        theme.text(),
                    ),
                ],
                width,
            ),
            None => kv_text("You are", "unknown to the host", width),
        });
        lines.push(kv_text(
            "Deployed",
            probe
                .deployed_commit
                .as_deref()
                .map_or("nothing yet", short_commit),
            width,
        ));
    }
    lines.push(kv_text("Routes", &probe.routes.to_string(), width));
    lines.push(kv(
        "Issues",
        if probe.issues == 0 {
            badge(Tone::Ok, "none")
        } else {
            badge(Tone::Warn, probe.issues.to_string())
        },
        width,
    ));
    if !probe.nodes.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Nodes"));
        for node in &probe.nodes {
            lines.extend(node_lines(node, columns));
        }
        lines.extend(nodes_checked);
    }
    if !probe.services.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Services"));
        for service in &probe.services {
            let mut spans = badge(service_tone(service.state), terminal_safe(&service.name));
            spans.push(Span::styled(
                format!("  {}", terminal_safe(&service.text)),
                theme.muted(),
            ));
            lines.push(Line::from(spans));
        }
    }
    lines
}

/// Where an environment runs and with what, from the configuration.
fn entry_lines(model: &Model, entry: &EnvironmentEntry, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(match &entry.remote {
            _ if entry.problem.is_some() => badge(Tone::Bad, "not set up"),
            Some(_) => badge(
                Tone::Info,
                format!("remote · {}", terminal_safe(&entry.target)),
            ),
            None => badge(Tone::Idle, "local · this machine"),
        }),
        Line::raw(""),
    ];
    // A cluster lists its nodes below; the first node's address alone
    // would read as the only host.
    let nodes = match known(model, entry) {
        Known::Current(probe) => probe.nodes.len(),
        Known::Checked(probe, _) => probe.nodes.len(),
        _ => 0,
    };
    match &entry.remote {
        Some(_) if nodes > 1 => lines.push(kv_text("Nodes", &nodes.to_string(), width)),
        Some(host) => lines.push(kv_text("Host", host, width)),
        None => {}
    }
    lines.push(kv_text(
        "Runtime",
        entry.runtime.map_or("—", runtime_label),
        width,
    ));
    if !entry.deploy_from.is_empty() {
        lines.push(kv_text(
            "Deploys from",
            &entry.deploy_from.join(", "),
            width,
        ));
    }
    lines
}

fn render_environment_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    entry: &EnvironmentEntry,
) {
    let theme = theme();
    let width = 13;
    let current = entry.name == model.snapshot.environment;
    let mut lines = entry_lines(model, entry, width);
    let columns = usize::from(area.width.saturating_sub(2));
    let mut keys = vec![("r", "check again")];
    match known(model, entry) {
        Known::Current(probe) => {
            let remote = entry.remote.is_some();
            let checked = nodes_checked(model);
            lines.extend(probe_lines(&probe, remote, width, columns, checked));
            // Only a cluster has anything to check again: its nodes.
            if model.snapshot.nodes.len() < 2 {
                keys.clear();
            }
        }
        Known::Checked(probe, _) => {
            let remote = entry.remote.is_some();
            lines.extend(probe_lines(probe, remote, width, columns, None));
            keys.insert(0, ("enter", "switch"));
        }
        Known::Checking => {
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled(spinner(model.tick), theme.accent()),
                Span::styled(" Checking status…", theme.accent()),
            ]));
            keys = vec![("enter", "switch")];
        }
        Known::Failed(message, _) => {
            lines.push(Line::raw(""));
            lines.push(Line::from(badge(Tone::Bad, "The last check failed")));
            lines.push(Line::styled(terminal_safe(message), theme.soft()));
            keys.insert(0, ("enter", "switch anyway"));
        }
        Known::Unknown => {
            if let Some(problem) = &entry.problem {
                lines.push(Line::raw(""));
                lines.push(Line::from(badge(
                    Tone::Bad,
                    "This environment is not set up on this machine",
                )));
                lines.push(Line::styled(terminal_safe(problem), theme.soft()));
                keys = vec![("enter", "see what it needs")];
            } else {
                keys.insert(0, ("enter", "switch"));
            }
        }
    }
    if current {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            "This is the environment every other view shows.",
            theme.muted(),
        ));
    }
    if !keys.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(hints(&keys)));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(terminal_safe(&entry.name), false))
            .wrap(Wrap { trim: false }),
        area,
    );
}
