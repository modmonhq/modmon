use super::*;

pub(super) fn render_ingress(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let statuses = &model.snapshot.ingress;
    let routed = statuses
        .iter()
        .filter(|status| status.url.is_some())
        .count();
    let block = with_meta(
        card("Routes", true),
        vec![Span::styled(
            format!("{routed} of {} routed", statuses.len()),
            theme.muted(),
        )],
    );
    if statuses.is_empty() {
        empty_state(
            frame,
            area,
            block,
            Tone::Idle,
            "No routable services",
            "Services with HTTP ports appear here once configured.",
        );
        return;
    }
    let selected = statuses.get(model.scroll);
    let note = selected.and_then(|status| status.warning.as_deref().or(status.reason.as_deref()));
    let [table, detail] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(if note.is_some() { 3 } else { 0 }),
    ])
    .areas(area);
    let rows = statuses.iter().map(ingress_row);
    render_table(
        frame,
        table,
        Table::new(
            rows,
            [
                Constraint::Percentage(24),
                Constraint::Min(30),
                Constraint::Length(6),
                Constraint::Length(20),
            ],
        )
        .header(table_header(&["SERVICE", "URL", "PORT", "SOURCE"]))
        .block(block),
        statuses.len(),
        Some(model.scroll),
    );
    if let (Some(status), Some(note)) = (selected, note) {
        let tone = if status.warning.is_some() {
            Tone::Warn
        } else {
            Tone::Idle
        };
        frame.render_widget(
            Paragraph::new(Line::from(badge(tone, terminal_safe(note))))
                .block(card(terminal_safe(&status.service), false))
                .wrap(Wrap { trim: true }),
            detail,
        );
    }
}

/// One route: the service, its URL or why it is not routed, port, and source.
fn ingress_row(status: &crate::domain::ingress::IngressServiceStatus) -> Row<'_> {
    let theme = theme();
    let service = Line::from(vec![
        Span::styled(terminal_safe(&status.service), theme.strong()),
        Span::styled(format!("  {}", terminal_safe(&status.stack)), theme.muted()),
    ]);
    let (glyph, target) = match &status.url {
        Some(url) => (
            Span::styled("↗", theme.accent()),
            Span::styled(terminal_safe(url), theme.text()),
        ),
        None => (
            Span::styled("·", theme.muted()),
            Span::styled(
                terminal_safe(status.reason.as_deref().unwrap_or("not routed")),
                theme.muted(),
            ),
        ),
    };
    let mut route = vec![glyph, Span::raw(" "), target];
    if status.warning.is_some() {
        route.push(Span::styled("  ▲", theme.tone(Tone::Warn)));
    }
    Row::new([
        Cell::from(service),
        Cell::from(Line::from(route)),
        Cell::from(Span::styled(
            status
                .selected_port
                .map_or_else(|| "—".to_owned(), |port| port.to_string()),
            theme.soft(),
        )),
        Cell::from(Span::styled(
            status
                .provenance
                .map_or_else(String::new, |provenance| provenance.to_string()),
            theme.muted(),
        )),
    ])
}
