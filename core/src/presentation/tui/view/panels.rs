use super::*;
use crate::presentation::tui::panel::{
    LogsPanel, Panel, RunPanel, RunPrompt, RunState, ServiceMenu, StreamState,
};

/// Columns before the text of an output line: the stream marker and a space.
pub(super) const GUTTER: usize = 2;
/// Width of the action menu, borders included.
pub(super) const MENU_WIDTH: u16 = 64;

pub(super) fn render_panel(frame: &mut Frame<'_>, area: Rect, model: &Model, panel: &Panel) {
    match panel {
        Panel::Logs(logs) => render_logs(frame, area, model, logs),
        Panel::Run(run) => render_run(frame, area, model, run),
    }
}

/// Where the panel's target runs, for its title bar.
fn place(model: &Model) -> Vec<Span<'static>> {
    let theme = theme();
    vec![Span::styled(
        match model.remote() {
            Some(target) => format!(
                "{} · remote · {}",
                terminal_safe(&model.snapshot.environment),
                terminal_safe(target)
            ),
            None => format!("{} · local", terminal_safe(&model.snapshot.environment)),
        },
        theme.muted(),
    )]
}

/// Tints lines that name their own severity, so problems stand out while scrolling.
pub(super) fn line_style(text: &str) -> ratatui::style::Style {
    let theme = theme();
    let lowered = text.to_lowercase();
    if ["error", "fatal", "panic", "exception"]
        .iter()
        .any(|word| lowered.contains(word))
    {
        theme.tone(Tone::Bad)
    } else if lowered.contains("warn") {
        theme.tone(Tone::Warn)
    } else {
        theme.text()
    }
}

/// Splits `text` into rows of at most `width` characters; one clipped row when
/// wrapping is off.
pub(super) fn rows_of(text: &str, width: usize, wrap: bool) -> Vec<String> {
    let characters = text.chars().collect::<Vec<_>>();
    if characters.is_empty() {
        return vec![String::new()];
    }
    let width = width.max(1);
    if !wrap {
        return vec![characters.iter().take(width).collect()];
    }
    characters
        .chunks(width)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

/// One output line as screen rows: a marker for stderr, an optional prefix on
/// the first row, and aligned continuation rows.
fn output_rows(
    text: &str,
    stderr: bool,
    prefix: &str,
    width: usize,
    wrap: bool,
) -> Vec<Line<'static>> {
    let theme = theme();
    let style = line_style(text);
    let indent = prefix.chars().count();
    let available = width.saturating_sub(GUTTER + indent);
    rows_of(&terminal_safe(text), available, wrap)
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            Line::from(vec![
                Span::styled(if stderr { "▎ " } else { "  " }, theme.tone(Tone::Warn)),
                Span::styled(
                    if index == 0 {
                        prefix.to_owned()
                    } else {
                        " ".repeat(indent)
                    },
                    theme.muted(),
                ),
                Span::styled(row, style),
            ])
        })
        .collect()
}

/// Fills `height` rows from the newest line upward, so the view ends at `end`.
fn tail_rows<'a, T: 'a>(
    lines: impl DoubleEndedIterator<Item = &'a T>,
    height: usize,
    to_rows: impl Fn(&T) -> Vec<Line<'static>>,
) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    for line in lines.rev() {
        rows.extend(to_rows(line).into_iter().rev());
        if rows.len() >= height {
            break;
        }
    }
    rows.truncate(height);
    rows.reverse();
    rows
}

/// `HH:MM:SS` from an RFC 3339 timestamp, or the value itself when it is not one.
pub(super) fn clock_time(timestamp: &str) -> String {
    match timestamp.split_once('T') {
        Some((_, time)) if time.len() >= 8 => time.chars().take(8).collect(),
        _ => terminal_safe(timestamp),
    }
}

/// What the log view says beside its title: whether it follows, how many
/// lines it holds, and where they come from.
fn logs_meta(model: &Model, logs: &LogsPanel, visible: usize) -> Vec<Span<'static>> {
    let theme = theme();
    let mut meta = Vec::new();
    meta.extend(match &logs.state {
        StreamState::Connecting => vec![
            Span::styled(spinner(model.tick), theme.accent()),
            Span::styled(" connecting", theme.accent()),
        ],
        StreamState::Streaming if logs.scroll_back == 0 => badge(Tone::Ok, "following"),
        StreamState::Streaming => badge(
            Tone::Warn,
            if logs.unseen == 0 {
                "paused".to_owned()
            } else {
                format!("paused · {} new", logs.unseen)
            },
        ),
        StreamState::Ended => badge(Tone::Idle, "stream ended"),
        StreamState::Failed(_) => badge(Tone::Bad, "stream failed"),
    });
    meta.push(Span::styled(
        if logs.filter.is_empty() {
            format!("   {}", plural(visible, "line"))
        } else {
            format!("   {visible} of {}", plural(logs.lines.len(), "line"))
        },
        theme.muted(),
    ));
    meta.push(Span::raw("   "));
    meta.extend(place(model));
    meta
}

fn render_logs(frame: &mut Frame<'_>, area: Rect, model: &Model, logs: &LogsPanel) {
    let theme = theme();
    let visible = logs.visible();
    let block = with_meta(
        logs_card(model, logs),
        logs_meta(model, logs, visible.len()),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let show_filter = logs.filtering || !logs.filter.is_empty();
    let failure = match &logs.state {
        StreamState::Failed(message) => Some(message.as_str()),
        _ => None,
    };
    let [filter, body, status] = Layout::vertical([
        Constraint::Length(u16::from(show_filter) * 2),
        Constraint::Min(0),
        Constraint::Length(u16::from(failure.is_some()) * 2),
    ])
    .areas(inner);
    if show_filter {
        let mut spans = vec![
            Span::styled(
                "/ ",
                if logs.filtering {
                    theme.key()
                } else {
                    theme.muted()
                },
            ),
            Span::styled(terminal_safe(&logs.filter), theme.text()),
        ];
        if logs.filtering {
            spans.push(Span::styled("█", theme.accent()));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).block(
                Block::new()
                    .borders(Borders::BOTTOM)
                    .border_style(if logs.filtering {
                        theme.border_focus()
                    } else {
                        theme.border()
                    }),
            ),
            filter,
        );
    }
    if let Some(message) = failure {
        frame.render_widget(
            Paragraph::new(Line::from(badge(Tone::Bad, terminal_safe(message))))
                .block(
                    Block::new()
                        .borders(Borders::TOP)
                        .border_style(theme.border()),
                )
                .wrap(Wrap { trim: true }),
            status,
        );
    }
    if visible.is_empty() {
        let (tone, title, detail) = match &logs.state {
            StreamState::Connecting => (Tone::Info, "Connecting…", ""),
            _ if !logs.filter.is_empty() => (Tone::Idle, "No lines match", "esc clears the filter"),
            StreamState::Streaming => (Tone::Ok, "Waiting for output", ""),
            StreamState::Ended | StreamState::Failed(_) => {
                (Tone::Idle, "No log lines", "r connects again")
            }
        };
        empty_state(frame, body, Block::new(), tone, title, detail);
        return;
    }
    let end = visible.len().saturating_sub(logs.scroll_back);
    let width = usize::from(body.width);
    let height = usize::from(body.height);
    let columns = LogColumns::new(model, logs);
    let rows = tail_rows(visible[..end].iter(), height, |line| {
        columns.rows(line, logs.timestamps, width, logs.wrap)
    });
    frame.render_widget(Paragraph::new(rows), body);
    scrollbar(frame, area, visible.len(), end.saturating_sub(height));
}

fn render_run(frame: &mut Frame<'_>, area: Rect, model: &Model, run: &RunPanel) {
    let theme = theme();
    let mut meta = match &run.state {
        RunState::Running => vec![
            Span::styled(spinner(model.tick), theme.accent()),
            Span::styled(" running", theme.accent()),
        ],
        RunState::Exited(0) => badge(Tone::Ok, "exit 0"),
        RunState::Exited(code) => badge(Tone::Bad, format!("exit {code}")),
        RunState::Failed(_) => badge(Tone::Bad, "failed"),
    };
    meta.push(Span::raw("   "));
    meta.extend(place(model));
    let block = with_meta(
        card(format!("Run · {}", terminal_safe(&run.target.label)), true),
        meta,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let failure = match &run.state {
        RunState::Failed(message) => Some(message.as_str()),
        _ => None,
    };
    let [command, body, status] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(u16::from(failure.is_some()) * 3),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("$ ", theme.key()),
            Span::styled(terminal_safe(&run.command), theme.strong()),
        ]))
        .block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(theme.border()),
        ),
        command,
    );
    if let Some(message) = failure {
        frame.render_widget(
            Paragraph::new(Line::from(badge(Tone::Bad, terminal_safe(message))))
                .block(
                    Block::new()
                        .borders(Borders::TOP)
                        .border_style(theme.border()),
                )
                .wrap(Wrap { trim: true }),
            status,
        );
    }
    if run.lines.is_empty() {
        let (tone, title) = match &run.state {
            RunState::Running => (Tone::Info, "Waiting for output"),
            RunState::Exited(_) => (Tone::Idle, "The command printed nothing"),
            RunState::Failed(_) => (Tone::Bad, "The command did not run"),
        };
        empty_state(frame, body, Block::new(), tone, title, "");
        return;
    }
    let end = run.lines.len().saturating_sub(run.scroll_back);
    let width = usize::from(body.width);
    let height = usize::from(body.height);
    let rows = tail_rows(run.lines.iter().take(end), height, |line| {
        output_rows(&line.text, line.stderr, "", width, true)
    });
    frame.render_widget(Paragraph::new(rows), body);
    scrollbar(frame, area, run.lines.len(), end.saturating_sub(height));
}

/// The action menu, opened beside the selected row when it fits.
pub(super) fn render_menu(frame: &mut Frame<'_>, menu: &ServiceMenu, anchor: Option<(u16, u16)>) {
    let theme = theme();
    let lines = menu
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let selected = index == menu.selected;
            let available = item.disabled.is_none();
            let line = Line::from(vec![
                Span::styled(if selected { "▌ " } else { "  " }, theme.accent()),
                Span::styled(
                    format!("{}  ", item.key),
                    if available {
                        theme.key()
                    } else {
                        theme.muted()
                    },
                ),
                Span::styled(
                    format!("{:<16}", item.label),
                    if available {
                        theme.strong()
                    } else {
                        theme.muted()
                    },
                ),
                Span::styled(
                    if available {
                        item.detail.clone()
                    } else {
                        "not available here".to_owned()
                    },
                    theme.muted(),
                ),
            ]);
            if selected {
                line.style(theme.selected())
            } else {
                line
            }
        })
        .collect::<Vec<_>>();
    let screen = frame.area();
    let width = MENU_WIDTH.min(screen.width.saturating_sub(2)).max(1);
    let height = u16::try_from(lines.len() + 2)
        .unwrap_or(u16::MAX)
        .min(screen.height.saturating_sub(2))
        .max(1);
    let area = match anchor {
        // Below the row, or above it near the bottom of the screen.
        Some((x, y)) => {
            let x = x.min(screen.right().saturating_sub(width + 1));
            let y = if y + 1 + height < screen.bottom() {
                y + 1
            } else {
                y.saturating_sub(height)
            };
            Rect::new(x, y, width, height)
        }
        None => centered_rect(width, height, screen),
    };
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(with_meta(
            card(terminal_safe(&menu.target.label), true),
            vec![Span::styled("esc close", theme.muted())],
        )),
        area,
    );
}

/// The command line being typed, drawn over the current view.
pub(super) fn render_run_prompt(frame: &mut Frame<'_>, prompt: &RunPrompt) {
    let theme = theme();
    let area = centered_rect(76, 8, frame.area());
    let width = usize::from(area.width.saturating_sub(8));
    let input = terminal_safe(&prompt.input);
    let shown = input
        .chars()
        .skip(
            input
                .chars()
                .count()
                .saturating_sub(width.saturating_sub(1)),
        )
        .collect::<String>();
    let mut target = vec![Span::styled(
        terminal_safe(&prompt.target.service),
        theme.strong(),
    )];
    if prompt.target.container.is_some() {
        target.push(Span::styled(
            format!("  container {}", terminal_safe(&prompt.target.label)),
            theme.muted(),
        ));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(target),
            Line::styled(
                "Runs the exact arguments: no shell, no TTY. For pipes use sh -c '…'.",
                theme.muted(),
            ),
            Line::raw(""),
            Line::from(vec![
                Span::styled("$ ", theme.key()),
                Span::styled(shown, theme.text()),
                Span::styled("█", theme.accent()),
            ]),
            match &prompt.error {
                Some(error) => Line::from(badge(Tone::Bad, terminal_safe(error))),
                None => Line::raw(""),
            },
            Line::from(hints(&[
                ("enter", "run"),
                ("↑↓", "history"),
                ("ctrl-u", "clear"),
                ("esc", "cancel"),
            ])),
        ])
        .block(card("Run a command", true)),
        area,
    );
}

/// Shown while another environment's session is being built.
pub(super) fn render_switching(frame: &mut Frame<'_>, model: &Model, environment: &str) {
    let theme = theme();
    let remote = model
        .environments
        .entries
        .iter()
        .find(|entry| entry.name == environment)
        .and_then(|entry| entry.remote.as_deref());
    let detail = match remote {
        Some(host) => format!("Connecting to {}…", terminal_safe(host)),
        None => "Reading its status…".to_owned(),
    };
    let area = centered_rect(56, 6, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(spinner(model.tick), theme.accent()),
                Span::styled(
                    if model.environments.reloading {
                        format!(" Checking {}", terminal_safe(environment))
                    } else {
                        format!(" Switching to {}", terminal_safe(environment))
                    },
                    theme.strong(),
                ),
            ]),
            Line::styled(detail, theme.muted()),
            Line::raw(""),
            Line::from(hints(&[("esc", "cancel")])),
        ])
        .block(card("Environment", true)),
        area,
    );
}
