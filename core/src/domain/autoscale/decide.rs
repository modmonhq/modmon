//! The rules every node applies the same way: who decides, how many
//! containers each node runs, when the count changes, and what one node
//! does next about its own containers.

use std::time::Duration;

use super::{Rule, ScalePolicy, ScaleReport, Want};
use crate::domain::metrics::{Percent, Tally};

/// How many of `count` containers run on `node`, one of `nodes` (the
/// service's placement, in order). Fewest first: each node gets
/// `count / nodes`, and the first `count % nodes` nodes one more. So a
/// container that is added goes to the first node with the fewest, and
/// one that is removed comes from the last node with the most. A node
/// that is not in `nodes` runs none.
#[must_use]
pub fn share(count: u32, nodes: &[String], node: &str) -> u32 {
    let Some(index) = nodes.iter().position(|name| name == node) else {
        return 0;
    };
    let total = u32::try_from(nodes.len()).unwrap_or(u32::MAX);
    let index = u32::try_from(index).unwrap_or(u32::MAX);
    count / total + u32::from(index < count % total)
}

/// The node that decides for a service placed on `nodes`: the first one
/// that answers. Every node computes it from what it hears, so while the
/// nodes hear each other they agree, and when the decider stops
/// answering the next node takes over.
#[must_use]
pub fn decider(nodes: &[String], answers: impl Fn(&str) -> bool) -> Option<&str> {
    nodes.iter().map(String::as_str).find(|node| answers(node))
}

/// The want every node goes by: the one with the highest generation. Two
/// of the same generation (two deciders at once, while the nodes did not
/// hear each other) are settled by placement order: the earlier node's
/// wins, as it would have been the decider.
#[must_use]
pub fn newest_want<'a>(
    wants: impl IntoIterator<Item = &'a Want>,
    nodes: &[String],
) -> Option<&'a Want> {
    let rank = |want: &Want| {
        let place = nodes
            .iter()
            .position(|node| *node == want.by)
            .unwrap_or(usize::MAX);
        (want.generation, std::cmp::Reverse(place))
    };
    wants.into_iter().max_by_key(|want| rank(want))
}

/// The cluster's step of a service on the decider: its own newest tally
/// and the newest one of every node `heard` from recently. A node not heard
/// from is left out, so the average is over the containers that reported:
/// it never counts as zero, and the step is never held up waiting for it.
#[must_use]
pub fn cluster_tally(own: Tally, heard: &[(&str, &ScaleReport)]) -> Tally {
    heard
        .iter()
        .fold(own, |sum, (_, report)| sum.plus(report.tally))
}

/// Whether every node that answers (`answering`, `me` among them) runs its
/// share of `count`, every container healthy and none leaving. A node that
/// answers and has said nothing about the service yet has not settled it;
/// one that does not answer is not waited for.
#[must_use]
pub fn settled_everywhere(
    count: u32,
    nodes: &[String],
    (me, own): (&str, &ScaleReport),
    heard: &[(&str, &ScaleReport)],
    answering: &[&str],
) -> bool {
    answering.iter().all(|name| {
        let report = if *name == me {
            Some(own)
        } else {
            heard
                .iter()
                .find(|(node, _)| node == name)
                .map(|(_, report)| *report)
        };
        report.is_some_and(|report| report.settled && report.running == share(count, nodes, name))
    })
}

/// What the decider knows at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    /// The count the nodes go by now.
    pub count: u32,
    /// For how long the up condition has held without a break.
    pub up_held: Duration,
    /// For how long the down condition has held without a break.
    pub down_held: Duration,
    /// For how long every answering node has run its share, every
    /// container healthy and none leaving; `None` while one has not.
    pub settled: Option<Duration>,
    /// For how long this node has been the decider.
    pub deciding: Duration,
    /// The average over each rule's window, for the reason.
    pub up_average: Option<Percent>,
    pub down_average: Option<Percent>,
}

/// What the decider does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Change the count to `to`, for `reason` ("cpu 82% above 75% for 30s").
    Change { to: u32, reason: String },
    /// Keep the count; `reason` says why nothing changes now.
    Keep { reason: String },
}

/// One step of the decider. One container at a time: a change waits until
/// the last one has settled on every node, and then a condition has to
/// hold for its whole `for:` again, counted from that moment. A node that
/// has just become the decider also waits one whole `for:` (the flapping
/// guard), as does every node after `mmd` restarts, whose store is empty.
#[must_use]
pub fn decide(policy: &ScalePolicy, moment: &Moment) -> Decision {
    let metric = policy.metric.as_str();
    let bounded = policy.bounded(moment.count);
    if bounded != moment.count {
        return Decision::Change {
            to: bounded,
            reason: format!("the range is now {}..{}", policy.min, policy.max),
        };
    }
    let Some(settled) = moment.settled else {
        return Decision::Keep {
            reason: "waiting for the last change to settle on every node".to_owned(),
        };
    };
    // A rule's run counts from the moment the count settled and this node
    // decided, whichever came last.
    let quiet = settled.min(moment.deciding);
    let up_run = moment.up_held.min(quiet);
    let down_run = moment.down_held.min(quiet);
    if up_run >= policy.up.hold() {
        let reason = above(metric, moment.up_average, policy.up);
        return if moment.count < policy.max {
            Decision::Change {
                to: moment.count + 1,
                reason,
            }
        } else {
            Decision::Keep {
                reason: format!("{reason}, and {} is `scale.max`", policy.max),
            }
        };
    }
    if down_run >= policy.down.hold() {
        let reason = below(metric, moment.down_average, policy.down);
        return if moment.count > policy.min {
            Decision::Change {
                to: moment.count - 1,
                reason,
            }
        } else {
            Decision::Keep {
                reason: format!("{reason}, and {} is `scale.min`", policy.min),
            }
        };
    }
    let average = |average: Option<Percent>| {
        average.map_or_else(
            || "no measurement".to_owned(),
            |average| average.to_string(),
        )
    };
    let reason = if moment.deciding < policy.up.hold().min(policy.down.hold()) {
        "watching one whole `for:` before changing anything".to_owned()
    } else if !moment.up_held.is_zero() {
        format!(
            "{metric} {} above {} for {} of {}",
            average(moment.up_average),
            policy.up.threshold,
            span(up_run),
            span(policy.up.hold())
        )
    } else if !moment.down_held.is_zero() {
        format!(
            "{metric} {} below {} for {} of {}",
            average(moment.down_average),
            policy.down.threshold,
            span(down_run),
            span(policy.down.hold())
        )
    } else {
        format!(
            "{metric} {}, between {} and {}",
            average(moment.up_average.or(moment.down_average)),
            policy.down.threshold,
            policy.up.threshold
        )
    };
    Decision::Keep { reason }
}

fn above(metric: &str, average: Option<Percent>, rule: Rule) -> String {
    format!(
        "{metric} {} above {} for {}",
        average.map_or_else(|| "average".to_owned(), |average| average.to_string()),
        rule.threshold,
        span(rule.hold())
    )
}

fn below(metric: &str, average: Option<Percent>, rule: Rule) -> String {
    format!(
        "{metric} {} below {} for {}",
        average.map_or_else(|| "average".to_owned(), |average| average.to_string()),
        rule.threshold,
        span(rule.hold())
    )
}

/// `30s`, `5m`, `1h30m`.
fn span(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    let mut text = String::new();
    if hours > 0 {
        text.push_str(&format!("{hours}h"));
    }
    if minutes > 0 {
        text.push_str(&format!("{minutes}m"));
    }
    if seconds > 0 || text.is_empty() {
        text.push_str(&format!("{seconds}s"));
    }
    text
}

/// One container of a service on this node, as the engine shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnContainer {
    pub id: String,
    /// When the engine created it, in a form that sorts by time.
    pub created: String,
    pub running: bool,
    /// Running and passing its health check, when it has one.
    pub healthy: bool,
    /// Leaving this node (`domain::leaving`).
    pub leaving: bool,
}

impl OwnContainer {
    /// A container as the engine shows it (`running`, and `healthy`: its
    /// health check passes or it has none), with its leave when it has one:
    /// `Some(true)` restarts it in place, `Some(false)` removes it. One
    /// that restarts is starting until it is answered again, also in the
    /// moment the engine shows it stopped: it is no missing container to
    /// start another for.
    #[must_use]
    pub fn seen(
        (id, created): (String, String),
        running: bool,
        healthy: bool,
        leave: Option<bool>,
    ) -> Self {
        let restarting = leave == Some(true);
        Self {
            id,
            created,
            running: running || restarting,
            healthy: running && healthy && leave.is_none(),
            leaving: leave == Some(false),
        }
    }
}

/// What one node does next about its own containers of a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeStep {
    /// Start one more: Compose is asked for `scale_to` containers, which
    /// counts every container of the service it knows, leaving ones too.
    Start { scale_to: u32 },
    /// The newest container leaves.
    Leave { id: String },
    /// Something is under way; nothing new starts.
    Wait { reason: String },
    /// The node runs its share, every container healthy, none leaving.
    Settled,
}

/// The next step for a node whose share is `share`, one container at a
/// time: a new container counts once it is healthy, and a leaving one
/// once it is gone.
#[must_use]
pub fn node_step(share: u32, containers: &[OwnContainer]) -> NodeStep {
    let active = containers
        .iter()
        .filter(|container| container.running && !container.leaving)
        .collect::<Vec<_>>();
    let count = u32::try_from(active.len()).unwrap_or(u32::MAX);
    if active.iter().any(|container| !container.healthy) {
        return NodeStep::Wait {
            reason: "a container is starting".to_owned(),
        };
    }
    let leaving = containers.iter().any(|container| container.leaving);
    if count < share {
        return NodeStep::Start {
            scale_to: u32::try_from(containers.len())
                .unwrap_or(u32::MAX)
                .saturating_add(1),
        };
    }
    if leaving {
        return NodeStep::Wait {
            reason: "a container is leaving".to_owned(),
        };
    }
    let newest = active.iter().max_by(|left, right| {
        left.created
            .cmp(&right.created)
            .then_with(|| left.id.cmp(&right.id))
    });
    if count > share
        && let Some(newest) = newest
    {
        return NodeStep::Leave {
            id: newest.id.clone(),
        };
    }
    NodeStep::Settled
}
