use std::time::Duration;

use crate::domain::error::MmError;
use crate::domain::metrics::{
    Condition, ContainerSample, Metric, MetricQuery, MetricReading, Subject, Tally, UsageReading,
};

/// The CPU and memory of a node's containers over the last four hours, as
/// the node's `mmd` keeps them in memory. Every call reads or writes
/// memory only and returns at once.
pub trait Metrics: Send + Sync {
    /// The one read: the newest value, the average over the query's
    /// window, and, when asked for, every step of the window. `None` for
    /// a container or a service the node holds no series of.
    fn read(&self, query: &MetricQuery) -> Option<MetricReading>;

    /// For how long, up to now, `condition` has held in every step
    /// without a break; a step without a sample breaks it. An autoscale
    /// rule acts when this is at least its `for:`.
    fn held(&self, subject: &Subject, metric: Metric, condition: Condition) -> Duration;

    /// The sum and the count of what `containers` of this node used in
    /// the newest step: what the node tells the other nodes about a
    /// service whose containers here they are.
    fn tally(&self, metric: Metric, containers: &[String]) -> Tally;

    /// Adds the current step to a service's series: the sums of every
    /// node, added up. The series begins with its first step. `false`
    /// when the current step is already written.
    fn record_service(&self, service: &str, metric: Metric, tally: Tally) -> bool;

    /// Frees the series of a service that is no longer deployed.
    fn forget_service(&self, service: &str) -> bool;
}

/// The containers `mmd` already knows to be running on its node, by the
/// id it lists them under. Reads memory only: listing them must never
/// cost an engine command.
pub trait MeteredContainers: Send + Sync {
    fn running(&self) -> Vec<String>;
}

/// The counters the host keeps for each container. Reading them reads
/// files, so it belongs on a thread that serves nothing else.
pub trait UsageCounters: Send + Sync {
    /// One entry for each of `containers`, in their order: `None` for a
    /// container whose counters the host does not show.
    fn read(&self, containers: &[String]) -> Vec<Option<UsageReading>>;

    /// The node's memory in bytes, when the host says.
    fn node_memory(&self) -> Option<u64>;
}

/// The engine's own numbers, for the containers whose counters the host
/// does not show. It costs engine commands for every step, and it blocks
/// until the engine answered.
pub trait EngineUsage: Send + Sync {
    /// What each of `containers` uses now. A container the engine has no
    /// numbers for is left out.
    fn usage(&self, containers: &[String]) -> Result<Vec<ContainerSample>, MmError>;
}
