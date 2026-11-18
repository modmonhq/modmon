//! The review of a deployment before it is applied, and where it is while
//! it is applied.
//!
//! The review shows what `mm deploy plan` prints for the same plan, from the
//! same functions, so every line a plan has shows here too. `enter` applies
//! exactly that plan; while it is applied each node's row says which step it
//! is on, from what the apply itself reports as it goes, without asking the
//! hosts.

use crossterm::event::{KeyCode, KeyEvent};

use crate::domain::rollout::{ProgressStep, RolloutProgress};
use crate::presentation::tui::control::{StackOperation, StartReview};
use crate::presentation::tui::model::Model;
use crate::presentation::tui::update::Action;

/// Rows a page key moves the plan by.
const PAGE: usize = 15;

/// Where the reviewed plan is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewStage {
    /// Shown and waiting for `enter`.
    Reviewing,
    Applying,
    /// The apply ended: what it said.
    Applied(String),
    Failed(String),
}

/// What one node is doing while the plan is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStage {
    /// Its turn has not come.
    Waiting,
    /// The step it is on, in the words `mm deploy apply` prints.
    Working(String),
    /// It finished and recorded the deployment.
    Done,
    RolledBack,
    /// The apply failed while the node was in it, and it did not say it
    /// rolled back: the failure says what the node runs.
    Stopped,
}

/// One node's row while a plan is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRow {
    pub node: String,
    pub stage: NodeStage,
    /// The stacks it applied so far, in order.
    pub stacks: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Review {
    pub review: StartReview,
    pub stage: ReviewStage,
    /// The first line of the plan in view.
    pub scroll: usize,
    /// Hidden with `esc` while the apply goes on; `u` shows it again.
    pub shown: bool,
    pub nodes: Vec<NodeRow>,
}

impl Review {
    #[must_use]
    pub fn new(review: StartReview) -> Self {
        let nodes = review
            .nodes
            .iter()
            .map(|node| NodeRow {
                node: node.clone(),
                stage: NodeStage::Waiting,
                stacks: Vec::new(),
            })
            .collect();
        Self {
            review,
            stage: ReviewStage::Reviewing,
            scroll: 0,
            shown: true,
            nodes,
        }
    }

    /// The question the review asks, or what it shows once applied.
    #[must_use]
    pub fn title(&self, environment: &str) -> String {
        let rollback = self.review.rollback.is_some();
        match (&self.stage, rollback) {
            (ReviewStage::Reviewing, true) => format!("Roll back {environment}?"),
            (ReviewStage::Reviewing, false) if self.review.remote => {
                format!("Deploy to {environment}?")
            }
            (ReviewStage::Reviewing, false) => format!("Start {environment}?"),
            (ReviewStage::Applying, true) => format!("Rolling back {environment}"),
            (ReviewStage::Applying, false) if self.review.remote => {
                format!("Deploying to {environment}")
            }
            (ReviewStage::Applying, false) => format!("Starting {environment}"),
            (ReviewStage::Applied(_) | ReviewStage::Failed(_), _) => {
                format!("Plan {}", self.review.short_hash())
            }
        }
    }

    /// Records a step a node started. A step that still runs, and the
    /// apply's own line, change no row.
    pub fn record(&mut self, progress: &RolloutProgress) {
        if matches!(
            progress.step,
            ProgressStep::StillRunning(_) | ProgressStep::Operation(_)
        ) {
            return;
        }
        let index = match self.nodes.iter().position(|row| row.node == progress.node) {
            Some(index) => index,
            None => {
                self.nodes.push(NodeRow {
                    node: progress.node.clone(),
                    stage: NodeStage::Waiting,
                    stacks: Vec::new(),
                });
                self.nodes.len() - 1
            }
        };
        if progress.step == ProgressStep::Finishing {
            // The nodes finish one after another: the one before is done.
            let finishing = NodeStage::Working(ProgressStep::Finishing.to_string());
            for earlier in &mut self.nodes[..index] {
                if earlier.stage == finishing {
                    earlier.stage = NodeStage::Done;
                }
            }
        }
        let row = &mut self.nodes[index];
        row.stage = match &progress.step {
            // Its turn with the stack is over; it waits for the next one.
            ProgressStep::Healthy(stack) => {
                if !row.stacks.contains(stack) {
                    row.stacks.push(stack.clone());
                }
                NodeStage::Waiting
            }
            ProgressStep::Stack(stack) => NodeStage::Working(format!("applies {stack}")),
            step => NodeStage::Working(step.to_string()),
        };
    }

    /// The apply ended: every node that took part is done, or after a
    /// failure rolled back when it said so.
    pub fn finish(&mut self, result: Result<String, String>) {
        let failed = result.is_err();
        let rolling_back = NodeStage::Working(ProgressStep::RollingBack.to_string());
        for row in &mut self.nodes {
            let began = !row.stacks.is_empty() || matches!(row.stage, NodeStage::Working(_));
            if !began {
                continue;
            }
            row.stage = if row.stage == rolling_back {
                NodeStage::RolledBack
            } else if failed {
                NodeStage::Stopped
            } else {
                NodeStage::Done
            };
        }
        self.stage = match result {
            Ok(summary) => ReviewStage::Applied(summary),
            Err(error) => ReviewStage::Failed(error),
        };
        self.shown = true;
    }

    fn scroll_by(&mut self, delta: isize) {
        let last = self.review.lines.len().saturating_sub(1);
        self.scroll = self.scroll.saturating_add_signed(delta).min(last);
    }
}

/// Opens the review the plan came back with.
pub(super) fn open(model: &mut Model, review: StartReview) {
    model.review = Some(Review::new(review));
}

/// Shows the review of the apply that runs, after `esc` hid it.
pub(super) fn show(model: &mut Model) -> bool {
    match &mut model.review {
        Some(review) if review.stage == ReviewStage::Applying => {
            review.shown = true;
            true
        }
        _ => false,
    }
}

/// A step a node of the apply started.
pub(super) fn progress(model: &mut Model, progress: &RolloutProgress) {
    if let Some(review) = &mut model.review {
        review.record(progress);
    }
}

/// The keys of the review while it is shown.
pub(super) fn key(model: &mut Model, key: KeyEvent) -> Action {
    let Some(review) = model.review.as_mut() else {
        return Action::None;
    };
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => review.scroll_by(-1),
        KeyCode::Down | KeyCode::Char('j') => review.scroll_by(1),
        KeyCode::PageUp => review.scroll_by(-PAGE.cast_signed()),
        KeyCode::PageDown | KeyCode::Char(' ') => review.scroll_by(PAGE.cast_signed()),
        KeyCode::Home | KeyCode::Char('g') => review.scroll = 0,
        KeyCode::End | KeyCode::Char('G') => review.scroll_by(isize::MAX),
        KeyCode::Char('?') => model.help = true,
        KeyCode::Enter | KeyCode::Char('y') if review.stage == ReviewStage::Reviewing => {
            if model.busy.is_some() {
                return Action::None;
            }
            review.stage = ReviewStage::Applying;
            return Action::Stack(StackOperation::Start(review.review.plan_hash.clone()));
        }
        KeyCode::Esc | KeyCode::Char('q' | 'n') if review.stage == ReviewStage::Applying => {
            // The apply goes on; the header says so and `u` shows it again.
            review.shown = false;
        }
        KeyCode::Esc | KeyCode::Char('q' | 'n') | KeyCode::Enter => model.review = None,
        _ => {}
    }
    Action::None
}
