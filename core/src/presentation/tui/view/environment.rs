use ratatui::style::Style;
use ratatui::widgets::{HighlightSpacing, List, ListItem, ListState};

use super::*;
use crate::domain::environment::{
    EnvironmentStatusSnapshot, ServiceEnvironmentStatus, VariableStatus,
};
use crate::presentation::tui::model::{EnvironmentEdit, EnvironmentRow, VariablesPane};

/// Height of the selected-variable detail card under the table.
const DETAIL_HEIGHT: u16 = 5;
/// Table width from which the SOURCE column is shown.
const WIDE_TABLE: u16 = 64;
const SERVICE_LIST_MINIMUM: u16 = 12;
const SERVICE_LIST_MAXIMUM: u16 = 30;

pub(super) fn render_environment(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    if let Some(values) = model.host_values() {
        render_host_values(frame, area, model, values);
        return;
    }
    let Some(environment) = &model.environment else {
        empty_state(
            frame,
            area,
            card("Variables", true),
            Tone::Warn,
            "Variables are unavailable",
            if model.remote().is_some() {
                "This checkout differs from what the host has deployed, so its values would not match."
            } else {
                ""
            },
        );
        return;
    };
    let total = model.all_environment_rows();
    let rows = model.environment_rows();
    let block = with_meta(
        card("Variables", true),
        variables_meta(model, environment, &total, &rows),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [search, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    render_search(frame, search, model);
    let services = model.variable_services();
    if services.is_empty() {
        empty_state(
            frame,
            body,
            Block::new(),
            Tone::Idle,
            "No variables declared",
            "",
        );
        return;
    }
    let [list, _, content] = Layout::horizontal([
        Constraint::Length(service_list_width(&services, &total, body.width)),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(body);
    render_service_list(frame, list, model, (&total, &rows));
    render_variables(frame, content, model, &rows);
}

fn variables_meta(
    model: &Model,
    environment: &EnvironmentStatusSnapshot,
    total: &[EnvironmentRow<'_>],
    rows: &[EnvironmentRow<'_>],
) -> Vec<Span<'static>> {
    let theme = theme();
    let problems = total.iter().filter(|row| row.is_problem()).count();
    let mut meta = vec![Span::styled(
        if model.variable_search_active() {
            format!("{} of {}", rows.len(), plural(total.len(), "variable"))
        } else {
            plural(total.len(), "variable")
        },
        theme.muted(),
    )];
    if problems > 0 {
        meta.push(Span::raw("  "));
        meta.extend(badge(Tone::Bad, plural(problems, "problem")));
    }
    if model.env.pending_restart {
        meta.push(Span::raw("  "));
        meta.extend(badge(Tone::Warn, "restart to apply · u"));
    }
    if model.remote().is_some() {
        meta.push(Span::raw("  "));
        meta.extend(badge(Tone::Info, "from this checkout"));
    }
    meta.push(Span::styled(
        format!("  {} delivery", environment.delivery.mode),
        theme.muted(),
    ));
    meta
}

/// The table of the chosen service, or of every match, with the selected
/// variable's details under it.
fn render_variables(frame: &mut Frame<'_>, area: Rect, model: &Model, rows: &[EnvironmentRow<'_>]) {
    if rows.is_empty() {
        let (title, detail) = if model.variable_search_active() {
            (
                "No variables match".to_owned(),
                "esc clears the search and filters",
            )
        } else {
            let service = model
                .variable_services()
                .get(model.env.service)
                .map_or_else(String::new, |service| terminal_safe(&service.service));
            (format!("{service} declares no variables"), "")
        };
        empty_state(frame, area, Block::new(), Tone::Idle, &title, detail);
        return;
    }
    let remote = model.remote().is_some();
    let selected = rows.get(model.scroll).filter(|_| area.height >= 14);
    let height = selected.map_or(0, |row| detail_height(row, remote, area.width));
    let [table, detail] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(height)]).areas(area);
    render_variable_table(frame, table, model, rows);
    if let Some(row) = selected {
        render_variable_detail(frame, detail, row, remote);
    }
}

fn render_search(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let searching = model.env.searching;
    let mut spans = vec![Span::styled(
        "/ ",
        if searching {
            theme.key()
        } else {
            theme.muted()
        },
    )];
    if model.env.filter.is_empty() && !searching {
        spans.push(Span::styled(
            "search all services by name, key, value, or status",
            theme.muted(),
        ));
    } else {
        spans.push(Span::styled(terminal_safe(&model.env.filter), theme.text()));
    }
    if searching {
        spans.push(Span::styled("█", theme.accent()));
    }
    if model.env.problems_only {
        spans.push(Span::raw("   "));
        spans.extend(badge(Tone::Bad, "problems only"));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(if searching {
                    theme.border_focus()
                } else {
                    theme.border()
                }),
        ),
        area,
    );
}

/// How a service is named in the list: with its stack when another stack
/// has a service of the same name.
fn service_label(
    services: &[&ServiceEnvironmentStatus],
    service: &ServiceEnvironmentStatus,
) -> String {
    let shared = services
        .iter()
        .filter(|other| other.service == service.service)
        .count()
        > 1;
    if shared {
        format!(
            "{} · {}",
            terminal_safe(&service.service),
            terminal_safe(&service.stack)
        )
    } else {
        terminal_safe(&service.service)
    }
}

/// The count and problem badge beside a service in the list.
fn service_badge(count: usize, problems: usize, searching: bool) -> Vec<Span<'static>> {
    let theme = theme();
    let mut spans = Vec::new();
    if count > 0 || !searching {
        spans.push(Span::styled(
            count.to_string(),
            if searching {
                theme.text()
            } else {
                theme.muted()
            },
        ));
    }
    if problems > 0 {
        spans.push(Span::styled(
            format!(" {}{problems}", Tone::Bad.glyph()),
            theme.tone(Tone::Bad),
        ));
    }
    spans
}

fn problems_in(total: &[EnvironmentRow<'_>], group: usize) -> usize {
    total
        .iter()
        .filter(|row| row.group == group && row.is_problem())
        .count()
}

fn service_list_width(
    services: &[&ServiceEnvironmentStatus],
    total: &[EnvironmentRow<'_>],
    width: u16,
) -> u16 {
    let longest = services
        .iter()
        .enumerate()
        .map(|(group, service)| {
            let badge = service_badge(service.variables.len(), problems_in(total, group), false);
            Line::raw(service_label(services, service)).width() + Line::from(badge).width()
        })
        .max()
        .unwrap_or(0);
    let wanted = u16::try_from(longest + 5).unwrap_or(u16::MAX);
    let most = (width / 9 * 2).clamp(SERVICE_LIST_MINIMUM, SERVICE_LIST_MAXIMUM);
    wanted.clamp(SERVICE_LIST_MINIMUM, most)
}

/// The services down the left; the chosen one filters the table.
fn render_service_list(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    (total, shown): (&[EnvironmentRow<'_>], &[EnvironmentRow<'_>]),
) {
    let theme = theme();
    let focused = model.env.pane == VariablesPane::Services;
    let searching = model.variable_search_active();
    let services = model.variable_services();
    let counted = if searching { shown } else { total };
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(theme.border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [header, list] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(Line::styled(
            " SERVICES",
            if focused {
                theme.key().add_modifier(Modifier::BOLD)
            } else {
                theme.muted().add_modifier(Modifier::BOLD)
            },
        )),
        header,
    );
    let width = usize::from(list.width.saturating_sub(2));
    let items = services.iter().enumerate().map(|(group, service)| {
        let count = counted.iter().filter(|row| row.group == group).count();
        let badge = service_badge(count, problems_in(total, group), searching);
        let badge_width = Line::from(badge.clone()).width();
        let name_width = width.saturating_sub(badge_width + usize::from(badge_width > 0));
        let name = ellipsize(&service_label(&services, service), name_width);
        let dim = searching && count == 0;
        let mut spans = vec![Span::styled(
            name.clone(),
            if dim { theme.muted() } else { theme.text() },
        )];
        let used = Line::raw(name).width() + badge_width;
        spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
        spans.extend(badge);
        ListItem::new(Line::from(spans))
    });
    let mut state = ListState::default().with_selected(Some(model.env.service));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(if focused {
                theme.selected()
            } else {
                Style::new().add_modifier(Modifier::BOLD)
            })
            .highlight_symbol(if focused {
                selection_symbol()
            } else {
                Line::styled("▌", theme.border())
            })
            .highlight_spacing(HighlightSpacing::Always),
        list,
        &mut state,
    );
}

/// `text` cut to `width` columns, ending in an ellipsis when cut.
fn ellipsize(text: &str, width: usize) -> String {
    if Line::raw(text).width() <= width {
        return text.to_owned();
    }
    let mut cut = String::new();
    for character in text.chars() {
        let mut next = cut.clone();
        next.push(character);
        if Line::raw(next.as_str()).width() + 1 > width {
            break;
        }
        cut = next;
    }
    if width > 0 {
        cut.push('…');
    }
    cut
}

fn variable_columns(width: u16) -> Vec<Constraint> {
    if width >= WIDE_TABLE {
        vec![
            Constraint::Percentage(30),
            Constraint::Length(13),
            Constraint::Length(16),
            Constraint::Fill(1),
        ]
    } else {
        vec![
            Constraint::Percentage(40),
            Constraint::Length(13),
            Constraint::Fill(1),
        ]
    }
}

fn render_variable_table(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    rows: &[EnvironmentRow<'_>],
) {
    let theme = theme();
    let wide = area.width >= WIDE_TABLE;
    let columns = variable_columns(area.width);
    let name_width = Layout::horizontal(columns.clone())
        .spacing(2)
        .split(Rect::new(0, 0, area.width.saturating_sub(1), 1))[0]
        .width;
    let grouped = model.variable_search_active();
    let services = model.variable_services();
    let mut table_rows = Vec::new();
    let mut selected = None;
    for (position, row) in rows.iter().enumerate() {
        if grouped && row.first {
            table_rows.push(Row::new([Cell::from(Span::styled(
                ellipsize(
                    &service_label(&services, row.service),
                    usize::from(name_width),
                ),
                theme.strong(),
            ))]));
        }
        if position == model.scroll {
            selected = Some(table_rows.len());
        }
        let variable = row.variable;
        let indent = if grouped { "  " } else { "" };
        let mut cells = vec![
            Cell::from(Span::styled(
                format!("{indent}{}", terminal_safe(&variable.name)),
                theme.text(),
            )),
            Cell::from(Line::from(badge(
                environment_tone(variable.status),
                environment_status(variable.status),
            ))),
        ];
        if wide {
            cells.push(Cell::from(Span::styled(
                environment_source(variable.source),
                if variable.source == ValueSource::SqliteOverride {
                    theme.accent()
                } else {
                    theme.muted()
                },
            )));
        }
        cells.push(Cell::from(variable_value(variable)));
        table_rows.push(Row::new(cells));
    }
    let header: &[&str] = if wide {
        &["VARIABLE", "STATUS", "SOURCE", "VALUE"]
    } else {
        &["VARIABLE", "STATUS", "VALUE"]
    };
    let count = table_rows.len();
    let mut table = selectable_table(Table::new(table_rows, columns).header(table_header(header)));
    if model.env.pane == VariablesPane::Services {
        table = table
            .row_highlight_style(Style::new().add_modifier(Modifier::BOLD))
            .highlight_symbol(Line::styled("▌", theme.border()));
    }
    let mut state = TableState::default().with_selected(selected);
    frame.render_stateful_widget(table, area, &mut state);
    scrollbar(frame, area, count, state.offset());
}

fn variable_value(variable: &VariableStatus) -> Span<'static> {
    let theme = theme();
    if variable.sensitive {
        Span::styled("••••••", theme.muted())
    } else {
        variable.value.as_deref().map_or_else(
            || Span::styled("—", theme.muted()),
            |value| Span::styled(terminal_safe(value), theme.text()),
        )
    }
}

fn variable_action(variable: &VariableStatus, remote: bool) -> &'static str {
    match (&variable.global_key, variable.sensitive) {
        (Some(_), _) if remote => {
            "enter sets it in the configuration for this environment; commit and deploy to apply"
        }
        (None, _) => "edit the module template to change it",
        (Some(_), true) => "enter sets the secret (hidden input)",
        (Some(_), false) if variable.source == ValueSource::SqliteOverride => {
            "enter edits the override · x removes it · c sets it in the configuration"
        }
        (Some(_), false) => "enter sets a local override · c sets it in the configuration",
    }
}

/// Rows the detail card needs at `width`, its value and action lines wrapped.
fn detail_height(row: &EnvironmentRow<'_>, remote: bool, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    let value = 10 + variable_value(row.variable).width();
    let action = Line::raw(variable_action(row.variable, remote)).width();
    let wrapped = value.div_ceil(width) + action.div_ceil(width);
    DETAIL_HEIGHT
        + u16::from(row.variable.source == ValueSource::Secret)
        + u16::try_from(wrapped.saturating_sub(2)).unwrap_or(0)
}

/// What the selected variable is, where it comes from, and what pressing enter does.
fn render_variable_detail(
    frame: &mut Frame<'_>,
    area: Rect,
    row: &EnvironmentRow<'_>,
    remote: bool,
) {
    let theme = theme();
    let variable = row.variable;
    let width = 10;
    let mut key = vec![Span::styled(
        variable
            .global_key
            .as_deref()
            .map_or_else(|| "none (template only)".to_owned(), terminal_safe),
        theme.text(),
    )];
    if variable.required {
        key.push(Span::styled("  required", theme.muted()));
    }
    if let Some(provider) = &variable.provider {
        key.push(Span::styled(
            format!("  provider {}", terminal_safe(provider)),
            theme.muted(),
        ));
    }
    let location = &variable.location;
    let mut lines = vec![
        kv("Value", vec![variable_value(variable)], width),
        kv("Key", key, width),
    ];
    if variable.source == ValueSource::Secret {
        lines.push(kv_text(
            "Set",
            &crate::presentation::output::set_summary(
                variable.set_at_unix_ms,
                variable.set_by.as_deref(),
                chrono::Utc::now().timestamp_millis(),
            ),
            width,
        ));
    }
    lines.extend([
        kv_text(
            "Template",
            &format!("{}:{}", location.path.display(), location.line),
            width,
        ),
        Line::styled(variable_action(variable, remote), theme.muted()),
    ]);
    let title = format!(
        "{} · {}",
        terminal_safe(&variable.name),
        terminal_safe(&row.service.service)
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::new()
                    .borders(Borders::TOP)
                    .border_style(theme.border())
                    .title(Span::styled(format!(" {title} "), theme.strong())),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The override editor, drawn over the Environment view.
pub(super) fn render_environment_edit(frame: &mut Frame<'_>, edit: &EnvironmentEdit, remote: bool) {
    let theme = theme();
    let area = centered_rect(72, 10, frame.area());
    let width = usize::from(area.width.saturating_sub(6));
    let value = terminal_safe(&edit.value);
    let shown = value
        .chars()
        .skip(
            value
                .chars()
                .count()
                .saturating_sub(width.saturating_sub(1)),
        )
        .collect::<String>();
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled(terminal_safe(&edit.key), theme.strong()),
                Span::styled(
                    format!("  used by {}", plural(edit.usages, "variable")),
                    theme.muted(),
                ),
            ]),
            Line::styled(
                if remote {
                    "Staged on every host; live after the next deployment."
                } else {
                    "Saved as a workspace-local override; never written to mm.yaml."
                },
                theme.muted(),
            ),
            Line::raw(""),
            Line::from(vec![
                Span::styled(shown, theme.text()),
                Span::styled("█", theme.accent()),
            ]),
            Line::raw(""),
            Line::from(hints(&[
                ("enter", "save"),
                ("ctrl-u", "clear"),
                ("ctrl-w", "delete word"),
                ("esc", "cancel"),
            ])),
        ])
        .block(card("Set override", true)),
        area,
    );
}

pub(super) fn render_diagnostics(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let diagnostics = model.diagnostics();
    let block = with_meta(
        card("Diagnostics", true),
        vec![Span::styled(
            plural(diagnostics.len(), "finding"),
            theme.muted(),
        )],
    );
    if diagnostics.is_empty() {
        empty_state(
            frame,
            area,
            block,
            Tone::Ok,
            "No issues",
            "Configuration, runtime, and environment checks passed.",
        );
        return;
    }
    let width = usize::from(block.inner(area).width.saturating_sub(4));
    let items = diagnostics.iter().map(|diagnostic| {
        let tone = severity_tone(diagnostic.severity);
        let mut lines = vec![Line::from(vec![
            Span::styled(tone.glyph(), theme.tone(tone)),
            Span::raw(" "),
            Span::styled(terminal_safe(&diagnostic.message), theme.strong()),
        ])];
        lines.push(Line::styled(
            format!("  {}", terminal_safe(&diagnostic.code)),
            theme.tone(tone),
        ));
        if let Some(help) = &diagnostic.help {
            for chunk in wrap_text(&terminal_safe(help), width) {
                lines.push(Line::styled(format!("  {chunk}"), theme.soft()));
            }
        }
        lines.push(Line::raw(""));
        ListItem::new(lines)
    });
    let mut state = ListState::default().with_selected(Some(model.scroll));
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme.selected())
            .highlight_symbol(selection_symbol())
            .highlight_spacing(HighlightSpacing::Always),
        area,
        &mut state,
    );
}

/// Greedy word wrap for list items, which ratatui does not wrap on its own.
pub(super) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}
