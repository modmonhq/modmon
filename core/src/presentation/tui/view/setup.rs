use super::*;

/// Greedy word wrap by character count; a word longer than the width keeps
/// its own line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let length = current.chars().count();
        if length > 0 && length + 1 + word.chars().count() > width {
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

/// Whether the project has another environment to go to.
fn has_other_environments(model: &Model) -> bool {
    model
        .environments
        .entries
        .iter()
        .any(|entry| entry.name != model.snapshot.environment)
}

fn environments_hint(model: &Model) -> Option<String> {
    has_other_environments(model).then(|| {
        let key = Tab::Environments
            .digit()
            .map_or_else(String::new, |digit| format!(" ({digit})"));
        format!("Other environments still open from the Environments view{key}.")
    })
}

/// The card's lines and, for each step, the range of lines it occupies.
fn setup_lines(model: &Model, width: usize) -> (Vec<Line<'static>>, Vec<std::ops::Range<usize>>) {
    let theme = theme();
    let environment = terminal_safe(&model.snapshot.environment);
    let mut lines = Vec::new();
    let mut ranges = Vec::new();
    if model.setup.steps.is_empty() {
        let reason = model.setup.unavailable.as_deref().unwrap_or_default();
        for (index, line) in wrap(&terminal_safe(reason), width.saturating_sub(2))
            .into_iter()
            .enumerate()
        {
            lines.push(if index == 0 {
                Line::from(badge(Tone::Bad, line))
            } else {
                Line::styled(format!("  {line}"), theme.tone(Tone::Bad))
            });
        }
        lines.push(Line::raw(""));
        let mut next = "mm keeps checking and shows it as soon as it answers.".to_owned();
        if let Some(hint) = environments_hint(model) {
            next = format!("{next} {hint}");
        }
        lines.extend(
            wrap(&next, width)
                .into_iter()
                .map(|line| Line::styled(line, theme.muted())),
        );
        return (lines, ranges);
    }
    let mut intro = if model.setup.unavailable.is_some() {
        format!("{environment} cannot be shown until these are done.")
    } else {
        format!("{environment} cannot start until these are done.")
    };
    if let Some(hint) = environments_hint(model) {
        intro = format!("{intro} {hint}");
    }
    lines.extend(
        wrap(&intro, width)
            .into_iter()
            .map(|line| Line::styled(line, theme.muted())),
    );
    for (index, step) in model.setup.steps.iter().enumerate() {
        lines.push(Line::raw(""));
        let start = lines.len();
        let selected = index == model.scroll;
        let mut title = vec![
            Span::styled(if selected { "▌" } else { " " }, theme.accent()),
            Span::raw(" "),
            Span::styled(
                (index + 1).to_string(),
                if selected { theme.key() } else { theme.muted() },
            ),
            Span::raw("  "),
            Span::styled(terminal_safe(&step.title), theme.strong()),
        ];
        if !step.ready {
            title.push(Span::styled("  after the steps above", theme.muted()));
        }
        if let Some(command) = &step.command {
            let command = terminal_safe(command);
            let used = Line::from(title.clone()).width() + command.chars().count() + 2;
            if used <= width {
                title.push(Span::raw(" ".repeat(width - used + 2)));
                title.push(Span::styled(command, theme.soft()));
            }
        }
        let title = Line::from(title);
        lines.push(if selected {
            title.style(theme.selected())
        } else {
            title
        });
        lines.extend(
            wrap(&terminal_safe(&step.detail), width.saturating_sub(5))
                .into_iter()
                .map(|line| Line::styled(format!("     {line}"), theme.soft())),
        );
        ranges.push(start..lines.len());
    }
    (lines, ranges)
}

/// The text width inside the card: borders and one column of padding each side.
fn inner_width(width: u16) -> usize {
    usize::from(width.saturating_sub(4))
}

/// How tall the setup card is at `width`, borders included.
pub(super) fn setup_height(model: &Model, width: u16) -> u16 {
    let (lines, _) = setup_lines(model, inner_width(width));
    u16::try_from(lines.len() + 2).unwrap_or(u16::MAX)
}

/// What the environment needs before it can start, or why it cannot be shown.
pub(super) fn render_setup(frame: &mut Frame<'_>, area: Rect, model: &Model) {
    let environment = terminal_safe(&model.snapshot.environment);
    let (lines, ranges) = setup_lines(model, inner_width(area.width));
    let block = if model.setup.steps.is_empty() {
        card(format!("Cannot show {environment}"), true)
    } else {
        with_meta(
            card(format!("Set up {environment}"), true),
            vec![Span::styled(
                format!("{} left", plural(model.setup.steps.len(), "step")),
                theme().tone(Tone::Warn),
            )],
        )
    };
    // A short terminal shows the selected step; the rest scrolls with it.
    let visible = usize::from(area.height.saturating_sub(2));
    let offset = ranges.get(model.scroll).map_or(0, |range| {
        if range.end <= visible {
            0
        } else {
            (range.end - visible).min(range.start)
        }
    });
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .scroll((u16::try_from(offset).unwrap_or(u16::MAX), 0)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn wrap_breaks_on_words_and_keeps_long_words_whole() {
        assert_eq!(
            wrap("12 services have no built image yet", 16),
            ["12 services have", "no built image", "yet"]
        );
        assert_eq!(
            wrap("mm-services-dev-412092c800fc43085768 is missing", 10),
            ["mm-services-dev-412092c800fc43085768", "is missing"]
        );
        assert!(wrap("", 10).is_empty());
    }
}
