//! How much of its CPU and memory a container uses, kept for four hours.
//!
//! `mmd` on a node samples its own containers every [`STEP_SECONDS`] and
//! keeps the samples in memory, in ring buffers of a fixed size
//! ([`MetricStore`]): one series per container and metric, and one per
//! service and metric that holds, for each step, the sum and the count over
//! the service's containers on every node. Nothing is written to disk, so a
//! restarted `mmd` starts with an empty store.
//!
//! A sample is a share in hundredths of a percent ([`Percent`]): of the
//! container's limit when it has one, and otherwise of one core for CPU and
//! of the node's memory for memory ([`Basis`]). A step without a sample is
//! kept as "no sample" and never reads as zero.
//!
//! Three questions are answered without walking a series: the newest value,
//! the average over the last while ([`MetricStore::read`]), and how long a
//! condition has held without a break ([`MetricStore::held`]). The same
//! read returns the samples themselves for a chart.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[path = "metrics/ring.rs"]
mod ring;
#[path = "metrics/store.rs"]
mod store;
#[path = "metrics/usage.rs"]
mod usage;

pub use store::MetricStore;
pub use usage::{CpuLimit, UsageReading};

/// Seconds between two samples of every series.
pub const STEP_SECONDS: u64 = 5;
/// How many steps a series keeps: four hours.
pub const RETAINED_STEPS: u32 = 2_880;
/// The longest time a question can look back, and so the longest `for:` of
/// an autoscale rule.
pub const RETAINED: Duration = Duration::from_secs(RETAINED_STEPS as u64 * STEP_SECONDS);
/// How many steps a container may go unlisted before its series are freed:
/// a container that restarts keeps its history, one that was removed gives
/// its memory back within a minute.
pub const KEPT_UNLISTED_STEPS: u64 = 12;

/// What is measured.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Metric {
    Cpu,
    Memory,
}

impl Metric {
    pub const ALL: [Self; 2] = [Self::Cpu, Self::Memory];

    const fn index(self) -> usize {
        match self {
            Self::Cpu => 0,
            Self::Memory => 1,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "memory",
        }
    }
}

/// What a percentage is a share of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Basis {
    /// The container's own CPU or memory limit.
    Limit,
    /// One core, for the CPU of a container without a CPU limit: a
    /// container that keeps two cores busy reads 200%.
    OneCore,
    /// The node's memory, for a container without a memory limit.
    NodeMemory,
}

impl Basis {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Limit => "limit",
            Self::OneCore => "one-core",
            Self::NodeMemory => "node-memory",
        }
    }
}

/// A share in hundredths of a percent, from 0% to 655.34%. More than that
/// reads as 655.34%. In JSON it is a number of percent, such as `75.12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "f64", try_from = "f64")]
pub struct Percent(u16);

impl Percent {
    pub const ZERO: Self = Self(0);
    /// The largest share a sample can hold.
    pub const MAX: Self = Self(u16::MAX - 1);

    /// `hundredths` of a percent; more than [`Percent::MAX`] becomes it.
    #[must_use]
    pub fn from_hundredths(hundredths: u64) -> Self {
        Self(u16::try_from(hundredths).map_or(Self::MAX.0, |value| value.min(Self::MAX.0)))
    }

    /// `part` as a share of `whole`, rounded down. `None` when `whole` is
    /// zero.
    #[must_use]
    pub fn of(part: u64, whole: u64) -> Option<Self> {
        let hundredths = (u128::from(part) * 10_000).checked_div(u128::from(whole))?;
        Some(Self::from_hundredths(
            u64::try_from(hundredths).unwrap_or(u64::MAX),
        ))
    }

    #[must_use]
    pub const fn hundredths(self) -> u16 {
        self.0
    }

    /// A whole number of percent, at most 655.
    #[must_use]
    pub const fn from_whole(percent: u16) -> Self {
        let percent = if percent > 655 { 655 } else { percent };
        Self(percent * 100)
    }
}

impl From<Percent> for f64 {
    fn from(percent: Percent) -> Self {
        Self::from(percent.0) / 100.0
    }
}

impl TryFrom<f64> for Percent {
    type Error = String;

    fn try_from(percent: f64) -> Result<Self, Self::Error> {
        if !percent.is_finite() || percent < 0.0 {
            return Err(format!("a percentage is a number from 0 up, not {percent}"));
        }
        let hundredths = (percent * 100.0).round().min(f64::from(Self::MAX.0));
        // In range by the two checks above.
        Ok(Self::from_hundredths(hundredths as u64))
    }
}

impl std::fmt::Display for Percent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (whole, hundredths) = (self.0 / 100, self.0 % 100);
        if hundredths == 0 {
            write!(formatter, "{whole}%")
        } else {
            write!(formatter, "{whole}.{hundredths:02}%")
        }
    }
}

impl JsonSchema for Percent {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Percent".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "number",
            "description": "A share in percent, with two decimals, such as 75.12.",
            "minimum": 0,
            "maximum": 655.34
        })
    }
}

/// A point in time, counted in steps of [`STEP_SECONDS`] from wherever the
/// one who feeds the store started counting. Steps only ever grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step(pub u64);

impl Step {
    /// The step that holds the moment `elapsed` after counting started.
    #[must_use]
    pub const fn after(elapsed: Duration) -> Self {
        Self(elapsed.as_secs() / STEP_SECONDS)
    }
}

/// How far back a question looks: a whole number of steps, from one step
/// to four hours. In JSON it is a number of seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(into = "u64", try_from = "u64")]
pub struct Window(u32);

impl Window {
    /// The newest step alone.
    pub const NEWEST: Self = Self(1);
    /// Everything a series keeps.
    pub const LONGEST: Self = Self(RETAINED_STEPS);

    /// The window that covers `duration`, rounded up to whole steps.
    /// `None` for no time at all and for more than [`RETAINED`].
    #[must_use]
    pub fn of(duration: Duration) -> Option<Self> {
        if duration.is_zero() || duration > RETAINED {
            return None;
        }
        let step = u128::from(STEP_SECONDS) * 1_000;
        let steps = duration.as_millis().div_ceil(step);
        u32::try_from(steps).ok().map(Self)
    }

    #[must_use]
    pub const fn steps(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn duration(self) -> Duration {
        Duration::from_secs(self.0 as u64 * STEP_SECONDS)
    }
}

impl From<Window> for u64 {
    fn from(window: Window) -> Self {
        window.duration().as_secs()
    }
}

impl TryFrom<u64> for Window {
    type Error = String;

    fn try_from(seconds: u64) -> Result<Self, Self::Error> {
        Self::of(Duration::from_secs(seconds)).ok_or_else(|| {
            format!(
                "a window is from 1 to {} seconds (four hours), not {seconds}",
                RETAINED.as_secs()
            )
        })
    }
}

impl JsonSchema for Window {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "MetricWindow".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "integer",
            "description": "How far back to look, in seconds: at most four hours.",
            "minimum": 1,
            "maximum": RETAINED.as_secs()
        })
    }
}

/// What one step of a service holds: the sum of its containers' samples
/// and how many containers gave one. A node tells the other nodes this
/// about its own containers, and the sums of every node, added up, are the
/// step of the service's series.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct Tally {
    /// In hundredths of a percent.
    pub sum: u32,
    pub count: u16,
}

impl Tally {
    /// No sample.
    pub const NONE: Self = Self { sum: 0, count: 0 };

    /// The sample of one container.
    #[must_use]
    pub const fn one(sample: Percent) -> Self {
        Self {
            sum: sample.0 as u32,
            count: 1,
        }
    }

    /// Both together. The sum of 65,535 samples fits, and nothing wraps
    /// beyond that.
    #[must_use]
    pub const fn plus(self, other: Self) -> Self {
        Self {
            sum: self.sum.saturating_add(other.sum),
            count: self.count.saturating_add(other.count),
        }
    }

    /// The average sample, rounded to the nearest hundredth; `None`
    /// without a sample.
    #[must_use]
    pub fn average(self) -> Option<Percent> {
        average(u64::from(self.sum), u64::from(self.count))
    }
}

fn average(sum: u64, count: u64) -> Option<Percent> {
    (sum + count / 2)
        .checked_div(count)
        .map(Percent::from_hundredths)
}

/// What an autoscale rule or an alert asks of a sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Condition {
    /// The sample is above this.
    Greater(Percent),
    /// The sample is below this.
    Less(Percent),
}

impl Condition {
    /// Whether a step satisfies it. A step without a sample never does.
    #[must_use]
    pub fn holds(self, sample: Option<Percent>) -> bool {
        match (self, sample) {
            (Self::Greater(threshold), Some(sample)) => sample > threshold,
            (Self::Less(threshold), Some(sample)) => sample < threshold,
            (_, None) => false,
        }
    }
}

/// Whose series a question is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Subject {
    /// A container of this node, by the id the node lists it under.
    Container(String),
    /// A service, by the name its sums were recorded under.
    Service(String),
}

/// A measured share and what it is a share of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measured {
    pub share: Percent,
    pub of: Basis,
}

/// What one container used during one step. A missing value is a step
/// without a sample for that metric.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerSample {
    pub container: String,
    pub cpu: Option<Measured>,
    pub memory: Option<Measured>,
}

impl ContainerSample {
    #[must_use]
    pub const fn of(&self, metric: Metric) -> Option<Measured> {
        match metric {
            Metric::Cpu => self.cpu,
            Metric::Memory => self.memory,
        }
    }
}

/// One question to the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct MetricQuery {
    pub subject: Subject,
    pub metric: Metric,
    /// How far back the average, and the samples, reach.
    pub window: Window,
    /// Also return every step of the window, for a chart.
    #[serde(default)]
    pub samples: bool,
}

/// The answer: the newest value, the average over the window, and, when
/// asked for, every step of the window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct MetricReading {
    pub metric: Metric,
    /// What a container's percentages are a share of, as of its newest
    /// sample. Absent for a service, whose containers may differ, and for
    /// a container that has no sample yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis: Option<Basis>,
    /// The newest step. Absent when that step has no sample, which is also
    /// the case for a series nothing was added to for two steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest: Option<Percent>,
    /// For a service: how many containers the newest step counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containers: Option<u16>,
    /// The average over the window's samples. Absent when it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average: Option<Percent>,
    /// The length of the window.
    pub window_seconds: u64,
    /// How much of the window holds samples. A reader that must not act on
    /// a window with holes compares this with `window-seconds`.
    pub sampled_seconds: u64,
    /// The time between two samples.
    pub step_seconds: u64,
    /// How long before the answer the window ended: zero, or one step
    /// while the current step is still being sampled.
    pub age_seconds: u64,
    /// Every step of the window, oldest first, `null` where a step has no
    /// sample. The last one ended `age-seconds` before the answer and each
    /// one before it `step-seconds` earlier.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<Option<Percent>>,
}
