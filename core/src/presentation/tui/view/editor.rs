use super::*;
use crate::domain::onboarding::ConfigMutationAction;
use crate::presentation::tui::editor::{Editor, EditorStage, Field, FieldKind};

const LABEL_WIDTH: usize = 15;

/// The configuration form, its review, or its next steps, over the current view.
pub(super) fn render_editor(frame: &mut Frame<'_>, editor: &Editor) {
    match &editor.stage {
        EditorStage::Form => render_form(frame, editor),
        EditorStage::Review { plan, scroll } => render_review(frame, editor, plan, *scroll),
        EditorStage::Done { headline, steps } => render_done(frame, editor, headline, steps),
    }
}

fn form_row(field: &Field, focused: bool, width: usize) -> Line<'static> {
    let theme = theme();
    let label = if field.required {
        format!("{} *", field.label)
    } else {
        field.label.clone()
    };
    match &field.kind {
        FieldKind::Text | FieldKind::List | FieldKind::Command => {
            // Keep the end of a long value in view, where the cursor is.
            let room = width.saturating_sub(LABEL_WIDTH + 4);
            let value = terminal_safe(&field.value);
            let shown = value
                .chars()
                .skip(value.chars().count().saturating_sub(room))
                .collect::<String>();
            widgets::field(&label, &shown, focused, LABEL_WIDTH)
        }
        FieldKind::Choice(_) | FieldKind::Toggle => {
            let mut spans = vec![
                Span::styled(if focused { "▌ " } else { "  " }, theme.accent()),
                // Two columns narrower: the choice markers take them.
                Span::styled(
                    format!("{label:<width$}", width = LABEL_WIDTH - 2),
                    if focused {
                        theme.strong()
                    } else {
                        theme.muted()
                    },
                ),
            ];
            let quiet = matches!(field.value.as_str(), "none" | "no" | "project default");
            let value = Span::styled(
                terminal_safe(&field.value),
                if quiet { theme.soft() } else { theme.key() },
            );
            if focused {
                spans.push(Span::styled("‹ ", theme.muted()));
                spans.push(value);
                spans.push(Span::styled(" ›", theme.muted()));
            } else {
                spans.push(Span::raw("  "));
                spans.push(value);
            }
            Line::from(spans)
        }
    }
}

fn render_form(frame: &mut Frame<'_>, editor: &Editor) {
    let theme = theme();
    let screen = frame.area();
    let width = 84.min(screen.width.saturating_sub(2)).max(1);
    let inner = usize::from(width.saturating_sub(4));
    let mut lines = Vec::new();
    if let Some(note) = &editor.note {
        for chunk in wrap_text(note, inner) {
            lines.push(Line::styled(chunk, theme.muted()));
        }
        lines.push(Line::raw(""));
    }
    for (index, field) in editor.fields.iter().enumerate() {
        lines.push(form_row(field, index == editor.focus, inner));
    }
    lines.push(Line::raw(""));
    let help = editor
        .fields
        .get(editor.focus)
        .map_or("", |field| field.help.as_str());
    let help = wrap_text(help, inner);
    // Two rows of help keep the form from jumping as the focus moves.
    for index in 0..2 {
        lines.push(Line::styled(
            help.get(index).cloned().unwrap_or_default(),
            theme.soft(),
        ));
    }
    match &editor.error {
        Some(error) => {
            for chunk in wrap_text(&terminal_safe(error), inner.saturating_sub(2)) {
                lines.push(Line::from(badge(Tone::Bad, chunk)));
            }
        }
        None => lines.push(Line::raw("")),
    }
    let choosing = editor
        .fields
        .get(editor.focus)
        .is_some_and(|field| matches!(field.kind, FieldKind::Choice(_) | FieldKind::Toggle));
    lines.push(Line::from(hints(&[
        ("tab", "next field"),
        if choosing {
            ("← →", "change")
        } else {
            ("type", "to edit")
        },
        ("enter", "review"),
        ("esc", "cancel"),
    ])));
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let area = centered_rect(width, height, screen);
    let mut block = card(terminal_safe(&editor.title()), true);
    // The reminder shares the title row, so it yields to the title when narrow.
    if width >= 80 {
        block = with_meta(
            block,
            vec![Span::styled(
                "nothing is saved until you review",
                theme.muted(),
            )],
        );
    }
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

/// The diff of a configuration change, coloured as a diff.
fn diff_lines(plan: &crate::domain::onboarding::ConfigMutationPlan) -> Vec<Line<'static>> {
    let theme = theme();
    plan.canonical_diff
        .lines()
        .map(|line| {
            let style = match line.chars().next() {
                Some('+') if !line.starts_with("+++") => theme.tone(Tone::Ok),
                Some('-') if !line.starts_with("---") => theme.tone(Tone::Bad),
                Some('@') => theme.tone(Tone::Info),
                _ => theme.muted(),
            };
            Line::styled(terminal_safe(line), style)
        })
        .collect()
}

/// The names of the files a change writes.
fn affected_files(plan: &crate::domain::onboarding::ConfigMutationPlan) -> String {
    plan.affected_files
        .iter()
        .map(|path| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_review(
    frame: &mut Frame<'_>,
    editor: &Editor,
    plan: &crate::domain::onboarding::ConfigMutationPlan,
    scroll: usize,
) {
    let theme = theme();
    let screen = frame.area();
    let width = 100.min(screen.width.saturating_sub(2)).max(1);
    let diff = diff_lines(plan);
    let files = affected_files(plan);
    let error_rows = u16::from(editor.error.is_some()) * 2;
    let chrome = 6 + error_rows;
    let height = u16::try_from(diff.len())
        .unwrap_or(u16::MAX)
        .saturating_add(chrome)
        .min(screen.height.saturating_sub(2))
        .max(chrome + 1);
    let area = centered_rect(width, height, screen);
    let destructive = editor.action == ConfigMutationAction::Remove;
    let block = with_meta(
        card(format!("Review: {}", terminal_safe(&editor.title())), true)
            .border_style(theme.tone(if destructive { Tone::Warn } else { Tone::Ok })),
        vec![Span::styled(
            format!(
                "plan {}",
                plan.plan_hash.chars().take(12).collect::<String>()
            ),
            theme.muted(),
        )],
    );
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    let [summary, body, error, keys] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(error_rows),
        Constraint::Length(2),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Writes ", theme.muted()),
            Span::styled(terminal_safe(&files), theme.strong()),
            Span::styled(". Nothing else changes until you deploy.", theme.muted()),
        ])),
        summary,
    );
    let viewport = usize::from(body.height);
    let offset = scroll.min(diff.len().saturating_sub(viewport));
    let total = diff.len();
    frame.render_widget(
        Paragraph::new(diff.into_iter().skip(offset).collect::<Vec<_>>()),
        body,
    );
    if total > viewport {
        scrollbar(
            frame,
            Rect::new(
                area.x,
                body.y.saturating_sub(1),
                area.width,
                body.height + 2,
            ),
            total,
            offset,
        );
    }
    if let Some(message) = &editor.error {
        frame.render_widget(
            Paragraph::new(Line::from(badge(Tone::Bad, terminal_safe(message))))
                .wrap(Wrap { trim: true }),
            error,
        );
    }
    let mut pairs = vec![("enter/y", if destructive { "remove" } else { "apply" })];
    if !destructive {
        pairs.push(("b", "back to the form"));
    }
    pairs.extend([("j/k", "scroll"), ("esc/n", "cancel")]);
    frame.render_widget(
        Paragraph::new(vec![Line::raw(""), Line::from(hints(&pairs))]),
        keys,
    );
}

fn render_done(frame: &mut Frame<'_>, editor: &Editor, headline: &str, steps: &[String]) {
    let theme = theme();
    let screen = frame.area();
    let width = 92.min(screen.width.saturating_sub(2)).max(1);
    let inner = usize::from(width.saturating_sub(4));
    let mut lines = vec![
        Line::from(badge(Tone::Ok, terminal_safe(headline))),
        Line::raw(""),
        section("What happens next"),
    ];
    for (index, step) in steps.iter().enumerate() {
        for (row, chunk) in wrap_text(&terminal_safe(step), inner.saturating_sub(3))
            .into_iter()
            .enumerate()
        {
            lines.push(Line::from(vec![
                Span::styled(
                    if row == 0 {
                        format!("{}  ", index + 1)
                    } else {
                        "   ".to_owned()
                    },
                    theme.key(),
                ),
                Span::styled(chunk, theme.text()),
            ]));
        }
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(hints(&[("enter", "done")])));
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let area = centered_rect(width, height, screen);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(card(terminal_safe(&editor.title()), true)),
        area,
    );
}
