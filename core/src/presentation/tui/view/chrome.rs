use super::*;
use crate::presentation::tui::editor::EditorStage;
use crate::presentation::tui::gate::GateScreen;
use crate::presentation::tui::model::{Confirm, JobsPane, NoticeKind, TabGroup, VariablesPane};
use crate::presentation::tui::panel::{Panel, RunState, StreamState};
use crate::presentation::tui::provider::ProviderScreen;

pub(super) fn render_header(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(theme.border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut status = Vec::new();
    if let Some(operation) = &model.busy {
        status.push(Span::styled(spinner(model.tick), theme.accent()));
        status.push(Span::styled(
            format!(" {}…", operation.progress(model.remote().is_some())),
            theme.accent(),
        ));
        status.push(Span::raw("   "));
    }
    let services = model.services();
    if !services.is_empty() {
        let running = services
            .iter()
            .filter(|service| service.state.is_up())
            .count();
        let tone = if running == services.len() {
            Tone::Ok
        } else {
            Tone::Warn
        };
        status.extend(badge(tone, format!("{running}/{} running", services.len())));
        status.push(Span::raw("   "));
    }
    let reading = model
        .loading
        .is_pending(crate::presentation::tui::loading::Part::Status);
    if !reading {
        status.extend(health_status(model));
    }
    if model.loading.is_active() {
        // The session is still coming in; nothing is "live" yet.
        status.push(Span::styled(spinner(model.tick), theme.accent()));
        status.push(Span::styled(" reading", theme.muted()));
    } else {
        status.extend(match &model.live {
            LiveState::Live => badge(Tone::Ok, "live"),
            LiveState::Failed(_) => badge(Tone::Warn, "refresh failed"),
        });
    }
    status.push(Span::raw(" "));
    let status = Line::from(status);
    let status_width = u16::try_from(status.width()).unwrap_or(u16::MAX);

    let available = usize::from(inner.width).saturating_sub(status.width() + 2);
    // A remote environment gets a solid chip: what you do here happens elsewhere.
    let (chip, place, place_tone) = match model.remote() {
        Some(target) => (
            theme.chip(Tone::Info),
            format!("remote · {}", terminal_safe(target)),
            Tone::Info,
        ),
        None => (theme.chip_quiet(), "local".to_owned(), Tone::Idle),
    };
    let mut identity = vec![
        Span::styled(" mm ", theme.brand()),
        Span::raw("  "),
        Span::styled(terminal_safe(&model.snapshot.project_name), theme.strong()),
        Span::raw("  "),
        Span::styled(
            format!(" {} ", terminal_safe(&model.snapshot.environment)),
            chip,
        ),
    ];
    // Context tags drop from the right before they can collide with the status.
    for (tag, style) in [
        (place, theme.tone(place_tone)),
        (
            runtime_label(model.snapshot.runtime.kind).to_owned(),
            theme.soft(),
        ),
    ] {
        let separator = Span::styled("  ·  ", theme.border());
        let tag = Span::styled(tag, style);
        if Line::from(identity.clone()).width() + separator.width() + tag.width() > available {
            break;
        }
        if identity.len() > 5 {
            identity.push(separator);
        } else {
            identity.push(Span::raw("  "));
        }
        identity.push(tag);
    }
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(status_width.min(inner.width / 2)),
    ])
    .areas(inner);
    frame.render_widget(Paragraph::new(Line::from(identity)), left);
    frame.render_widget(Paragraph::new(status.right_aligned()), right);
}

/// Setup and issue counts for the header. Unfinished setup is the headline;
/// "no issues" beside it would mislead.
fn health_status(model: &Model) -> Vec<Span<'static>> {
    let mut status = Vec::new();
    if !model.setup.steps.is_empty() {
        status.extend(badge(
            Tone::Warn,
            plural(model.setup.steps.len(), "setup step"),
        ));
        status.push(Span::raw("   "));
    } else if model.setup.unavailable.is_some() {
        status.extend(badge(Tone::Bad, "not available"));
        status.push(Span::raw("   "));
    }
    let issues = model.issue_count();
    if issues > 0 {
        status.extend(badge(issues_tone(model), plural(issues, "issue")));
        status.push(Span::raw("   "));
    } else if !model.setup.is_open() {
        status.extend(badge(Tone::Ok, "no issues"));
        status.push(Span::raw("   "));
    }
    status
}

fn issues_tone(model: &Model) -> Tone {
    if model
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
    {
        Tone::Bad
    } else {
        Tone::Warn
    }
}

pub(super) fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// A short status shown beside a view's name in the sidebar.
fn tab_meta(model: &Model, tab: Tab) -> Option<(String, Tone)> {
    match tab {
        Tab::Environments => {
            let entries = &model.environments.entries;
            (entries.len() > 1).then(|| (entries.len().to_string(), Tone::Idle))
        }
        Tab::Variables => {
            let problems = model
                .all_environment_rows()
                .iter()
                .filter(|row| row.is_problem())
                .count();
            (problems > 0).then(|| (problems.to_string(), Tone::Bad))
        }
        Tab::Services => {
            let services = model.services();
            (!services.is_empty()).then(|| {
                let running = services
                    .iter()
                    .filter(|service| service.state.is_up())
                    .count();
                let tone = if running == services.len() {
                    Tone::Ok
                } else {
                    Tone::Warn
                };
                (format!("{running}/{}", services.len()), tone)
            })
        }
        Tab::Ingress => {
            let routed = model
                .snapshot
                .ingress
                .iter()
                .filter(|status| status.url.is_some())
                .count();
            (routed > 0).then(|| (format!("{routed} ↗"), Tone::Idle))
        }
        // No verdict before the environment has answered.
        Tab::Diagnostics
            if model
                .loading
                .is_pending(crate::presentation::tui::loading::Part::Status) =>
        {
            None
        }
        Tab::Diagnostics => Some(match model.issue_count() {
            0 => ("✓".to_owned(), Tone::Ok),
            count => (count.to_string(), issues_tone(model)),
        }),
        // Local HTTPS is this machine's, not a remote environment's.
        Tab::Tls if model.remote().is_some() => None,
        Tab::Tls => model.tls.as_ref().map(|tls| {
            if tls_ready(tls) {
                ("✓".to_owned(), Tone::Ok)
            } else {
                ("!".to_owned(), Tone::Warn)
            }
        }),
        Tab::Gates => (!model.gates.policies.is_empty())
            .then(|| (model.gates.policies.len().to_string(), Tone::Idle)),
        Tab::Jobs => {
            (!model.jobs.jobs.is_empty()).then(|| (model.jobs.jobs.len().to_string(), Tone::Idle))
        }
        Tab::Overview if !model.setup.steps.is_empty() => {
            Some((model.setup.steps.len().to_string(), Tone::Warn))
        }
        Tab::Overview if model.setup.unavailable.is_some() => Some(("!".to_owned(), Tone::Bad)),
        Tab::Overview | Tab::Mounts | Tab::Access | Tab::Providers | Tab::Guided => None,
    }
}

pub(super) fn render_sidebar(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(theme.border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let width = usize::from(inner.width);
    let mut lines = vec![Line::raw("")];
    for group in TabGroup::ALL {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                group.title().to_uppercase(),
                theme.muted().add_modifier(Modifier::BOLD),
            ),
        ]));
        for tab in group.tabs() {
            let selected = tab == model.tab;
            let digit = tab.digit().map_or_else(|| "·".to_owned(), String::from);
            let mut spans = vec![
                Span::styled(if selected { "▌" } else { " " }, theme.accent()),
                Span::raw(" "),
                Span::styled(digit, if selected { theme.key() } else { theme.muted() }),
                Span::raw("  "),
                Span::styled(
                    tab.title(),
                    if selected {
                        theme.strong()
                    } else {
                        theme.soft()
                    },
                ),
            ];
            if let Some((meta, tone)) = tab_meta(model, tab) {
                let used = Line::from(spans.clone()).width() + meta.chars().count() + 1;
                spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
                spans.push(Span::styled(meta, theme.tone(tone)));
            }
            let line = Line::from(spans);
            lines.push(if selected {
                line.style(theme.selected())
            } else {
                line
            });
        }
        lines.push(Line::raw(""));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The narrow-terminal replacement for the sidebar: the current group's views in one
/// line. When they do not all fit, the strip shows the views around the selected one.
pub(super) fn render_tab_strip(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let group = model.tab.group();
    let title = Span::styled(
        format!(" {} ", group.title().to_uppercase()),
        theme.muted().add_modifier(Modifier::BOLD),
    );
    let tabs = group.tabs().collect::<Vec<_>>();
    let labels = tabs
        .iter()
        .map(|tab| {
            tab.digit().map_or_else(
                || format!(" {} ", tab.title()),
                |digit| format!(" {digit} {} ", tab.title()),
            )
        })
        .collect::<Vec<_>>();
    let selected = tabs.iter().position(|tab| *tab == model.tab).unwrap_or(0);
    let width = |range: std::ops::Range<usize>| {
        labels[range]
            .iter()
            .map(|label| label.chars().count())
            .sum::<usize>()
    };
    // Room for the group title, a space, and a marker on each side.
    let available = usize::from(area.width).saturating_sub(title.width() + 5);
    let (mut first, mut last) = (selected, selected + 1);
    loop {
        let grow_left = first > 0 && width(first - 1..last) <= available;
        if grow_left {
            first -= 1;
        }
        let grow_right = last < labels.len() && width(first..last + 1) <= available;
        if grow_right {
            last += 1;
        }
        if !grow_left && !grow_right {
            break;
        }
    }
    let mut spans = vec![
        title,
        Span::styled(if first > 0 { "‹ " } else { "  " }, theme.muted()),
    ];
    for (index, label) in labels.iter().enumerate().take(last).skip(first) {
        spans.push(Span::styled(
            label.clone(),
            if index == selected {
                theme.selected().patch(theme.key())
            } else {
                theme.soft()
            },
        ));
    }
    if last < labels.len() {
        spans.push(Span::styled(" ›", theme.muted()));
    }
    let hint = Span::styled("   [ ] groups", theme.muted());
    if Line::from(spans.clone()).width() + hint.width() <= usize::from(area.width) {
        spans.push(hint);
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(theme.border()),
        ),
        area,
    );
}

/// Whether the current screen is collecting typed input, which disables global keys.
fn typing(model: &Model) -> bool {
    if model.typing() || model.environments.switching.is_some() {
        return true;
    }
    match model.tab {
        Tab::Providers => matches!(
            model.providers.screen,
            ProviderScreen::ExternalSetup(_) | ProviderScreen::DataPath(_)
        ),
        Tab::Gates => matches!(model.gates.screen, GateScreen::Username(_)),
        _ => model.typing(),
    }
}

/// Keys that act on the current view and screen; shared by the footer and help overlay.
pub(super) fn view_hints(model: &Model) -> Vec<(&'static str, &'static str)> {
    // Until the session is here, keys only look around.
    if model.loading.is_active() && model.environments.switching.is_none() {
        return if model.tab == Tab::Environments {
            vec![("j/k", "select"), ("enter", "switch")]
        } else {
            vec![("j/k", "select")]
        };
    }
    settled_hints(model)
}

const SELECT: (&str, &str) = ("j/k", "select");
const SCROLL: (&str, &str) = ("j/k", "scroll");
const EDIT: [(&str, &str); 3] = [("a", "add"), ("e", "edit"), ("D", "remove")];

/// `view_hints` once the session is complete.
fn settled_hints(model: &Model) -> Vec<(&'static str, &'static str)> {
    if model.environments.switching.is_some() {
        return vec![("esc", "cancel")];
    }
    if let Some(keys) = overlay_hints(model) {
        return keys;
    }
    tab_hints(model)
}

/// The keys of whatever is open over the view: a form, a prompt, a menu, a
/// review, or a log or command.
fn overlay_hints(model: &Model) -> Option<Vec<(&'static str, &'static str)>> {
    if let Some(editor) = &model.editor {
        return Some(match editor.stage {
            EditorStage::Form => vec![
                ("tab", "next field"),
                ("enter", "review"),
                ("esc", "cancel"),
            ],
            EditorStage::Review { .. } => vec![("enter", "apply"), ("esc", "cancel")],
            EditorStage::Done { .. } => vec![("enter", "done"), ("esc", "close")],
        });
    }
    if model.run_prompt.is_some() {
        return Some(vec![("enter", "run"), ("↑↓", "history"), ("esc", "cancel")]);
    }
    if model.menu.is_some() {
        return Some(vec![SELECT, ("enter", "choose"), ("esc", "close")]);
    }
    if let Some(review) = model.review.as_ref().filter(|review| review.shown) {
        return Some(review_hints(review));
    }
    model.panel.as_ref().map(panel_hints)
}

fn review_hints(
    review: &crate::presentation::tui::review::Review,
) -> Vec<(&'static str, &'static str)> {
    use crate::presentation::tui::review::ReviewStage;
    match review.stage {
        ReviewStage::Reviewing => vec![
            SCROLL,
            (
                "enter",
                match (&review.review.rollback, review.review.remote) {
                    (Some(_), _) => "roll back",
                    (None, true) => "deploy",
                    (None, false) => "start",
                },
            ),
            ("esc", "cancel"),
        ],
        ReviewStage::Applying => vec![SCROLL, ("esc", "hide; u shows it again")],
        ReviewStage::Applied(_) | ReviewStage::Failed(_) => vec![SCROLL, ("enter", "close")],
    }
}

fn panel_hints(panel: &Panel) -> Vec<(&'static str, &'static str)> {
    match panel {
        Panel::Logs(logs) if logs.filtering => {
            vec![("type", "filter"), ("enter", "done"), ("esc", "clear")]
        }
        Panel::Logs(logs) => {
            let mut keys = vec![
                SCROLL,
                if logs.scroll_back == 0 {
                    ("space", "pause")
                } else {
                    ("space", "follow")
                },
                ("/", "filter"),
                ("esc", "close"),
                ("w", "wrap"),
                ("t", "timestamps"),
                ("c", "clear"),
            ];
            if !matches!(logs.state, StreamState::Streaming) {
                keys.insert(4, ("r", "reconnect"));
            }
            keys
        }
        Panel::Run(run) if run.state == RunState::Running => {
            vec![SCROLL, ("esc", "stop and close")]
        }
        Panel::Run(_) => vec![
            ("enter", "run another"),
            ("r", "run again"),
            SCROLL,
            ("esc", "close"),
        ],
    }
}

/// The keys of the view itself.
fn tab_hints(model: &Model) -> Vec<(&'static str, &'static str)> {
    // On a remote environment `u` deploys this commit, `b` rolls back, and
    // nothing stops the stack.
    let stack: &[(&str, &str)] = if model.remote().is_some() {
        &[("u", "deploy"), ("b", "roll back")]
    } else {
        &[("u", "start"), ("d", "stop")]
    };
    match model.tab {
        Tab::Overview => {
            let mut keys = if model.setup.steps.is_empty() {
                Vec::new()
            } else {
                vec![SELECT, ("enter", "run step")]
            };
            keys.extend_from_slice(stack);
            keys.push(("w", "guided setup"));
            keys
        }
        Tab::Tls if model.remote().is_some() => Vec::new(),
        Tab::Tls => vec![("w", "guided setup")],
        Tab::Access => {
            let mut keys = vec![
                SELECT,
                ("a", "add teammate"),
                ("e", "edit"),
                ("D", "remove"),
            ];
            if !model.access.team.iter().any(|member| member.you) {
                keys.push(("j", "request access"));
            }
            keys
        }
        Tab::Environments => {
            let mut keys = vec![SELECT, ("enter", "switch"), ("r", "check again")];
            keys.extend(EDIT);
            keys
        }
        Tab::Services => {
            let mut keys = vec![
                SELECT,
                ("enter", "actions"),
                ("l", "logs"),
                ("x", "run"),
                ("r", "restart"),
                ("v", "variables"),
                ("space", "fold"),
            ];
            if !model.snapshot.optional.is_empty() {
                keys.push(("o", "optional on/off"));
            }
            keys.extend(stack);
            keys
        }
        Tab::Variables => variable_hints(model, stack),
        Tab::Ingress | Tab::Diagnostics => vec![SELECT, ("pgup/pgdn", "page")],
        Tab::Mounts => vec![SELECT, ("c", "configure"), ("d", "disable")],
        Tab::Gates => gate_hints(&model.gates.screen),
        Tab::Providers => provider_hints(model),
        Tab::Jobs => {
            let mut keys = vec![("h/l", "pane"), SELECT];
            keys.extend(match model.jobs_pane {
                JobsPane::Jobs => vec![("r", "run")],
                JobsPane::Schedules => vec![("n", "run now"), ("space", "toggle")],
                JobsPane::Runs => vec![("c", "cancel"), ("y", "retry")],
            });
            if model.jobs_pane != JobsPane::Runs {
                keys.extend(EDIT);
            }
            keys.extend([("x", "reconcile"), ("t", "tick")]);
            keys
        }
        Tab::Guided => vec![
            ("g/i/p/n/e", "start"),
            ("enter", "continue"),
            ("b", "back"),
            ("r", "resume"),
            ("esc", "cancel"),
        ],
    }
}

fn variable_hints(
    model: &Model,
    stack: &[(&'static str, &'static str)],
) -> Vec<(&'static str, &'static str)> {
    if model.env.edit.is_some() {
        return vec![("enter", "save"), ("ctrl-u", "clear"), ("esc", "cancel")];
    }
    if model.env.searching {
        return vec![
            ("type", "filter"),
            ("↑↓", "select"),
            ("enter", "done"),
            ("esc", "clear"),
        ];
    }
    if model.host_values().is_some() {
        let mut keys = vec![SELECT, ("/", "search")];
        keys.extend([("enter", "set"), ("x", "stage removal"), ("X", "discard")]);
        if !model.env.filter.is_empty() {
            keys.push(("esc", "clear filter"));
        }
        keys.extend(stack);
        return keys;
    }
    if model.env.pane == VariablesPane::Services {
        let mut keys = vec![("j/k", "service"), ("l", "variables"), ("/", "search")];
        keys.push(("p", "problems"));
        if model.variable_search_active() {
            keys.push(("esc", "clear filter"));
        }
        keys.extend(stack);
        return keys;
    }
    let mut keys = vec![SELECT, ("h", "services"), ("/", "search")];
    if model.remote().is_none() {
        keys.extend([
            ("enter", "override"),
            ("x", "remove override"),
            ("c", "set in config"),
        ]);
    } else {
        keys.push(("enter", "set in config"));
    }
    keys.extend([("D", "remove from config"), ("p", "problems")]);
    if !model.env.filter.is_empty() || model.env.problems_only {
        keys.push(("esc", "clear filter"));
    }
    keys.extend(stack);
    keys
}

fn gate_hints(screen: &GateScreen) -> Vec<(&'static str, &'static str)> {
    match screen {
        GateScreen::List => vec![SELECT, ("a", "add user")],
        GateScreen::Username(_) => {
            vec![("type", "username"), ("enter", "review"), ("esc", "cancel")]
        }
        GateScreen::Review { .. } => {
            vec![("enter", "set password and apply"), ("esc", "cancel")]
        }
        GateScreen::Applying => vec![],
        GateScreen::Complete(_) | GateScreen::Error(_) | GateScreen::Cancelled => {
            vec![("enter", "back")]
        }
    }
}

fn provider_hints(model: &Model) -> Vec<(&'static str, &'static str)> {
    match &model.providers.screen {
        ProviderScreen::Dashboard => vec![SELECT],
        ProviderScreen::ExternalSetup(_) => vec![
            ("tab/↑↓", "field"),
            ("space", "bootstrap"),
            ("enter", "next"),
            ("esc", "cancel"),
        ],
        ProviderScreen::DataPath(_) => {
            vec![("type", "path"), ("enter", "review"), ("esc", "cancel")]
        }
        ProviderScreen::Review(_) => {
            vec![("enter", "apply"), ("b", "back"), ("c/esc", "cancel")]
        }
        ProviderScreen::Applying(_) => vec![],
        ProviderScreen::Complete(_) => vec![("enter", "back")],
        ProviderScreen::Error(_) if model.providers.last_review.is_some() => {
            vec![("r", "retry"), ("b", "back")]
        }
        ProviderScreen::Error(_) => vec![("b", "back")],
        ProviderScreen::Cancelled => vec![("r", "resume"), ("b", "back")],
    }
}

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let local = view_hints(model);
    let global = if !typing(model) {
        hints(&[("?", "help"), ("q", "quit")])
    } else if local.iter().any(|(key, _)| *key == "esc") {
        // The view already says what esc does here.
        Vec::new()
    } else {
        hints(&[("esc", "cancel")])
    };
    if let Some(notice) = &model.notice {
        let theme = theme();
        let tone = match notice.kind {
            NoticeKind::Success => Tone::Ok,
            NoticeKind::Info => Tone::Info,
            NoticeKind::Error => Tone::Bad,
        };
        let mut spans = vec![Span::raw(" ")];
        spans.extend(badge(tone, terminal_safe(&notice.text)));
        spans.push(Span::styled("   any key dismisses", theme.muted()));
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }
    let global = Line::from(global);
    let width = u16::try_from(global.width() + 1).unwrap_or(u16::MAX);
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(width)]).areas(area);
    let mut keys = vec![Span::raw(" ")];
    for (index, pair) in local.iter().enumerate() {
        let mut next = hints(std::slice::from_ref(pair));
        if index > 0 {
            next.insert(0, Span::raw("   "));
        }
        if Line::from(keys.clone()).width() + Line::from(next.clone()).width()
            > usize::from(left.width)
        {
            break;
        }
        keys.extend(next);
    }
    frame.render_widget(Paragraph::new(Line::from(keys)), left);
    frame.render_widget(Paragraph::new(global.right_aligned()), right);
}

pub(super) fn render_help(frame: &mut Frame<'_>, model: &Model) {
    let theme = theme();
    let row = |key: &str, label: &str| {
        Line::from(vec![
            Span::styled(format!("  {key:<14}"), theme.key()),
            Span::styled(label.to_owned(), theme.soft()),
        ])
    };
    let mut lines = vec![section("Navigation")];
    for (key, label) in [
        ("1-9, 0", "open view by number"),
        ("2, enter", "switch environment"),
        ("[ ]", "previous / next group"),
        ("tab, h/l", "next / previous view"),
        ("j/k, ↑↓", "move selection"),
        ("pgup/pgdn", "move by page"),
        ("home/end", "first / last row"),
        ("ctrl-r", "refresh status now"),
    ] {
        lines.push(row(key, label));
    }
    let local = view_hints(model)
        .into_iter()
        .filter(|(key, _)| !matches!(*key, "j/k" | "pgup/pgdn"))
        .collect::<Vec<_>>();
    if !local.is_empty() {
        lines.push(Line::raw(""));
        let reviewing = model.review.as_ref().is_some_and(|review| review.shown);
        lines.push(section(match &model.panel {
            _ if reviewing => "Review",
            Some(Panel::Logs(_)) => "Logs",
            Some(Panel::Run(_)) => "Run",
            None => model.tab.title(),
        }));
        for (key, label) in local {
            lines.push(row(key, label));
        }
    }
    lines.push(Line::raw(""));
    lines.push(section("General"));
    for (key, label) in [("?", "toggle help"), ("q", "quit"), ("ctrl-c", "interrupt")] {
        lines.push(row(key, label));
    }
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let area = centered_rect(52, height, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(with_meta(
            card("Keys", true),
            vec![Span::styled("esc close", theme.muted())],
        )),
        area,
    );
}

/// A yes/no dialog; destructive questions get a warning border.
pub(super) fn render_confirm(frame: &mut Frame<'_>, confirm: &Confirm) {
    let theme = theme();
    let mut lines = Vec::new();
    for line in &confirm.lines {
        lines.push(Line::styled(terminal_safe(line), theme.soft()));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(hints(&[
        ("enter/y", confirm.accept),
        ("esc/n", "cancel"),
    ])));
    let width = confirm
        .lines
        .iter()
        .map(|line| line.chars().count())
        .chain(std::iter::once(confirm.title.chars().count()))
        .max()
        .unwrap_or(0)
        .saturating_add(6)
        .clamp(40, 84);
    let area = centered_rect(
        u16::try_from(width).unwrap_or(84),
        u16::try_from(lines.len() + 2).unwrap_or(u16::MAX),
        frame.area(),
    );
    let tone = if confirm.destructive {
        Tone::Warn
    } else {
        Tone::Ok
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(card(terminal_safe(&confirm.title), true).border_style(theme.tone(tone)))
            .wrap(Wrap { trim: false }),
        area,
    );
}
