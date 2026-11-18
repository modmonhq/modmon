//! What a command that waits is doing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One thing a running command says about its work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", tag = "event")]
pub enum ProgressEvent {
    /// A step begins.
    Started { step: String, label: String },
    /// Where one node is within a step.
    Node {
        step: String,
        node: String,
        state: NodeState,
        /// What the node is on, such as `running the playbook`.
        detail: Option<String>,
    },
    /// How much of a step is done, when that can be counted.
    Amount {
        step: String,
        done: u64,
        total: Option<u64>,
        unit: AmountUnit,
    },
    /// What the step is on right now, when it has no nodes.
    Detail { step: String, detail: String },
    /// The step ends.
    Finished {
        step: String,
        outcome: StepOutcome,
        summary: Option<String>,
    },
}

/// Where a node is within a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NodeState {
    /// Its turn has not come.
    Waiting,
    Working,
    Done,
    Failed,
    /// The step leaves it alone.
    Skipped,
}

/// How a step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StepOutcome {
    Done,
    Failed,
    /// The step had nothing to do.
    Skipped,
}

/// What an amount counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AmountUnit {
    /// Things, such as nodes or secrets.
    Items,
    Bytes,
}

impl ProgressEvent {
    #[must_use]
    pub fn started(step: &str, label: impl Into<String>) -> Self {
        Self::Started {
            step: step.to_owned(),
            label: label.into(),
        }
    }

    #[must_use]
    pub fn node(step: &str, node: &str, state: NodeState, detail: Option<&str>) -> Self {
        Self::Node {
            step: step.to_owned(),
            node: node.to_owned(),
            state,
            detail: detail.map(str::to_owned),
        }
    }

    #[must_use]
    pub fn items(step: &str, done: u64, total: u64) -> Self {
        Self::Amount {
            step: step.to_owned(),
            done,
            total: Some(total),
            unit: AmountUnit::Items,
        }
    }

    #[must_use]
    pub fn bytes(step: &str, done: u64, total: Option<u64>) -> Self {
        Self::Amount {
            step: step.to_owned(),
            done,
            total,
            unit: AmountUnit::Bytes,
        }
    }

    #[must_use]
    pub fn detail(step: &str, detail: impl Into<String>) -> Self {
        Self::Detail {
            step: step.to_owned(),
            detail: detail.into(),
        }
    }

    #[must_use]
    pub fn finished(step: &str, outcome: StepOutcome, summary: Option<String>) -> Self {
        Self::Finished {
            step: step.to_owned(),
            outcome,
            summary,
        }
    }

    /// The step this event is about.
    #[must_use]
    pub fn step(&self) -> &str {
        match self {
            Self::Started { step, .. }
            | Self::Node { step, .. }
            | Self::Amount { step, .. }
            | Self::Detail { step, .. }
            | Self::Finished { step, .. } => step,
        }
    }
}
