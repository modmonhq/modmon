//! One log line as text: which node and container wrote it, then the line.
//!
//! The command line and the terminal UI print the same prefix, so a line
//! copied from one is found in the other.

use super::terminal_safe;
use crate::domain::observability::{LogRecord, LogSource};

/// Formats the lines of one read. The prefix columns are as wide as the
/// widest name seen so far, so lines stay aligned as nodes and containers
/// appear.
#[derive(Debug, Clone, Default)]
pub(crate) struct LogLines {
    timestamps: bool,
    node_width: usize,
    container_width: usize,
}

/// The parts of one formatted line, for an interface that colours them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogLineParts {
    /// The node's name padded to the column; empty on a single host.
    pub node: String,
    /// The container's short name padded to the column; empty when the
    /// line names none.
    pub container: String,
    /// `|` before a line a container wrote, `!` before what mm says about
    /// a node (such as one that did not answer).
    pub mark: char,
    pub text: String,
}

impl LogLineParts {
    /// The line as plain text: `app-1 web-1 | GET / 200`.
    pub(crate) fn plain(&self) -> String {
        let mut line = String::new();
        for column in [&self.node, &self.container] {
            if !column.is_empty() {
                line.push_str(column);
                line.push(' ');
            }
        }
        if !line.is_empty() {
            line.push(self.mark);
            line.push(' ');
        }
        line.push_str(&self.text);
        line
    }
}

impl LogLines {
    /// `timestamps`: whether each line shows when the engine recorded it.
    pub(crate) fn new(timestamps: bool) -> Self {
        Self {
            timestamps,
            ..Self::default()
        }
    }

    /// Makes room for these node names before the first line arrives.
    pub(crate) fn with_nodes<'a>(mut self, nodes: impl IntoIterator<Item = &'a str>) -> Self {
        self.node_width = nodes
            .into_iter()
            .map(|node| node.chars().count())
            .max()
            .unwrap_or(0);
        self
    }

    pub(crate) fn parts(&mut self, record: &LogRecord) -> LogLineParts {
        let node = record
            .node
            .as_deref()
            .map(terminal_safe)
            .unwrap_or_default();
        self.node_width = self.node_width.max(node.chars().count());
        let about_node = record.source == LogSource::Runtime;
        let container = if about_node {
            String::new()
        } else {
            record
                .container_label()
                .or(Some(record.identity.service.as_str()).filter(|service| !service.is_empty()))
                .map(terminal_safe)
                .unwrap_or_default()
        };
        self.container_width = self.container_width.max(container.chars().count());
        let timestamp = record
            .timestamp
            .as_deref()
            .filter(|_| self.timestamps)
            .map(|timestamp| format!("{} ", terminal_safe(timestamp)))
            .unwrap_or_default();
        LogLineParts {
            node: if self.node_width == 0 {
                String::new()
            } else {
                format!("{node:<width$}", width = self.node_width)
            },
            // What mm says about a node has no container column: the mark
            // follows the node's name.
            container: if self.container_width == 0 || about_node {
                String::new()
            } else {
                format!("{container:<width$}", width = self.container_width)
            },
            mark: if about_node { '!' } else { '|' },
            text: format!(
                "{timestamp}{}{}",
                terminal_safe(&record.message),
                if record.truncated {
                    " [line truncated]"
                } else {
                    ""
                }
            ),
        }
    }

    /// The line as plain text.
    pub(crate) fn line(&mut self, record: &LogRecord) -> String {
        self.parts(record).plain()
    }
}
