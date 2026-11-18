//! What an apply is doing right now, node by node.
//!
//! The client orders every step of a deployment, so it knows where each
//! node is without asking: it says so each time a node starts a step. The
//! command line prints each step as a line; the terminal UI keeps the last
//! step of every node and draws them as rows.

use std::fmt;

/// One node starting one step of an apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutProgress {
    pub node: String,
    pub step: ProgressStep,
}

/// The step a node starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressStep {
    /// The node's whole section is applied in one request: an access sync.
    Applying,
    /// The node begins its part of the rollout.
    Preparing,
    /// The `start-first` services the node started next to the old.
    StartingNext {
        stack: String,
        services: Vec<String>,
    },
    /// The node applies a stack; it is done once the stack is healthy there
    /// and every node resolves its new containers.
    Stack(String),
    /// The stack is healthy on the node and every node resolves it: the
    /// node's turn with it is over.
    Healthy(String),
    /// Every stack is healthy everywhere: the node removes what it no
    /// longer runs and records the deployment.
    Finishing,
    /// The node returns to what it ran before.
    RollingBack,
    /// Anything else the apply says about the node, such as the shared
    /// volumes it creates or the services it stops after a move.
    Note(String),
    /// The node's step still runs: how long, and what its host says about
    /// it. Not a new step: the node is still where it was.
    StillRunning(String),
    /// What the whole apply is, before any node starts: the operation's ID
    /// and how to look it up or stop it. Not about one node.
    Operation(String),
}

impl RolloutProgress {
    #[must_use]
    pub fn new(node: impl Into<String>, step: ProgressStep) -> Self {
        Self {
            node: node.into(),
            step,
        }
    }
}

impl fmt::Display for ProgressStep {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applying => formatter.write_str("applying"),
            Self::Preparing => formatter.write_str("preparing"),
            Self::StartingNext { stack, services } => write!(
                formatter,
                "{stack}: started the new {} next to the old",
                services.join(", ")
            ),
            Self::Stack(stack) => formatter.write_str(stack),
            Self::Healthy(stack) => write!(formatter, "{stack}: healthy"),
            Self::Finishing => formatter.write_str("finishing"),
            Self::RollingBack => formatter.write_str("rolling back"),
            Self::Note(note) | Self::StillRunning(note) | Self::Operation(note) => {
                formatter.write_str(note)
            }
        }
    }
}

/// The line `mm deploy apply` prints for it: `app-2: preparing`, the
/// node's name alone for a node whose section is applied at once, or the
/// operation's line alone.
impl fmt::Display for RolloutProgress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.step {
            ProgressStep::Applying => formatter.write_str(&self.node),
            ProgressStep::Operation(line) => formatter.write_str(line),
            step => write!(formatter, "{}: {step}", self.node),
        }
    }
}
