//! The Prometheus text exposition format, version 0.0.4, with a limit on
//! the size of what it writes.

use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Gauge,
    Counter,
}

impl Kind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Gauge => "gauge",
            Self::Counter => "counter",
        }
    }
}

/// Room kept at the end for the line that says the output was cut.
const TRUNCATION_ROOM: usize = 256;

/// The text of one scrape. A family or sample that would take it past its
/// limit is left out, and the text ends with `mm_metrics_truncated 1`.
#[derive(Debug)]
pub struct Exposition {
    text: String,
    limit: usize,
    truncated: bool,
}

impl Exposition {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            text: String::new(),
            limit: limit.saturating_sub(TRUNCATION_ROOM),
            truncated: false,
        }
    }

    fn push(&mut self, line: &str) {
        if self.truncated || self.text.len() + line.len() > self.limit {
            self.truncated = true;
            return;
        }
        self.text.push_str(line);
    }

    /// Starts the family `name`: its help and its type.
    pub fn family(&mut self, name: &str, kind: Kind, help: &str) {
        let help = help.replace('\\', "\\\\").replace('\n', "\\n");
        self.push(&format!(
            "# HELP {name} {help}\n# TYPE {name} {}\n",
            kind.as_str()
        ));
    }

    /// One sample of the family started last.
    pub fn sample(&mut self, name: &str, labels: &[(&str, &str)], value: f64) {
        let mut line = String::from(name);
        if !labels.is_empty() {
            line.push('{');
            for (index, (label, value)) in labels.iter().enumerate() {
                if index > 0 {
                    line.push(',');
                }
                let _ = write!(line, "{label}=\"{}\"", escape(value));
            }
            line.push('}');
        }
        let _ = writeln!(line, " {}", number(value));
        self.push(&line);
    }

    #[must_use]
    pub fn finish(mut self) -> String {
        if self.truncated {
            self.text.push_str(
                "# HELP mm_metrics_truncated The answer reached its size limit and leaves out the rest.\n# TYPE mm_metrics_truncated gauge\nmm_metrics_truncated 1\n",
            );
        }
        self.text
    }
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        String::from(if value > 0.0 { "+Inf" } else { "-Inf" })
    } else {
        format!("{value}")
    }
}
