//! The timing trace: with `MM_TIMING=1`, every phase of a command is written
//! to standard error when it ends, with the time since the process started
//! and how long the phase took.
//!
//! ```text
//! timing  +   412.6 ms    398.1 ms  ssh.connect  node 'app-1' (203.0.113.11:22)
//! ```

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Instant;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

/// The environment variable that turns the trace on.
pub const TIMING_ENVIRONMENT: &str = "MM_TIMING";

/// Prints phases recorded on `crate::app::timing::TARGET`.
pub struct TimingLayer {
    started: Instant,
}

impl TimingLayer {
    /// The layer, when `MM_TIMING` is set to anything but `0` or nothing.
    /// Times count from this call, so make it first thing in `main`.
    #[must_use]
    pub fn from_environment() -> Option<Self> {
        let value = std::env::var_os(TIMING_ENVIRONMENT)?;
        (!value.is_empty() && value != "0").then(|| Self {
            started: Instant::now(),
        })
    }

    /// The filter that hands this layer the timing target and nothing else.
    #[must_use]
    pub fn filter() -> tracing_subscriber::filter::Targets {
        tracing_subscriber::filter::Targets::new()
            .with_target(crate::app::timing::TARGET, tracing::Level::TRACE)
    }

    fn write(&self, line: &str) {
        // The trace is best effort: a closed standard error must not fail
        // the command it measures.
        writeln!(std::io::stderr().lock(), "{line}").unwrap_or_default();
    }
}

/// When a phase began and what it is called.
struct Phase {
    began: Instant,
    label: Label,
}

#[derive(Default)]
struct Label {
    name: String,
    detail: String,
}

impl Visit for Label {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "name" | "mark" => value.clone_into(&mut self.name),
            "detail" => value.clone_into(&mut self.detail),
            _ => {}
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let text = match field.name() {
            "name" | "mark" => &mut self.name,
            "detail" => &mut self.detail,
            _ => return,
        };
        text.clear();
        if write!(text, "{value:?}").is_err() {
            text.clear();
        }
    }
}

impl Label {
    /// The name and what it is about, on one line of bounded length.
    fn line(&self) -> String {
        let detail = self
            .detail
            .chars()
            .map(|character| {
                if character.is_control() {
                    ' '
                } else {
                    character
                }
            })
            .take(160)
            .collect::<String>();
        if detail.is_empty() {
            self.name.clone()
        } else {
            format!("{}  {detail}", self.name)
        }
    }
}

fn milliseconds(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

impl<S> Layer<S> for TimingLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let Some(span) = context.span(id) else {
            return;
        };
        let mut label = Label::default();
        attributes.record(&mut label);
        span.extensions_mut().insert(Phase {
            began: Instant::now(),
            label,
        });
    }

    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut label = Label::default();
        event.record(&mut label);
        self.write(&format!(
            "timing  +{:8.1} ms               {}",
            milliseconds(self.started.elapsed()),
            label.line()
        ));
    }

    fn on_close(&self, id: Id, context: Context<'_, S>) {
        let Some(span) = context.span(&id) else {
            return;
        };
        let extensions = span.extensions();
        let Some(phase) = extensions.get::<Phase>() else {
            return;
        };
        self.write(&format!(
            "timing  +{:8.1} ms {:8.1} ms  {}",
            milliseconds(phase.began.duration_since(self.started)),
            milliseconds(phase.began.elapsed()),
            phase.label.line()
        ));
    }
}
