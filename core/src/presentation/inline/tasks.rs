//! The live task list.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::text::{Line, Span};

use super::style::{self, Palette};
use super::text;
use crate::domain::progress::{AmountUnit, NodeState, ProgressEvent, StepOutcome};
use crate::presentation::tui::Tone;

/// Where a step is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone)]
struct NodeRow {
    name: String,
    state: NodeState,
    detail: Option<String>,
    since: Instant,
    took: Option<Duration>,
}

#[derive(Debug, Clone, Copy)]
struct Amount {
    done: u64,
    total: Option<u64>,
    unit: AmountUnit,
    /// When the first amount came and what it was.
    first: (Instant, u64),
}

#[derive(Debug, Clone)]
struct Step {
    key: String,
    label: String,
    summary: Option<String>,
    state: StepState,
    started: Option<Instant>,
    took: Option<Duration>,
    nodes: Vec<NodeRow>,
    amount: Option<Amount>,
    detail: Option<String>,
}

#[derive(Debug, Clone)]
enum Record {
    Step(usize),
    Note(Option<Tone>, String),
}

/// A command's steps and what is still to be written above the viewport.
#[derive(Debug, Clone)]
pub struct Tasks {
    steps: Vec<Step>,
    records: VecDeque<Record>,
    palette: Palette,
}

/// The widest node name column; longer names are cut.
const NODE_COLUMN: usize = 24;
/// Columns of a progress bar.
const BAR: usize = 24;

impl Tasks {
    #[must_use]
    pub fn new(palette: Palette) -> Self {
        Self {
            steps: Vec::new(),
            records: VecDeque::new(),
            palette,
        }
    }

    /// Adds a step that has not started, so the list shows what is to come.
    pub fn plan(&mut self, key: &str, label: &str) {
        if self.position(key).is_none() {
            self.steps.push(Step::new(key, label));
        }
    }

    /// Writes `text` above the viewport, in order with the finished steps.
    pub fn note(&mut self, tone: Option<Tone>, text: impl Into<String>) {
        self.records.push_back(Record::Note(tone, text.into()));
    }

    fn position(&self, key: &str) -> Option<usize> {
        self.steps.iter().position(|step| step.key == key)
    }

    fn step_mut(&mut self, key: &str) -> &mut Step {
        let index = self.position(key).unwrap_or_else(|| {
            self.steps.push(Step::new(key, key));
            self.steps.len() - 1
        });
        &mut self.steps[index]
    }

    /// Takes in one event at `now`.
    pub fn apply(&mut self, event: ProgressEvent, now: Instant) {
        match event {
            ProgressEvent::Started { step, label } => {
                let step = self.step_mut(&step);
                step.label = label;
                step.state = StepState::Running;
                step.started = Some(now);
            }
            ProgressEvent::Node {
                step,
                node,
                state,
                detail,
            } => self.step_mut(&step).node(node, state, detail, now),
            ProgressEvent::Amount {
                step,
                done,
                total,
                unit,
            } => {
                let step = self.step_mut(&step);
                let first = step.amount.map_or((now, done), |amount| amount.first);
                step.amount = Some(Amount {
                    done,
                    total,
                    unit,
                    first,
                });
            }
            ProgressEvent::Detail { step, detail } => self.step_mut(&step).detail = Some(detail),
            ProgressEvent::Finished {
                step,
                outcome,
                summary,
            } => self.finish(&step, outcome, summary, now),
        }
    }

    fn finish(&mut self, key: &str, outcome: StepOutcome, summary: Option<String>, now: Instant) {
        let step = self.step_mut(key);
        if matches!(
            step.state,
            StepState::Done | StepState::Failed | StepState::Skipped
        ) {
            return;
        }
        step.state = match outcome {
            StepOutcome::Done => StepState::Done,
            StepOutcome::Failed => StepState::Failed,
            StepOutcome::Skipped => StepState::Skipped,
        };
        step.took = step.started.map(|started| now.duration_since(started));
        step.summary = summary;
        let index = self.position(key).unwrap_or_default();
        self.records.push_back(Record::Step(index));
    }

    /// Ends every running step as failed: the command was interrupted.
    pub fn interrupt(&mut self, now: Instant) {
        let running = self
            .steps
            .iter()
            .filter(|step| step.state == StepState::Running)
            .map(|step| step.key.clone())
            .collect::<Vec<_>>();
        for key in running {
            let label = format!("{} (stopped)", self.step_mut(&key).label);
            self.finish(&key, StepOutcome::Failed, Some(label), now);
        }
    }

    /// Whether any step runs.
    #[must_use]
    pub fn running(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.state == StepState::Running)
    }

    /// The records not written yet, laid out `width` columns wide.
    pub fn take_records(&mut self, width: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        while let Some(record) = self.records.pop_front() {
            match record {
                Record::Step(index) => lines.extend(self.steps[index].record(self.palette, width)),
                Record::Note(tone, text) => lines.push(note_line(self.palette, tone, &text, width)),
            }
        }
        lines
    }

    /// The records not written yet, as plain lines.
    pub fn take_plain(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        while let Some(record) = self.records.pop_front() {
            match record {
                Record::Step(index) => lines.extend(self.steps[index].plain()),
                Record::Note(_, text) => {
                    lines.push(crate::presentation::output::terminal_safe(&text))
                }
            }
        }
        lines
    }

    /// What the viewport shows at `now`.
    #[must_use]
    pub fn live(&self, now: Instant, tick: u64, width: usize, rows: usize) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for step in self
            .steps
            .iter()
            .filter(|step| step.state == StepState::Running)
        {
            lines.extend(step.live(self.palette, now, tick, width));
        }
        let pending = self
            .steps
            .iter()
            .filter(|step| step.state == StepState::Pending)
            .collect::<Vec<_>>();
        let room = rows.saturating_sub(lines.len());
        let shown = if pending.len() > room {
            room.saturating_sub(1)
        } else {
            pending.len()
        };
        for step in &pending[..shown] {
            lines.push(Line::from(text::fit(
                vec![
                    Span::styled(format!("{} ", style::PENDING), self.palette.dim()),
                    Span::styled(step.label.clone(), self.palette.dim()),
                ],
                width,
            )));
        }
        if shown < pending.len() && room > 0 {
            lines.push(Line::styled(
                format!(
                    "  {} more {}",
                    pending.len() - shown,
                    if pending.len() - shown == 1 {
                        "step"
                    } else {
                        "steps"
                    }
                ),
                self.palette.dim(),
            ));
        }
        lines.truncate(rows);
        lines
    }
}

impl Step {
    fn new(key: &str, label: &str) -> Self {
        Self {
            key: key.to_owned(),
            label: label.to_owned(),
            summary: None,
            state: StepState::Pending,
            started: None,
            took: None,
            nodes: Vec::new(),
            amount: None,
            detail: None,
        }
    }

    fn node(&mut self, name: String, state: NodeState, detail: Option<String>, now: Instant) {
        if self.state == StepState::Pending {
            self.state = StepState::Running;
            self.started = Some(now);
        }
        let index = self
            .nodes
            .iter()
            .position(|row| row.name == name)
            .unwrap_or_else(|| {
                self.nodes.push(NodeRow {
                    name,
                    state: NodeState::Waiting,
                    detail: None,
                    since: now,
                    took: None,
                });
                self.nodes.len() - 1
            });
        let row = &mut self.nodes[index];
        if state == NodeState::Working && row.state != NodeState::Working {
            row.since = now;
            row.took = None;
        }
        if matches!(state, NodeState::Done | NodeState::Failed) && row.took.is_none() {
            row.took = Some(now.duration_since(row.since));
        }
        row.state = state;
        if detail.is_some() || state == NodeState::Working {
            row.detail = detail;
        }
    }

    fn glyph(&self, palette: Palette) -> Span<'static> {
        match self.state {
            StepState::Done => Span::styled(style::DONE, palette.tone(Tone::Ok)),
            StepState::Failed => Span::styled(style::FAILED, palette.tone(Tone::Bad)),
            StepState::Skipped => Span::styled(style::SKIPPED, palette.dim()),
            StepState::Pending => Span::styled(style::PENDING, palette.dim()),
            StepState::Running => Span::styled(style::spinner(0), palette.accent()),
        }
    }

    fn title(&self) -> String {
        self.summary.clone().unwrap_or_else(|| self.label.clone())
    }

    /// The lines a finished step leaves above the viewport.
    fn record(&self, palette: Palette, width: usize) -> Vec<Line<'static>> {
        let time = self
            .took
            .map(|took| vec![Span::styled(text::duration(took), palette.dim())])
            .unwrap_or_default();
        let title_style = match self.state {
            StepState::Skipped => palette.dim(),
            _ => palette.text(),
        };
        let mut lines = vec![text::row(
            vec![
                self.glyph(palette),
                Span::raw(" "),
                Span::styled(self.title(), title_style),
            ],
            time,
            width,
        )];
        if self.state == StepState::Failed {
            let column = self.node_column();
            for node in self
                .nodes
                .iter()
                .filter(|node| node.state == NodeState::Failed)
            {
                lines.push(node.line(palette, column, None, width));
            }
        }
        lines
    }

    fn plain(&self) -> Vec<String> {
        let glyph = match self.state {
            StepState::Done => style::DONE,
            StepState::Failed => style::FAILED,
            StepState::Skipped => style::SKIPPED,
            StepState::Pending | StepState::Running => style::PENDING,
        };
        let title = crate::presentation::output::terminal_safe(&self.title());
        let mut lines = vec![match self.took {
            Some(took) => format!("{glyph} {title} ({})", text::duration(took)),
            None => format!("{glyph} {title}"),
        }];
        if self.state == StepState::Failed {
            for node in self
                .nodes
                .iter()
                .filter(|node| node.state == NodeState::Failed)
            {
                lines.push(format!(
                    "    {} {}: {}",
                    style::FAILED,
                    crate::presentation::output::terminal_safe(&node.name),
                    crate::presentation::output::terminal_safe(
                        node.detail.as_deref().unwrap_or("failed")
                    )
                ));
            }
        }
        lines
    }

    fn node_column(&self) -> usize {
        self.nodes
            .iter()
            .map(|node| text::width(&node.name))
            .max()
            .unwrap_or(0)
            .min(NODE_COLUMN)
    }

    fn live(&self, palette: Palette, now: Instant, tick: u64, width: usize) -> Vec<Line<'static>> {
        let elapsed = self
            .started
            .map(|started| now.duration_since(started))
            .unwrap_or_default();
        let mut left = vec![
            Span::styled(style::spinner(tick), palette.accent()),
            Span::raw(" "),
            Span::styled(self.label.clone(), palette.strong()),
        ];
        if let Some(detail) = &self.detail {
            left.push(Span::styled(format!("  {detail}"), palette.dim()));
        }
        let mut lines = vec![text::row(
            left,
            vec![Span::styled(text::duration(elapsed), palette.dim())],
            width,
        )];
        let column = self.node_column();
        for node in &self.nodes {
            lines.push(node.line(palette, column, Some((now, tick)), width));
        }
        if let Some(amount) = &self.amount {
            lines.push(amount.line(palette, now, width));
        }
        lines
    }
}

impl NodeRow {
    /// `    app-1   ⠹ installing podman        8.1s`.
    fn line(
        &self,
        palette: Palette,
        column: usize,
        live: Option<(Instant, u64)>,
        width: usize,
    ) -> Line<'static> {
        let (glyph, glyph_style) = match self.state {
            NodeState::Working => (
                live.map_or(style::PENDING, |(_, tick)| style::spinner(tick)),
                palette.accent(),
            ),
            NodeState::Done => (style::DONE, palette.tone(Tone::Ok)),
            NodeState::Failed => (style::FAILED, palette.tone(Tone::Bad)),
            NodeState::Waiting => (style::WAITING, palette.dim()),
            NodeState::Skipped => (style::SKIPPED, palette.dim()),
        };
        let detail = self.detail.clone().unwrap_or_else(|| {
            match self.state {
                NodeState::Waiting => "waiting",
                NodeState::Working => "working",
                NodeState::Done => "done",
                NodeState::Failed => "failed",
                NodeState::Skipped => "left alone",
            }
            .to_owned()
        });
        let detail_style = match self.state {
            NodeState::Failed => palette.tone(Tone::Bad),
            NodeState::Working => palette.text(),
            _ => palette.dim(),
        };
        let time = match (self.state, live) {
            (NodeState::Working, Some((now, _))) => Some(now.duration_since(self.since)),
            (NodeState::Done | NodeState::Failed, _) => self.took,
            _ => None,
        };
        text::row(
            vec![
                Span::raw("    "),
                Span::styled(text::pad(&self.name, column), palette.text()),
                Span::raw("  "),
                Span::styled(glyph, glyph_style),
                Span::raw(" "),
                Span::styled(detail, detail_style),
            ],
            time.map(|time| vec![Span::styled(text::duration(time), palette.dim())])
                .unwrap_or_default(),
            width,
        )
    }
}

impl Amount {
    /// `    ━━━━━━━━──────  12 of 68 · 2.1 MB/s · 4s left`.
    fn line(&self, palette: Palette, now: Instant, width: usize) -> Line<'static> {
        let quantity = |value: u64| match self.unit {
            AmountUnit::Bytes => crate::domain::delivery::human_size(value),
            AmountUnit::Items => value.to_string(),
        };
        let mut spans = vec![Span::raw("    ")];
        let mut label = match self.total {
            Some(total) => format!("{} of {}", quantity(self.done), quantity(total)),
            None => quantity(self.done),
        };
        if let Some(total) = self.total.filter(|total| *total > 0) {
            let filled = usize::try_from(self.done.min(total) * BAR as u64 / total).unwrap_or(BAR);
            spans.push(Span::styled("━".repeat(filled), palette.accent()));
            spans.push(Span::styled("─".repeat(BAR - filled), palette.dim()));
            spans.push(Span::raw("  "));
        }
        let (since, first) = self.first;
        let elapsed = now.duration_since(since).as_secs_f64();
        if self.unit == AmountUnit::Bytes && elapsed >= 1.0 && self.done > first {
            let rate = (self.done - first) as f64 / elapsed;
            let per_second = rate as u64;
            label.push_str(&format!(
                " · {}/s",
                crate::domain::delivery::human_size(per_second)
            ));
            if let Some(total) = self.total.filter(|total| *total > self.done) {
                let left = Duration::from_secs_f64((total - self.done) as f64 / rate);
                label.push_str(&format!(" · {} left", text::duration(left)));
            }
        }
        spans.push(Span::styled(label, palette.dim()));
        Line::from(text::fit(spans, width))
    }
}

fn note_line(palette: Palette, tone: Option<Tone>, text: &str, width: usize) -> Line<'static> {
    let mut spans = Vec::new();
    if let Some(tone) = tone {
        spans.push(Span::styled(
            format!("{} ", tone.glyph()),
            palette.tone(tone),
        ));
    }
    spans.push(Span::styled(text.to_owned(), palette.text()));
    Line::from(text::fit(spans, width))
}
