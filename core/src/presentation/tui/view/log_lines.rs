//! The lines of the log view: which node and container wrote each one, then
//! the text, in the layout of `mm service logs`.

use super::*;
use crate::presentation::tui::panel::{LogLine, LogsPanel};

/// The log view's card. On a cluster target its title says how many nodes
/// the lines are merged from, or the one node they come from.
pub(super) fn logs_card(model: &Model, logs: &LogsPanel) -> Block<'static> {
    card_with_note(
        format!("Logs · {}", terminal_safe(&logs.target.label)),
        merged_from(model, logs),
        true,
    )
}

fn merged_from(model: &Model, logs: &LogsPanel) -> Option<String> {
    let target = &logs.target;
    let service = model
        .services()
        .into_iter()
        .find(|service| service.stack == target.stack && service.name == target.service)?;
    let one = |node: &str| format!("on {}", terminal_safe(node));
    if let Some(node) = &target.node {
        return Some(one(node));
    }
    if let Some(id) = &target.container {
        // One container is on one node.
        let container = service
            .containers
            .iter()
            .find(|container| &container.id == id)?;
        return container.node.as_deref().map(one);
    }
    match service.nodes.as_slice() {
        [] => None,
        [share] => Some(one(&share.node)),
        nodes => Some(format!("{} nodes, in order", nodes.len())),
    }
}

/// The columns before the text of every line, as wide as the widest name
/// among the lines the view holds, so the text stays aligned.
pub(super) struct LogColumns<'a> {
    /// The target's nodes in declared order, then any other in the order
    /// its first line arrived. A node's place picks its colour.
    nodes: Vec<&'a str>,
    node: usize,
    container: usize,
}

impl<'a> LogColumns<'a> {
    pub(super) fn new(model: &'a Model, logs: &'a LogsPanel) -> Self {
        let mut nodes = model
            .snapshot
            .nodes
            .iter()
            .map(|node| node.name.as_str())
            .collect::<Vec<_>>();
        let mut containers = Vec::new();
        let (mut node, mut container) = (0, 0);
        for line in &logs.lines {
            if let Some(name) = line.node.as_deref() {
                node = node.max(name.chars().count());
                if !nodes.contains(&name) {
                    nodes.push(name);
                }
            }
            if let Some(name) = line.container.as_deref() {
                container = container.max(name.chars().count());
                if !containers.contains(&name) {
                    containers.push(name);
                }
            }
        }
        // One container on a single host: the title already names it.
        if node == 0 && containers.len() < 2 {
            container = 0;
        }
        Self {
            nodes,
            node,
            container,
        }
    }

    /// How many columns the names and the mark after them take.
    fn width(&self) -> usize {
        match (self.node, self.container) {
            (0, 0) => 0,
            (node, 0) => node + 3,
            (0, container) => container + 3,
            (node, container) => node + container + 4,
        }
    }

    /// The node and the container of a line, each padded to its column,
    /// then the mark: `│` before what a container wrote, `!` before what mm
    /// says about a node.
    fn names(&self, line: &LogLine) -> Vec<Span<'static>> {
        let theme = theme();
        let mut spans = Vec::new();
        if self.width() == 0 {
            return spans;
        }
        let node = line.node.as_deref().unwrap_or("");
        if self.node > 0 {
            let place = self.nodes.iter().position(|name| *name == node);
            spans.push(Span::styled(
                format!("{:<width$} ", terminal_safe(node), width = self.node),
                if line.about_node {
                    theme.tone(Tone::Warn)
                } else {
                    theme.node(place.unwrap_or(0))
                },
            ));
        }
        if self.container > 0 {
            let container = line.container.as_deref().unwrap_or("");
            spans.push(Span::styled(
                format!(
                    "{:<width$} ",
                    terminal_safe(container),
                    width = self.container
                ),
                theme.muted(),
            ));
        }
        spans.push(if line.about_node {
            Span::styled("! ", theme.tone(Tone::Warn).add_modifier(Modifier::BOLD))
        } else {
            Span::styled("│ ", theme.border())
        });
        spans
    }

    /// One line as screen rows: a marker for stderr or for what mm says
    /// about a node, the names and the time on the first row, and the rows
    /// that follow under the text.
    pub(super) fn rows(
        &self,
        line: &LogLine,
        timestamps: bool,
        width: usize,
        wrap: bool,
    ) -> Vec<Line<'static>> {
        let theme = theme();
        let clock = match (&line.timestamp, timestamps) {
            (Some(timestamp), true) => format!("{} ", clock_time(timestamp)),
            _ => String::new(),
        };
        let (marker, style) = if line.about_node {
            (Tone::Warn.glyph(), theme.tone(Tone::Warn))
        } else if line.stderr {
            ("▎", line_style(&line.text))
        } else {
            (" ", line_style(&line.text))
        };
        let names = self.width();
        let indent = names + clock.chars().count();
        let available = width.saturating_sub(GUTTER + indent);
        rows_of(&terminal_safe(&line.text), available, wrap)
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let first = index == 0;
                // The stream marker runs down a wrapped line; a warning is marked once.
                let marker = if first || !line.about_node {
                    marker
                } else {
                    " "
                };
                let mut spans = vec![Span::styled(format!("{marker} "), theme.tone(Tone::Warn))];
                if first {
                    spans.extend(self.names(line));
                    spans.push(Span::styled(clock.clone(), theme.muted()));
                } else if names > 0 && !line.about_node {
                    spans.push(Span::raw(" ".repeat(names - 2)));
                    spans.push(Span::styled("│ ", theme.border()));
                    spans.push(Span::raw(" ".repeat(indent - names)));
                } else {
                    spans.push(Span::raw(" ".repeat(indent)));
                }
                spans.push(Span::styled(row, style));
                Line::from(spans)
            })
            .collect()
    }
}
