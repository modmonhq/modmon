use super::*;
use crate::domain::staged::{HostValue, RemoteValues, ValueKind, ValueOrigin};
use crate::presentation::tui::host_values::HostRow;

fn short(revision: &str) -> String {
    revision.chars().take(8).collect()
}

fn live(value: &HostValue) -> Span<'static> {
    let theme = theme();
    match (&value.active, value.origin) {
        (_, Some(ValueOrigin::ExternalStore)) => Span::styled(
            format!("in {}", value.provider.as_deref().unwrap_or("its store")),
            theme.muted(),
        ),
        (Some(active), _) => Span::styled(short(&active.revision), theme.text()),
        (None, _) if value.defaulted => Span::styled("default", theme.muted()),
        (None, _) if value.required => Span::styled("unset", theme.tone(Tone::Bad)),
        (None, _) => Span::styled("unset", theme.muted()),
    }
}

fn staged(row: &HostRow<'_>) -> Span<'static> {
    let theme = theme();
    let revisions = row
        .nodes
        .iter()
        .map(|(_, value)| value.staged.as_ref().map(|staged| &staged.revision))
        .collect::<std::collections::BTreeSet<_>>();
    if revisions.len() > 1 {
        return Span::styled("nodes differ", theme.tone(Tone::Bad));
    }
    match row.value().and_then(|value| value.staged.as_ref()) {
        Some(staged) if staged.removal => Span::styled(
            format!("- by {}", terminal_safe(&staged.staged_by)),
            theme.tone(Tone::Warn),
        ),
        Some(staged) => Span::styled(
            format!(
                "{} by {}",
                short(&staged.revision),
                terminal_safe(&staged.staged_by)
            ),
            theme.tone(Tone::Warn),
        ),
        None => Span::styled("—", theme.muted()),
    }
}

/// The Variables view of a remote environment: what its hosts hold.
pub(super) fn render_host_values(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    values: &RemoteValues,
) {
    let theme = theme();
    let rows = model.host_value_rows();
    let staged_count = rows
        .iter()
        .filter(|row| row.value().is_some_and(|value| value.staged.is_some()))
        .count();
    let mut meta = vec![Span::styled(plural(rows.len(), "key"), theme.muted())];
    if staged_count > 0 {
        meta.push(Span::raw("  "));
        meta.extend(badge(
            Tone::Warn,
            format!("{staged_count} staged · deploy redeploy"),
        ));
    }
    if !values.unreachable.is_empty() {
        meta.push(Span::raw("  "));
        meta.extend(badge(
            Tone::Bad,
            format!(
                "{} did not answer",
                plural(values.unreachable.len(), "host")
            ),
        ));
    }
    meta.push(Span::styled("  on the hosts", theme.muted()));
    let block = with_meta(card("Variables", true), meta);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let detail_height = if inner.height >= 16 { 6 } else { 0 };
    let [table, detail] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(detail_height)]).areas(inner);
    if rows.is_empty() {
        empty_state(
            frame,
            table,
            Block::new(),
            Tone::Idle,
            "No secret or variable to set on the hosts",
            "declare one under secrets: or use a variable in a template, then deploy",
        );
        return;
    }
    let table_rows = rows.iter().map(|row| {
        let value = row.value();
        Row::new([
            Cell::from(Span::styled(terminal_safe(row.key), theme.text())),
            Cell::from(Span::styled(row.kind.as_str(), theme.muted())),
            Cell::from(value.map_or_else(|| Span::raw(""), live)),
            Cell::from(staged(row)),
            Cell::from(Span::styled(
                value
                    .map(|value| value.services.join(", "))
                    .unwrap_or_default(),
                theme.muted(),
            )),
        ])
    });
    render_table(
        frame,
        table,
        Table::new(
            table_rows,
            [
                Constraint::Percentage(30),
                Constraint::Length(9),
                Constraint::Length(16),
                Constraint::Length(22),
                Constraint::Min(8),
            ],
        )
        .header(table_header(&["KEY", "KIND", "LIVE", "STAGED", "SERVICES"])),
        rows.len(),
        Some(model.scroll),
    );
    if detail_height > 0
        && let Some(row) = rows.get(model.scroll)
    {
        render_host_detail(frame, detail, row);
    }
}

fn render_host_detail(frame: &mut Frame<'_>, area: Rect, row: &HostRow<'_>) {
    let theme = theme();
    let mut lines = row
        .nodes
        .iter()
        .map(|(node, value)| {
            let mut spans = vec![live(value)];
            if let Some(by) = value
                .active
                .as_ref()
                .and_then(|active| active.changed_by.as_ref())
            {
                spans.push(Span::styled(
                    format!(" by {}", terminal_safe(by)),
                    theme.muted(),
                ));
            }
            if let Some(staged) = &value.staged {
                spans.push(Span::styled(
                    if staged.removal {
                        format!("  · removal staged by {}", terminal_safe(&staged.staged_by))
                    } else {
                        format!(
                            "  · {} staged by {}",
                            short(&staged.revision),
                            terminal_safe(&staged.staged_by)
                        )
                    },
                    theme.tone(Tone::Warn),
                ));
            }
            kv(node, spans, 10)
        })
        .take(usize::from(area.height.saturating_sub(2)))
        .collect::<Vec<_>>();
    lines.push(Line::styled(
        match row.kind {
            ValueKind::Secret => {
                "enter sets it (hidden input) · x stages its removal · X drops what is staged"
            }
            ValueKind::Variable => "enter sets it · x stages its removal · X drops what is staged",
        },
        theme.muted(),
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::new()
                    .borders(Borders::TOP)
                    .border_style(theme.border())
                    .title(Span::styled(
                        format!(" {} ", terminal_safe(row.key)),
                        theme.strong(),
                    )),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}
