//! Every series a node keeps, and the one way to read them.

use std::collections::HashMap;
use std::time::Duration;

use super::ring::{Ring, Samples, Slots, Tallies};
use super::{
    Basis, Condition, ContainerSample, KEPT_UNLISTED_STEPS, Metric, MetricQuery, MetricReading,
    STEP_SECONDS, Step, Subject, Tally,
};

const METRICS: usize = Metric::ALL.len();

/// A container's series, one per metric.
struct ContainerSeries {
    rings: [Ring<Samples>; METRICS],
    /// What the newest sample of each metric is a share of.
    basis: [Option<Basis>; METRICS],
    /// The newest step the container was recorded for.
    listed: u64,
}

/// A service's series. A metric nobody records takes no memory.
#[derive(Default)]
struct ServiceSeries {
    rings: [Option<Box<Ring<Tallies>>>; METRICS],
}

/// The series of a node's containers and of the services it follows.
///
/// It has no clock and no lock: whoever feeds it says which step a sample
/// belongs to, and whoever shares it between threads wraps it. Adding a
/// step to a series that exists allocates nothing, and what a series takes
/// is fixed when it is created: [`MetricStore::CONTAINER_BYTES`] for a
/// container and [`MetricStore::SERVICE_METRIC_BYTES`] for each metric of
/// a service.
#[derive(Default)]
pub struct MetricStore {
    containers: HashMap<Box<str>, Box<ContainerSeries>>,
    services: HashMap<Box<str>, ServiceSeries>,
    /// The newest step containers were recorded for.
    recorded: Option<u64>,
}

impl std::fmt::Debug for MetricStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetricStore")
            .field("containers", &self.containers.len())
            .field("services", &self.services.len())
            .finish_non_exhaustive()
    }
}

impl MetricStore {
    /// The bytes the series of one container take, both metrics together,
    /// without the container's name.
    pub const CONTAINER_BYTES: usize = size_of::<ContainerSeries>() + METRICS * Samples::BYTES;
    /// The bytes one metric of one service takes.
    pub const SERVICE_METRIC_BYTES: usize = Ring::<Tallies>::BYTES;

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the step `step` to the series of every container in `samples`,
    /// creating the series of a container that is new. A container that
    /// was in none of the last [`KEPT_UNLISTED_STEPS`] steps is gone: its
    /// series are freed.
    pub fn record(&mut self, step: Step, samples: &[ContainerSample]) {
        for sample in samples {
            if let Some(series) = self.containers.get_mut(sample.container.as_str()) {
                series.add(step.0, sample);
            } else {
                let mut series = Box::new(ContainerSeries {
                    rings: [Ring::new(), Ring::new()],
                    basis: [None; METRICS],
                    listed: step.0,
                });
                series.add(step.0, sample);
                self.containers
                    .insert(sample.container.as_str().into(), series);
            }
        }
        self.containers
            .retain(|_, series| step.0.saturating_sub(series.listed) <= KEPT_UNLISTED_STEPS);
        self.recorded = self.recorded.max(Some(step.0));
    }

    /// The sum and the count of what `containers` used in the newest step
    /// recorded: what this node adds to a service whose containers on this
    /// node they are. A container without a sample in that step is left
    /// out, never counted as zero.
    #[must_use]
    pub fn tally<I: AsRef<str>>(&self, metric: Metric, containers: &[I]) -> Tally {
        let Some(step) = self.recorded else {
            return Tally::NONE;
        };
        containers
            .iter()
            .filter_map(|container| self.containers.get(container.as_ref()))
            .fold(Tally::NONE, |tally, series| {
                tally.plus(series.rings[metric.index()].at(step))
            })
    }

    /// Adds the step `step` to a service's series: the sums of every node
    /// for that step, added up. The series is created with its first
    /// step. Returns `false`, and changes nothing, for a step that is not
    /// newer than the series' newest.
    pub fn record_service(
        &mut self,
        service: &str,
        metric: Metric,
        step: Step,
        tally: Tally,
    ) -> bool {
        if let Some(series) = self.services.get_mut(service) {
            return series.add(metric, step.0, tally);
        }
        let mut series = ServiceSeries::default();
        let added = series.add(metric, step.0, tally);
        self.services.insert(service.into(), series);
        added
    }

    /// Frees the series of a service that is no longer deployed. Returns
    /// whether there were any.
    pub fn forget_service(&mut self, service: &str) -> bool {
        self.services.remove(service).is_some()
    }

    /// The newest value, the average over the window, and, when asked
    /// for, every step of the window, as of the step `now`. `None` for a
    /// container or a service the store holds no series of.
    #[must_use]
    pub fn read(&self, query: &MetricQuery, now: Step) -> Option<MetricReading> {
        let metric = query.metric.index();
        match &query.subject {
            Subject::Container(container) => {
                let series = self.containers.get(container.as_str())?;
                let mut reading = reading(&series.rings[metric], query, now.0);
                reading.basis = series.basis[metric];
                reading.containers = None;
                Some(reading)
            }
            Subject::Service(service) => {
                let ring = self.services.get(service.as_str())?.rings[metric].as_deref()?;
                Some(reading(ring, query, now.0))
            }
        }
    }

    /// For how long, up to the newest step, `condition` has held in every
    /// step without a break. Zero for a series the store does not hold,
    /// for one whose newest step does not satisfy it, and for one nothing
    /// was added to for two steps. An autoscale rule acts when this is at
    /// least its `for:`.
    ///
    /// The store follows the conditions it is asked about (four at a time
    /// for each series), so asking again costs no walk over the series.
    pub fn held(
        &mut self,
        subject: &Subject,
        metric: Metric,
        condition: Condition,
        now: Step,
    ) -> Duration {
        let metric = metric.index();
        let steps = match subject {
            Subject::Container(container) => self
                .containers
                .get_mut(container.as_str())
                .map(|series| series.rings[metric].held(condition, now.0)),
            Subject::Service(service) => self
                .services
                .get_mut(service.as_str())
                .and_then(|series| series.rings[metric].as_deref_mut())
                .map(|ring| ring.held(condition, now.0)),
        };
        Duration::from_secs(steps.unwrap_or_default() * STEP_SECONDS)
    }

    /// How many containers the store holds series of.
    #[must_use]
    pub fn containers(&self) -> usize {
        self.containers.len()
    }

    /// How many services the store holds series of.
    #[must_use]
    pub fn services(&self) -> usize {
        self.services.len()
    }

    /// The bytes the store holds on to: every series, every name, and the
    /// two tables that find a series by name.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        // A table keeps one entry and one byte of bookkeeping per place.
        let table = |places: usize, entry: usize| places * (entry + 1);
        let service_rings = self
            .services
            .values()
            .flat_map(|series| &series.rings)
            .flatten()
            .count();
        size_of::<Self>()
            + table(
                self.containers.capacity(),
                size_of::<(Box<str>, Box<ContainerSeries>)>(),
            )
            + self.containers.keys().map(|name| name.len()).sum::<usize>()
            + self.containers.len() * Self::CONTAINER_BYTES
            + table(
                self.services.capacity(),
                size_of::<(Box<str>, ServiceSeries)>(),
            )
            + self.services.keys().map(|name| name.len()).sum::<usize>()
            + service_rings * Self::SERVICE_METRIC_BYTES
    }
}

impl ServiceSeries {
    fn add(&mut self, metric: Metric, step: u64, tally: Tally) -> bool {
        self.rings[metric.index()]
            .get_or_insert_with(|| Box::new(Ring::new()))
            .append(step, tally)
    }
}

impl ContainerSeries {
    fn add(&mut self, step: u64, sample: &ContainerSample) {
        for metric in Metric::ALL {
            let measured = sample.of(metric);
            self.rings[metric.index()].append(
                step,
                measured.map_or(Tally::NONE, |measured| Tally::one(measured.share)),
            );
            if let Some(measured) = measured {
                self.basis[metric.index()] = Some(measured.of);
            }
        }
        self.listed = self.listed.max(step);
    }
}

fn reading<S: Slots>(ring: &Ring<S>, query: &MetricQuery, now: u64) -> MetricReading {
    let end = ring.end(now);
    let steps = query.window.steps();
    let totals = ring.window(end, steps);
    let newest = ring.at(end);
    let samples = if query.samples {
        (0..u64::from(steps))
            .rev()
            .map(|back| {
                end.checked_sub(back)
                    .and_then(|step| ring.at(step).average())
            })
            .collect()
    } else {
        Vec::new()
    };
    MetricReading {
        metric: query.metric,
        basis: None,
        latest: newest.average(),
        containers: Some(newest.count),
        average: Ring::<S>::average(totals),
        window_seconds: query.window.duration().as_secs(),
        sampled_seconds: totals.steps * STEP_SECONDS,
        step_seconds: STEP_SECONDS,
        age_seconds: now.saturating_sub(end) * STEP_SECONDS,
        samples,
    }
}
