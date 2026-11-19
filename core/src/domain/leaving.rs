//! A container leaving a cluster node without dropping a request.
//!
//! One way for every reason a container goes while its service stays: a
//! rolling update replaces it, its service moves to another node, it is
//! scaled away, or an operator takes it out with `mmd container leave`.
//! In this order, and never shortened by a timeout elsewhere:
//!
//! 1. The container is marked as leaving: its node stops answering names
//!    with it and tells the other nodes, whose name answers and ingress
//!    proxies drop it. Every node confirms that it did.
//! 2. It keeps running for its drain time (the service's `drain`, default
//!    one minute) while requests in flight and lookups already answered
//!    finish.
//! 3. It gets its stop signal and its `stop_grace_period`, and is removed;
//!    or, for a restart, it starts again and comes back into the answers
//!    once it is healthy (the answers only ever hold healthy containers).
//!
//! The node keeps each leave in a file, so an `mmd` that restarts in the
//! middle carries on where it was: the container is never stopped early and
//! never left behind.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long a leaving container keeps serving when its service sets no
/// `drain`.
pub const DEFAULT_DRAIN: Duration = Duration::from_secs(60);

/// The longest `drain`: a rolling update waits for it inside one step.
pub const MAX_DRAIN: Duration = Duration::from_secs(15 * 60);

/// The label that carries a service's `drain` on its containers, so a
/// leave started on the node knows it without the configuration.
pub const DRAIN_LABEL: &str = "io.mm.drain";

/// The label a node's status puts on a container that is leaving; the
/// engine's own labels never carry it.
pub const LEAVING_LABEL: &str = "io.mm.leaving";

/// A service's drain time from its `drain` setting.
pub fn drain_of(spec: Option<&crate::domain::config::DurationSpec>) -> Result<Duration, String> {
    let Some(spec) = spec else {
        return Ok(DEFAULT_DRAIN);
    };
    let drain = spec.parse()?;
    if drain > MAX_DRAIN {
        return Err(format!(
            "is {}, more than the longest drain of {}m; lower it",
            spec.0,
            MAX_DRAIN.as_secs() / 60
        ));
    }
    Ok(drain)
}

/// One container that is leaving its node, as the node keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LeaveRecord {
    /// The engine's id of the container, whole.
    pub id: String,
    /// The engine's name of the container, for people.
    pub name: String,
    pub drain_ms: u64,
    /// When the leave began.
    pub marked_unix_ms: i64,
    /// When every node had confirmed that it no longer sends the container
    /// anything; the drain counts from here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_unix_ms: Option<i64>,
    /// The container is restarted in place of being removed: after its
    /// drain it gets its stop signal and starts again, and comes back into
    /// the answers once it is healthy.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restart: bool,
}

/// Where a leave stands at a moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveStage {
    /// Waiting for every node to confirm.
    Marked,
    /// Confirmed; the container serves what it still has until then.
    Draining { until_unix_ms: i64 },
    /// The drain is over: the container gets its stop signal now.
    Stopping,
}

impl LeaveRecord {
    #[must_use]
    pub fn new(id: &str, name: &str, drain: Duration, now_unix_ms: i64) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            drain_ms: u64::try_from(drain.as_millis()).unwrap_or(u64::MAX),
            marked_unix_ms: now_unix_ms,
            confirmed_unix_ms: None,
            restart: false,
        }
    }

    /// The same leave for a container that restarts afterwards.
    #[must_use]
    pub fn restarting(mut self) -> Self {
        self.restart = true;
        self
    }

    /// The first twelve characters of the id: what the nodes' records use.
    #[must_use]
    pub fn short_id(&self) -> &str {
        self.id.get(..12).unwrap_or(&self.id)
    }

    #[must_use]
    pub fn stage(&self, now_unix_ms: i64) -> LeaveStage {
        let Some(confirmed) = self.confirmed_unix_ms else {
            return LeaveStage::Marked;
        };
        let until = confirmed.saturating_add(i64::try_from(self.drain_ms).unwrap_or(i64::MAX));
        if now_unix_ms < until {
            LeaveStage::Draining {
                until_unix_ms: until,
            }
        } else {
            LeaveStage::Stopping
        }
    }

    /// The file a node keeps the leave in, named by the container's id.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.json", self.id)
    }

    /// Refuses a record no node could have written: an id that is not the
    /// engine's hexadecimal one could name a path.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.len() < 12
            || self.id.len() > 64
            || !self.id.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(format!("'{}' is not a container id", self.id));
        }
        Ok(())
    }
}
