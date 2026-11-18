use super::*;
use crate::domain::development_mount::MountConfidence;

const fn confidence(confidence: MountConfidence) -> (&'static str, Tone) {
    match confidence {
        MountConfidence::High => ("high", Tone::Ok),
        MountConfidence::Medium => ("medium", Tone::Info),
        MountConfidence::Low => ("low", Tone::Warn),
        MountConfidence::Missing => ("missing", Tone::Bad),
    }
}

pub(super) fn render_mounts(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let theme = theme();
    let configured = model
        .mounts
        .iter()
        .filter(|plan| plan.proposal.configured)
        .count();
    let block = with_meta(
        card("Development mounts", true),
        vec![Span::styled(
            format!("{configured} of {} configured", model.mounts.len()),
            theme.muted(),
        )],
    );
    let [table, message] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(u16::from(model.mount_message.is_some()) * 3),
    ])
    .areas(area);
    if model.mounts.is_empty() {
        empty_state(
            frame,
            table,
            block,
            Tone::Idle,
            "No mount proposals",
            "Modules with local source appear here.",
        );
    } else {
        let rows = model.mounts.iter().map(|plan| {
            let proposal = &plan.proposal;
            let mapping = proposal.intent.as_ref().map_or_else(
                || Line::styled("target missing", theme.tone(Tone::Bad)),
                |intent| {
                    Line::from(vec![
                        Span::styled(
                            terminal_safe(&intent.source.display().to_string()),
                            theme.text(),
                        ),
                        Span::styled(" → ", theme.accent()),
                        Span::styled(terminal_safe(&intent.target), theme.soft()),
                    ])
                },
            );
            let (label, tone) = confidence(proposal.confidence);
            let state = if proposal.configured {
                badge(Tone::Ok, "configured")
            } else {
                badge(Tone::Idle, "proposal")
            };
            Row::new([
                Cell::from(Span::styled(
                    terminal_safe(&proposal.module),
                    theme.strong(),
                )),
                Cell::from(mapping),
                Cell::from(Span::styled(label, theme.tone(tone))),
                Cell::from(Line::from(state)),
            ])
        });
        render_table(
            frame,
            table,
            Table::new(
                rows,
                [
                    Constraint::Percentage(18),
                    Constraint::Min(30),
                    Constraint::Length(11),
                    Constraint::Length(13),
                ],
            )
            .header(table_header(&[
                "MODULE",
                "SOURCE → CONTAINER",
                "CONFIDENCE",
                "STATE",
            ]))
            .block(block),
            model.mounts.len(),
            Some(model.scroll),
        );
    }
    if let Some(text) = &model.mount_message {
        frame.render_widget(
            Paragraph::new(Line::from(badge(Tone::Info, terminal_safe(text))))
                .block(card("Last change", false))
                .wrap(Wrap { trim: true }),
            message,
        );
    }
}
