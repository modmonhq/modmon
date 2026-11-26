//! Which rollout steps a node's `mmd` knows. A step a node does not know
//! would be refused whole, so the client only plans the newer steps when
//! every node of the plan says it knows them (`NodeFacts::rollout_steps`).

/// The rollout steps this release adds, as a node lists them.
pub const NEWER_ROLLOUT_STEPS: [&str; 2] = ["pull", "stacks"];

/// Whether a node that lists `steps` knows every newer step.
#[must_use]
pub fn knows_newer_steps(steps: &[String]) -> bool {
    NEWER_ROLLOUT_STEPS
        .iter()
        .all(|wanted| steps.iter().any(|step| step == wanted))
}

/// What a node says about its rollout steps, as the client plans with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeSteps {
    pub node: String,
    /// Its `mmd`'s release, for the plan to name; empty when unknown.
    pub daemon_version: String,
    /// The steps beyond the first ones it lists; empty for an `mmd` that
    /// lists none or did not answer.
    pub steps: Vec<String>,
}
