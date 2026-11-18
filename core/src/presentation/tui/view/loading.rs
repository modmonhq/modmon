//! What a view shows while its part of the session is still being read.

use super::*;
use crate::presentation::tui::loading::Part;

/// Whether the view of the current tab has nothing to show yet but that its
/// part is on the way. Views that have something of their own to show
/// while they wait (the Overview, the team in Access, the services of the
/// nodes that already answered) draw themselves.
pub(super) fn waits(model: &Model) -> Option<Part> {
    let part = if model.tab == Tab::Tls && model.remote().is_some() {
        model
            .loading
            .is_pending(Part::Status)
            .then_some(Part::Status)?
    } else {
        model.loading.waiting(model.tab)?
    };
    match model.tab {
        Tab::Overview | Tab::Access => None,
        Tab::Services if !model.services().is_empty() => None,
        _ => Some(part),
    }
}

/// The whole view: what is being read, and on a cluster, which nodes have
/// answered.
pub(super) fn render_reading(frame: &mut Frame<'_>, area: Rect, model: &Model, part: Part) {
    let theme = theme();
    let block = card(model.tab.title(), true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = vec![reading_line(model, part).centered()];
    if part == Part::Status && !model.snapshot.nodes.is_empty() {
        lines.push(Line::raw(""));
        lines.extend(node_lines(model).into_iter().map(Line::centered));
    }
    let height = u16::try_from(lines.len()).unwrap_or(u16::MAX);
    let top = inner.y + inner.height.saturating_sub(height) / 2;
    frame.render_widget(
        Paragraph::new(lines).style(theme.text()),
        Rect::new(
            inner.x,
            top,
            inner.width,
            inner.height.saturating_sub(top - inner.y),
        ),
    );
}

/// A spinner and what is being read, for a card of its own or a line in one.
pub(super) fn reading_line(model: &Model, part: Part) -> Line<'static> {
    let theme = theme();
    Line::from(vec![
        Span::styled(spinner(model.tick), theme.accent()),
        Span::raw(" "),
        Span::styled(reading(model, part), theme.soft()),
    ])
}

/// What is being read, in a few words.
pub(super) fn reading(model: &Model, part: Part) -> String {
    match (part, model.remote()) {
        (Part::Status, Some(target)) => format!("Asking {}", terminal_safe(target)),
        (Part::Status, None) => "Asking the engine".to_owned(),
        (Part::Identity, Some(target)) => {
            format!("Asking {} who you are", terminal_safe(target))
        }
        (Part::Identity, None) => "Reading access".to_owned(),
        (Part::Variables, _) => "Resolving variables".to_owned(),
        (Part::Mounts, _) => "Reading development mounts".to_owned(),
        (Part::Https, _) => "Checking local HTTPS".to_owned(),
        (Part::SharedServices, _) => "Checking shared services".to_owned(),
    }
}

/// One line per node of a cluster: answered, not answered yet, or what is
/// wrong with it.
pub(super) fn node_lines(model: &Model) -> Vec<Line<'static>> {
    let theme = theme();
    model
        .snapshot
        .nodes
        .iter()
        .map(|node| {
            let name = terminal_safe(&node.name);
            if node.is_pending() {
                Line::from(vec![
                    Span::styled(format!("{} {name}", Tone::Idle.glyph()), theme.muted()),
                    Span::styled("  not answered yet", theme.muted()),
                ])
            } else if node.reachable {
                let mut spans = badge(Tone::Ok, name);
                spans.push(Span::styled("  answered", theme.muted()));
                Line::from(spans)
            } else {
                let mut spans = badge(Tone::Bad, name);
                spans.push(Span::styled(
                    format!(
                        "  {}",
                        terminal_safe(node.problem.as_deref().unwrap_or("does not answer"))
                    ),
                    theme.muted(),
                ));
                Line::from(spans)
            }
        })
        .collect()
}
