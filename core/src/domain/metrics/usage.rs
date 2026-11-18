//! From a container's counters to the shares a step stores.

use std::time::Duration;

use super::{Basis, Measured, Percent, STEP_SECONDS};

/// Two readings further apart than this are not turned into a CPU share:
/// the average over so long a time says little about the step it would be
/// stored in.
const LONGEST_INTERVAL: Duration = Duration::from_secs(3 * STEP_SECONDS);

/// A CPU limit as the kernel holds it: `quota_usec` microseconds of CPU
/// time in every `period_usec`. Two cores are a quota of twice the period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuLimit {
    pub quota_usec: u64,
    pub period_usec: u64,
}

/// A container's counters at one moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageReading {
    /// When the counters were read, on a clock that only moves forward.
    pub at: Duration,
    /// The CPU time the container has used since it started, in
    /// microseconds.
    pub cpu_usec: u64,
    pub cpu_limit: Option<CpuLimit>,
    /// The memory in use, without the file cache the kernel gives back
    /// the moment something else needs it. The engine's own `stats`
    /// counts the same way. `None` on a host that does not account
    /// memory for the container.
    pub memory_bytes: Option<u64>,
    pub memory_limit: Option<u64>,
}

impl UsageReading {
    /// The CPU used between `earlier` and this reading: a share of the
    /// container's limit when it has one, of one core otherwise. `None`
    /// when the counter started over (the container restarted) and when
    /// the two readings do not follow each other closely.
    #[must_use]
    pub fn cpu_since(&self, earlier: &Self) -> Option<Measured> {
        let elapsed = self.at.checked_sub(earlier.at)?;
        if elapsed.is_zero() || elapsed > LONGEST_INTERVAL {
            return None;
        }
        let elapsed = u64::try_from(elapsed.as_micros()).ok()?;
        let used = self.cpu_usec.checked_sub(earlier.cpu_usec)?;
        let (used, available, of) = match self.cpu_limit {
            Some(limit) if limit.quota_usec > 0 && limit.period_usec > 0 => (
                used.saturating_mul(limit.period_usec),
                elapsed.saturating_mul(limit.quota_usec),
                Basis::Limit,
            ),
            _ => (used, elapsed, Basis::OneCore),
        };
        Some(Measured {
            share: Percent::of(used, available)?,
            of,
        })
    }

    /// The memory in use: a share of the container's limit when it has
    /// one, of the node's memory otherwise. `None` for a container
    /// without a limit on a node whose memory is not known, and for one
    /// whose memory the host does not account.
    #[must_use]
    pub fn memory(&self, node_memory: Option<u64>) -> Option<Measured> {
        let (whole, of) = match self.memory_limit {
            Some(limit) if limit > 0 => (limit, Basis::Limit),
            _ => (node_memory?, Basis::NodeMemory),
        };
        Some(Measured {
            share: Percent::of(self.memory_bytes?, whole)?,
            of,
        })
    }
}
