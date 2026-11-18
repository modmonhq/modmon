use super::*;
use crate::presentation::tui::loading::Part;

/// A stat card whose number is still being read.
fn reading_card(title: &'static str) -> StatCard {
    (title, "…".to_owned(), "reading".to_owned(), Tone::Idle)
}

pub(super) fn render_overview(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    // The setup card keeps the height it needs; the details take what is left.
    let setup = if model.setup.is_open() {
        setup_height(model, area.width).min(area.height.saturating_sub(5))
    } else {
        0
    };
    let [stats, setup, details] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(setup),
        Constraint::Min(0),
    ])
    .areas(area);
    render_stats(frame, stats, model);
    if setup.height > 0 {
        render_setup(frame, setup, model);
    }
    if details.height < 4 {
        return;
    }
    let [workspace, _, attention] = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Length(1),
        Constraint::Percentage(50),
    ])
    .areas(details);
    render_workspace(frame, workspace, model);
    render_attention(frame, attention, model);
}

fn render_stats(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let reading = model.loading.is_pending(Part::Status);
    // Local HTTPS is this machine's; a remote environment's last card says
    // whether its hosts answer and what they run.
    let last = if model.remote().is_some() {
        if reading {
            reading_card(hosts_title(model))
        } else {
            hosts_card(model)
        }
    } else if model.loading.is_pending(Part::Https) {
        reading_card("HTTPS")
    } else {
        let (value, detail, tone) = tls_card(model.tls.as_ref());
        ("HTTPS", value, detail, tone)
    };
    let cards = if reading {
        [
            reading_card("Services"),
            reading_card("Routes"),
            reading_card("Issues"),
            last,
        ]
    } else {
        stat_cards(model, last)
    };
    let areas = Layout::horizontal([Constraint::Ratio(1, 4); 4])
        .spacing(1)
        .split(area);
    for ((title, value, detail, tone), area) in cards.into_iter().zip(areas.iter()) {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(tone.glyph(), theme.tone(tone)),
                    Span::raw(" "),
                    Span::styled(value, theme.strong()),
                ]),
                Line::styled(detail, theme.muted()),
            ])
            .block(card(title, false)),
            *area,
        );
    }
}

type StatCard = (&'static str, String, String, Tone);

/// The four numbers of the Overview once the status is here.
fn stat_cards(model: &Model, https: StatCard) -> [StatCard; 4] {
    let services = model.services();
    let running = services
        .iter()
        .filter(|service| service.state.is_up())
        .count();
    let degraded = services
        .iter()
        .filter(|service| service.state == ServiceState::Degraded)
        .count();
    let routed = model
        .snapshot
        .ingress
        .iter()
        .filter(|status| status.url.is_some())
        .count();
    let diagnostics = model.diagnostics();
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Warning)
        .count();
    let service_tone = if services.is_empty() {
        Tone::Idle
    } else if running == services.len() {
        Tone::Ok
    } else {
        Tone::Warn
    };
    let issue_tone = if errors > 0 {
        Tone::Bad
    } else if warnings > 0 {
        Tone::Warn
    } else if model.setup.is_open() {
        Tone::Idle
    } else {
        Tone::Ok
    };
    [
        (
            "Services",
            format!("{running}/{}", services.len()),
            if degraded > 0 {
                format!("running · {degraded} degraded")
            } else {
                "running".to_owned()
            },
            service_tone,
        ),
        (
            "Routes",
            routed.to_string(),
            format!("of {} services routed", model.snapshot.ingress.len()),
            if routed > 0 { Tone::Ok } else { Tone::Idle },
        ),
        (
            "Issues",
            (errors + warnings).to_string(),
            if errors + warnings == 0 && model.setup.is_open() {
                "apart from setup".to_owned()
            } else if errors + warnings == 0 {
                "all checks passed".to_owned()
            } else {
                format!(
                    "{} · {}",
                    plural(errors, "error"),
                    plural(warnings, "warning")
                )
            },
            issue_tone,
        ),
        https,
    ]
}

fn hosts_title(model: &Model) -> &'static str {
    if model.snapshot.nodes.len() > 1 {
        "Nodes"
    } else {
        "Deployed"
    }
}

/// On a cluster, how many nodes answer; on one host, the release it runs,
/// as the `deployed:` line of `mm service status` says it.
fn hosts_card(model: &Model) -> StatCard {
    let nodes = &model.snapshot.nodes;
    if nodes.len() > 1 {
        let answering = nodes.iter().filter(|node| node.reachable).count();
        let silent = nodes
            .iter()
            .filter(|node| !node.reachable && !node.is_pending())
            .count();
        let (detail, tone) = match silent {
            0 if answering == nodes.len() => ("answering".to_owned(), Tone::Ok),
            0 => ("answering so far".to_owned(), Tone::Idle),
            silent => (format!("{silent} not answering"), Tone::Bad),
        };
        return (
            "Nodes",
            format!("{answering}/{}", nodes.len()),
            detail,
            tone,
        );
    }
    match &model.snapshot.deployment {
        Some(deployment) => (
            "Deployed",
            short_commit(&deployment.commit).to_owned(),
            format!("by {}", deployment.deployed_by),
            Tone::Ok,
        ),
        None => (
            "Deployed",
            "nothing".to_owned(),
            "yet".to_owned(),
            Tone::Idle,
        ),
    }
}

/// The HTTPS card: whether the local CA is installed, trusted, and current.
fn tls_card(tls: Option<&crate::domain::ca::LocalCaStatus>) -> (String, String, Tone) {
    match tls {
        None => (
            "unknown".to_owned(),
            "status unavailable".to_owned(),
            Tone::Idle,
        ),
        Some(tls) if !tls.installed => (
            "missing".to_owned(),
            "run mm dev setup".to_owned(),
            Tone::Bad,
        ),
        Some(tls) if tls_ready(tls) => (
            "trusted".to_owned(),
            plural(tls.issued_hosts.len(), "host"),
            Tone::Ok,
        ),
        Some(_) => ("repair".to_owned(), "see TLS view".to_owned(), Tone::Warn),
    }
}

/// A value that is still being read: quieter than one that is here.
fn reading_value() -> Vec<Span<'static>> {
    vec![Span::styled("reading", theme().muted())]
}

/// The runtime as the engine describes itself, or that it is being asked.
fn runtime_lines(model: &Model, width: usize) -> Vec<Line<'static>> {
    let runtime = &model.snapshot.runtime;
    let reading = model.loading.is_pending(Part::Status);
    let text = |value: &str| vec![Span::styled(terminal_safe(value), theme().text())];
    let engine = match runtime.engine_version.as_deref() {
        Some(version) => text(version),
        None if reading => reading_value(),
        None => vec![Span::styled("unavailable", theme().tone(Tone::Bad))],
    };
    let compose = match &runtime.compose_provider {
        Some(provider) => text(&provider.name),
        None if reading => reading_value(),
        None => text("not detected"),
    };
    let rootless = match runtime.rootless {
        Some(rootless) => text(if rootless { "yes" } else { "no" }),
        None if reading => reading_value(),
        None => text("unknown"),
    };
    // A remote status reads the hosts' engine version and nothing more.
    if model.remote().is_some() {
        return vec![
            kv_text("Runtime", runtime_label(runtime.kind), width),
            kv("Engine", engine, width),
        ];
    }
    vec![
        kv_text("Runtime", runtime_label(runtime.kind), width),
        kv("Engine", engine, width),
        kv("Compose", compose, width),
        kv("Rootless", rootless, width),
    ]
}

fn render_workspace(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let width = 13;
    let reading = model.loading.is_pending(Part::Status);
    // Unfinished setup is reported with the configuration's findings, but it
    // says nothing about the configuration file.
    let invalid = model
        .snapshot
        .validation
        .diagnostics
        .iter()
        .any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && !model
                    .setup
                    .steps
                    .iter()
                    .any(|step| step.code == Some(diagnostic.code.as_str()))
        });
    let configuration = if reading {
        reading_value()
    } else if model.setup.unavailable.is_some() {
        badge(Tone::Idle, "not checked")
    } else if !invalid {
        badge(Tone::Ok, "valid")
    } else {
        badge(Tone::Bad, "invalid")
    };
    let delivery = match &model.environment {
        Some(environment) => vec![Span::styled(
            terminal_safe(&format!(
                "{} · {}",
                environment.delivery.mode,
                environment.delivery.provenance.as_str()
            )),
            theme().text(),
        )],
        None if model.loading.is_pending(Part::Variables) => reading_value(),
        None => vec![Span::styled("unavailable", theme().text())],
    };
    let theme = theme();
    let runs_on = match model.remote() {
        Some(target) => badge(Tone::Info, format!("remote · {}", terminal_safe(target))),
        None => vec![Span::styled("this machine", theme.text())],
    };
    let mut lines = vec![
        kv_text(
            "Root",
            &model.snapshot.workspace.root.display().to_string(),
            width,
        ),
        kv("Runs on", runs_on, width),
        kv("Config", configuration, width),
    ];
    lines.extend(runtime_lines(model, width));
    lines.push(kv("Env delivery", delivery, width));
    // Inside the card: its borders and padding take four columns.
    let room = usize::from(area.width.saturating_sub(4));
    lines.extend(deployed_lines(model, width, room));
    let mut block = card("Workspace", false);
    if model.environments.entries.len() > 1 {
        let key = Tab::Environments
            .digit()
            .map(String::from)
            .unwrap_or_default();
        block = with_meta(block, hints(&[(key.as_str(), "environments")]));
        let entries = &model.environments.entries;
        let remote = entries
            .iter()
            .filter(|entry| entry.remote.is_some())
            .count();
        lines.push(kv_text(
            "Environments",
            &format!("{} · {remote} remote", entries.len()),
            width,
        ));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_attention(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    if model.loading.is_pending(Part::Status) {
        // Nothing can be said about the environment before it answers.
        let mut lines = vec![reading_line(model, Part::Status)];
        lines.extend(node_lines(model));
        frame.render_widget(
            Paragraph::new(lines)
                .block(card("Needs attention", false))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }
    let mut lines = Vec::new();
    if let LiveState::Failed(message) = &model.live {
        lines.push(Line::from(badge(
            Tone::Warn,
            "Live refresh failed; retrying",
        )));
        lines.push(Line::styled(
            format!("  {}", terminal_safe(message)),
            theme.muted(),
        ));
    }
    let services = model.services();
    // While setup is open its card says what to do; starting comes after.
    if !model.setup.is_open()
        && services
            .iter()
            .all(|service| service.state != ServiceState::Running)
    {
        lines.push(Line::from(badge(Tone::Idle, "The stack is not running")));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("u", theme.key()),
            Span::styled(
                if model.remote().is_some() {
                    " reviews and deploys this commit"
                } else {
                    " reviews and starts it"
                },
                theme.muted(),
            ),
        ]));
    }
    let mut issues = model
        .diagnostics()
        .into_iter()
        .filter(|diagnostic| diagnostic.severity != Severity::Info)
        // The setup card already lists these, in plainer words.
        .filter(|diagnostic| {
            !model
                .setup
                .steps
                .iter()
                .any(|step| step.code == Some(diagnostic.code.as_str()))
        })
        .collect::<Vec<_>>();
    issues.sort_by_key(|diagnostic| diagnostic.severity != Severity::Error);
    for diagnostic in &issues {
        lines.push(Line::from(badge(
            severity_tone(diagnostic.severity),
            terminal_safe(&diagnostic.message),
        )));
    }
    let block = with_meta(
        card("Needs attention", false),
        if issues.is_empty() {
            Vec::new()
        } else {
            let key = Tab::Diagnostics
                .digit()
                .map(String::from)
                .unwrap_or_default();
            hints(&[(key.as_str(), "details")])
        },
    );
    if lines.is_empty() {
        if model.setup.is_open() {
            empty_state(frame, area, block, Tone::Idle, "Nothing else to fix", "");
        } else {
            empty_state(frame, area, block, Tone::Ok, "Everything looks good", "");
        }
        return;
    }
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}
