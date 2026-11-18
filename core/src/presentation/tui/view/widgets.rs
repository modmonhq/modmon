use ratatui::widgets::{
    BorderType, HighlightSpacing, Padding, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use super::*;

pub(crate) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const MAXIMUM_PIPS: u32 = 10;

/// A rounded panel with a bold title; `focused` brightens the border.
pub(super) fn card<'a>(title: impl Into<String>, focused: bool) -> Block<'a> {
    card_with_note(title, None, focused)
}

/// A card whose title is followed by a quieter note about it.
pub(super) fn card_with_note<'a>(
    title: impl Into<String>,
    note: Option<String>,
    focused: bool,
) -> Block<'a> {
    let theme = theme();
    let mut title = vec![
        Span::raw(" "),
        Span::styled(title.into(), theme.strong()),
        Span::raw(" "),
    ];
    if let Some(note) = note {
        title.push(Span::styled(format!(" {note} "), theme.muted()));
    }
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(if focused {
            theme.border_focus()
        } else {
            theme.border()
        })
        .title(Line::from(title))
        .padding(Padding::horizontal(1))
}

/// Adds a right-aligned summary to a card's top border.
pub(super) fn with_meta<'a>(block: Block<'a>, mut meta: Vec<Span<'a>>) -> Block<'a> {
    if meta.is_empty() {
        return block;
    }
    meta.insert(0, Span::raw(" "));
    meta.push(Span::raw(" "));
    block.title_top(Line::from(meta).right_aligned())
}

pub(super) fn badge(tone: Tone, label: impl Into<String>) -> Vec<Span<'static>> {
    let style = theme().tone(tone);
    vec![
        Span::styled(tone.glyph(), style),
        Span::raw(" "),
        Span::styled(label.into(), style),
    ]
}

pub(super) fn kv(label: &str, value: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled(format!("{label:<width$}"), theme().muted())];
    spans.extend(value);
    Line::from(spans)
}

pub(super) fn kv_text(label: &str, value: &str, width: usize) -> Line<'static> {
    kv(
        label,
        vec![Span::styled(terminal_safe(value), theme().text())],
        width,
    )
}

pub(super) fn section(label: &str) -> Line<'static> {
    Line::styled(
        label.to_uppercase(),
        theme().muted().add_modifier(Modifier::BOLD),
    )
}

/// One pip per desired replica: healthy, running but unhealthy, or missing.
pub(super) fn pips(desired: u32, running: u32, healthy: u32) -> Vec<Span<'static>> {
    let theme = theme();
    let shown = desired.min(MAXIMUM_PIPS);
    let mut spans = (0..shown)
        .map(|index| {
            if index < healthy {
                Span::styled("▰", theme.tone(Tone::Ok))
            } else if index < running {
                Span::styled("▰", theme.tone(Tone::Warn))
            } else {
                Span::styled("▱", theme.muted())
            }
        })
        .collect::<Vec<_>>();
    if desired > MAXIMUM_PIPS {
        spans.push(Span::styled("…", theme.muted()));
    }
    spans
}

pub(super) fn hints(pairs: &[(&str, &str)]) -> Vec<Span<'static>> {
    let theme = theme();
    let mut spans = Vec::new();
    for (index, (key, label)) in pairs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled((*key).to_owned(), theme.key()));
        spans.push(Span::styled(format!(" {label}"), theme.muted()));
    }
    spans
}

pub(super) fn spinner(tick: u64) -> &'static str {
    SPINNER[usize::try_from(tick % SPINNER.len() as u64).unwrap_or(0)]
}

/// The leading highlight marker shared by every selectable table and list.
pub(super) fn selection_symbol() -> Line<'static> {
    Line::styled("▌", theme().accent())
}

pub(super) fn selectable_table<'a>(table: Table<'a>) -> Table<'a> {
    table
        .row_highlight_style(theme().selected())
        .highlight_symbol(selection_symbol())
        .highlight_spacing(HighlightSpacing::Always)
        .column_spacing(2)
}

pub(super) fn table_header<'a>(columns: &[&'a str]) -> Row<'a> {
    Row::new(columns.iter().copied())
        .style(theme().muted().add_modifier(Modifier::BOLD))
        .bottom_margin(0)
}

/// Draws a scrollbar on the right border of `area` when content overflows.
pub(super) fn scrollbar(frame: &mut Frame<'_>, area: Rect, total: usize, position: usize) {
    let viewport = usize::from(area.height.saturating_sub(2));
    if total <= viewport {
        return;
    }
    let mut state = ScrollbarState::new(total.saturating_sub(viewport)).position(position);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some("│"))
            .track_style(theme().border())
            .thumb_symbol("┃")
            .thumb_style(theme().accent()),
        area.inner(ratatui::layout::Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut state,
    );
}

/// A centered message for views and panels with nothing to show.
pub(super) fn empty_state(
    frame: &mut Frame<'_>,
    area: Rect,
    block: Block<'_>,
    tone: Tone,
    title: &str,
    detail: &str,
) {
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let height = if detail.is_empty() { 1 } else { 3 };
    let top = inner.y + inner.height.saturating_sub(height) / 2;
    let mut lines = vec![Line::from(badge(tone, title.to_owned())).centered()];
    if !detail.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(terminal_safe(detail), theme().muted()).centered());
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }),
        Rect::new(
            inner.x,
            top,
            inner.width,
            inner.height.saturating_sub(top - inner.y),
        ),
    );
}

/// A one-line progress indicator such as `● Choose ─ ◉ Discover ─ ○ Review`.
pub(super) fn stepper(steps: &[&str], current: usize, current_tone: Tone) -> Line<'static> {
    let theme = theme();
    let mut spans = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" ─ ", theme.border()));
        }
        let (glyph, style) = match index.cmp(&current) {
            std::cmp::Ordering::Less => ("●", theme.tone(Tone::Ok)),
            std::cmp::Ordering::Equal => {
                ("◉", theme.tone(current_tone).add_modifier(Modifier::BOLD))
            }
            std::cmp::Ordering::Greater => ("○", theme.muted()),
        };
        spans.push(Span::styled(format!("{glyph} {step}"), style));
    }
    Line::from(spans)
}

/// A form row; the focused field gets an accent bar and a block cursor.
pub(super) fn field(label: &str, value: &str, focused: bool, width: usize) -> Line<'static> {
    let theme = theme();
    let mut spans = vec![
        Span::styled(if focused { "▌ " } else { "  " }, theme.accent()),
        Span::styled(
            format!("{label:<width$}"),
            if focused {
                theme.strong()
            } else {
                theme.muted()
            },
        ),
        Span::styled(terminal_safe(value), theme.text()),
    ];
    if focused {
        spans.push(Span::styled("█", theme.accent()));
    }
    Line::from(spans)
}

/// A bordered outcome panel for applying, complete, error, and cancelled states.
pub(super) fn result_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    tone: Tone,
    headline: Vec<Span<'static>>,
    body: &str,
    keys: &[(&str, &str)],
) {
    let mut lines = vec![Line::from(headline), Line::raw("")];
    lines.extend(
        terminal_safe(body)
            .lines()
            .map(|line| Line::styled(line.to_owned(), theme().soft())),
    );
    if !keys.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(hints(keys)));
    }
    let block = card(title.to_owned(), false).border_style(theme().tone(tone));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}
