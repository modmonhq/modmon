use super::*;
use crate::presentation::tui::review::{NodeRow, NodeStage, Review, ReviewStage};

/// The review of a plan, and where its apply is: full width, like a log.
pub(super) fn render_deploy_review(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &Model,
    review: &Review,
) {
    let theme = theme();
    let meta = match &review.stage {
        ReviewStage::Reviewing => hints(&[("enter", accept(review)), ("esc", "cancel")]),
        ReviewStage::Applying => vec![
            Span::styled(spinner(model.tick), theme.accent()),
            Span::styled(format!(" {}", applying(model, review)), theme.accent()),
        ],
        ReviewStage::Applied(_) => badge(Tone::Ok, "applied"),
        ReviewStage::Failed(_) => badge(Tone::Bad, "stopped"),
    };
    let block = with_meta(
        card(
            terminal_safe(&review.title(&model.snapshot.environment)),
            true,
        ),
        meta,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let head = head_lines(model, review, usize::from(inner.width));
    let height = u16::try_from(head.len() + 1).unwrap_or(u16::MAX);
    let [top, plan] = Layout::vertical([
        Constraint::Length(height.min(inner.height / 2)),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(head).block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(theme.border()),
        ),
        top,
    );
    let rows = usize::from(plan.height);
    let width = usize::from(plan.width);
    let total = review.review.lines.len();
    let wrapped = review
        .review
        .lines
        .iter()
        .map(|line| wrapped(&terminal_safe(line), width))
        .collect::<Vec<_>>();
    // The last page stays full: `end` shows the plan's last lines.
    let mut last_page = total;
    let mut used = 0;
    while last_page > 0 && used + wrapped[last_page - 1].len() <= rows {
        last_page -= 1;
        used += wrapped[last_page].len();
    }
    let first = review.scroll.min(last_page);
    let lines = review
        .review
        .lines
        .iter()
        .zip(&wrapped)
        .skip(first)
        .flat_map(|(line, rows)| {
            let style = plan_style(line);
            rows.iter().map(move |row| Line::styled(row.clone(), style))
        })
        .take(rows)
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), plan);
    scrollbar(frame, area, total, first);
}

/// The verb `enter` stands for.
fn accept(review: &Review) -> &'static str {
    match (&review.review.rollback, review.review.remote) {
        (Some(_), _) => "roll back",
        (None, true) => "deploy",
        (None, false) => "start",
    }
}

fn applying(model: &Model, review: &Review) -> String {
    let remote = model.remote().is_some();
    let busy = model
        .busy
        .as_ref()
        .map_or("Applying", |busy| busy.progress(remote));
    format!("{busy} plan {}", review.review.short_hash())
}

/// What the plan does in a few lines, then each node while it is applied,
/// then how the apply ended.
fn head_lines(model: &Model, review: &Review, width: usize) -> Vec<Line<'static>> {
    let theme = theme();
    let plan = &review.review;
    let mut lines = Vec::new();
    if let Some(rollback) = &plan.rollback {
        lines.push(Line::styled(terminal_safe(rollback), theme.strong()));
    }
    let stacks = if plan.stacks.is_empty() {
        "no stack".to_owned()
    } else {
        plan.stacks.join(", ")
    };
    let nodes = if plan.nodes.is_empty() {
        String::new()
    } else {
        format!(" ({})", plan.nodes.join(", "))
    };
    lines.push(Line::from(vec![
        Span::styled(
            terminal_safe(&format!("{stacks} on target '{}'{nodes}", plan.target)),
            theme.text(),
        ),
        Span::styled(
            format!(
                "   {} · plan {}",
                plural(plan.routes, "route"),
                plan.short_hash()
            ),
            theme.muted(),
        ),
    ]));
    if review.stage != ReviewStage::Reviewing && !review.nodes.is_empty() {
        lines.push(Line::raw(""));
        let name_width = review
            .nodes
            .iter()
            .map(|row| row.node.chars().count())
            .max()
            .unwrap_or(0);
        lines.extend(
            review
                .nodes
                .iter()
                .map(|row| node_line(model, row, name_width)),
        );
    }
    match &review.stage {
        ReviewStage::Reviewing => lines.push(Line::styled(
            if plan.rollback.is_some() {
                "The plan as `mm deploy rollback` prints it:"
            } else {
                "The plan as `mm deploy plan` prints it:"
            },
            theme.muted(),
        )),
        ReviewStage::Applying => {}
        ReviewStage::Applied(summary) => {
            lines.push(Line::raw(""));
            lines.push(Line::from(badge(Tone::Ok, terminal_safe(summary))));
        }
        ReviewStage::Failed(error) => {
            lines.push(Line::raw(""));
            for row in rows_of(&terminal_safe(error), width.saturating_sub(2), true) {
                lines.push(Line::styled(row, theme.tone(Tone::Bad)));
            }
        }
    }
    lines
}

/// One node while the plan is applied: what it is doing, and the stacks it
/// has applied, as `mm deploy status` lists them.
fn node_line(model: &Model, row: &NodeRow, width: usize) -> Line<'static> {
    let theme = theme();
    let applied = if row.stacks.is_empty() {
        String::new()
    } else {
        format!("   stacks applied: {}", row.stacks.join(", "))
    };
    let (glyph, tone, text) = match &row.stage {
        NodeStage::Waiting if row.stacks.is_empty() => {
            ("○".to_owned(), Tone::Idle, "waiting".to_owned())
        }
        NodeStage::Waiting => (
            "◐".to_owned(),
            Tone::Info,
            "waits for its next turn".to_owned(),
        ),
        NodeStage::Working(step) => (spinner(model.tick).to_owned(), Tone::Info, step.clone()),
        NodeStage::Done => ("●".to_owned(), Tone::Ok, "done".to_owned()),
        NodeStage::RolledBack => (
            "▲".to_owned(),
            Tone::Warn,
            "runs what it ran before".to_owned(),
        ),
        NodeStage::Stopped => ("✗".to_owned(), Tone::Bad, "stopped, see below".to_owned()),
    };
    Line::from(vec![
        Span::styled(format!("  {glyph} "), theme.tone(tone)),
        Span::styled(
            format!("{:<width$}  ", terminal_safe(&row.node)),
            theme.strong(),
        ),
        Span::styled(terminal_safe(&text), theme.tone(tone)),
        Span::styled(terminal_safe(&applied), theme.muted()),
    ])
}

/// Headings of the plan stand out; a node's section header is one.
fn plan_style(line: &str) -> ratatui::style::Style {
    let theme = theme();
    if !line.starts_with(' ') {
        theme.strong()
    } else {
        theme.text()
    }
}

/// A line of the plan as rows of at most `width` characters, broken
/// between words; the rows after the first sit under the line's text.
fn wrapped(line: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    if line.chars().count() <= width {
        return vec![line.to_owned()];
    }
    let indent = line
        .chars()
        .take_while(|character| *character == ' ')
        .count();
    let next = " ".repeat((indent + 2).min(width / 2));
    let mut rows = Vec::new();
    let mut row = " ".repeat(indent);
    let mut empty = true;
    for word in line.trim_start().split(' ') {
        if !empty && row.chars().count() + 1 + word.chars().count() > width {
            rows.push(std::mem::replace(&mut row, next.clone()));
            empty = true;
        }
        if !empty {
            row.push(' ');
        }
        row.push_str(word);
        empty = false;
        // A word wider than the row is cut where the row ends.
        while row.chars().count() > width {
            let head = row.chars().take(width).collect::<String>();
            let tail = row.chars().skip(width).collect::<String>();
            rows.push(head);
            row = format!("{next}{tail}");
        }
    }
    rows.push(row);
    rows
}
