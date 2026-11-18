//! Services that change node within one cluster plan.
//!
//! Each node's section says which services leave it and where they run now.
//! Put together, that is a move: the service starts where it is placed, and
//! only after every node of the plan has applied does it stop where it was.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::NodeDeploymentPlan;
use crate::domain::deployment::NodeProjectPlan;

/// One service leaving one node for others, as the plan will carry it out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceMove {
    pub stack: String,
    pub service: String,
    /// The node the service stops on, last.
    pub from: String,
    /// The nodes that run the service once the plan is applied.
    pub to: Vec<String>,
    /// Those of `to` where the plan starts it; the others run it already.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub starts_on: Vec<String>,
}

impl ServiceMove {
    /// The move in words, in the order it happens.
    #[must_use]
    pub fn describe(&self) -> String {
        let name = format!("{}/{}", self.stack, self.service);
        if self.starts_on.is_empty() {
            format!(
                "{name}: already runs on {}; stops on {} after {} applied",
                self.to.join(", "),
                self.from,
                if self.to.len() == 1 {
                    "that node has"
                } else {
                    "those nodes have"
                }
            )
        } else {
            format!(
                "{name}: starts on {}, then stops on {}",
                self.starts_on.join(", "),
                self.from
            )
        }
    }
}

/// The moves in a plan's node sections, in section order.
#[must_use]
pub fn service_moves(nodes: &[NodeDeploymentPlan]) -> Vec<ServiceMove> {
    let sections = nodes
        .iter()
        .filter_map(|node| {
            Some((
                node.node.as_deref()?,
                node.plan.plan.as_ref()?.node.as_ref()?,
            ))
        })
        .collect::<Vec<_>>();
    moves_between(&sections)
}

/// The moves between named node projects: every service that leaves one
/// for others, with the nodes that start it.
fn moves_between(sections: &[(&str, &NodeProjectPlan)]) -> Vec<ServiceMove> {
    let starts = |node: &str, stack: &str, service: &str| {
        sections
            .iter()
            .find(|(name, _)| *name == node)
            .is_some_and(|(_, section)| {
                section
                    .starting
                    .iter()
                    .any(|starting| starting.stack == stack && starting.service == service)
            })
    };
    let mut moves = Vec::new();
    for (from, project) in sections {
        for leaving in project.moved() {
            moves.push(ServiceMove {
                stack: leaving.stack.clone(),
                service: leaving.service.clone(),
                from: (*from).to_owned(),
                to: leaving.moves_to.clone(),
                starts_on: leaving
                    .moves_to
                    .iter()
                    .filter(|to| starts(to, &leaving.stack, &leaving.service))
                    .cloned()
                    .collect(),
            });
        }
    }
    moves
}

/// Refuses a plan that would stop a moved service without starting it: one
/// that leaves out, with `--nodes`, a node the service moves to.
pub fn check_moves_are_planned(moves: &[ServiceMove], skipped: &[String]) -> Result<(), String> {
    for moved in moves {
        let missing = moved
            .to
            .iter()
            .filter(|node| skipped.contains(node))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "service '{}' of stack '{}' moves from {} to {}, but --nodes leaves {} out, so it would stop on {} without starting there. Add {} to --nodes, or leave {} out as well so the service keeps running where it is",
                moved.service,
                moved.stack,
                moved.from,
                moved.to.join(", "),
                missing.join(", "),
                moved.from,
                missing.join(", "),
                moved.from,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "moves_tests.rs"]
mod tests;
