//! The metrics `mmd` keeps about the containers of its own node: the store
//! every reader shares, and the sampler that fills it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::app::ports::{EngineUsage, MeteredContainers, Metrics, UsageCounters};
use crate::domain::metrics::{
    Condition, ContainerSample, Measured, Metric, MetricQuery, MetricReading, MetricStore,
    STEP_SECONDS, Step, Subject, Tally, UsageReading,
};

/// A node's metric store, shared between the sampler that writes it and
/// whoever reads it. Steps are counted from the moment it was created, on
/// a clock that never jumps, so setting the host's time does not tear a
/// hole into a series.
pub struct NodeMetrics {
    store: Mutex<MetricStore>,
    started: Instant,
}

impl std::fmt::Debug for NodeMetrics {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeMetrics")
            .finish_non_exhaustive()
    }
}

impl NodeMetrics {
    /// An empty store. It lives in memory only: a restarted `mmd` knows
    /// nothing about the time before.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(MetricStore::new()),
            started: Instant::now(),
        })
    }

    /// The step that is being sampled now.
    #[must_use]
    pub fn now(&self) -> Step {
        Step::after(self.started.elapsed())
    }

    /// How long until the next step begins.
    fn until_next_step(&self) -> Duration {
        let step = Duration::from_secs(STEP_SECONDS);
        let into = self.started.elapsed().as_nanos() % step.as_nanos();
        step.saturating_sub(Duration::from_nanos(
            u64::try_from(into).unwrap_or_default(),
        ))
    }

    /// Adds `step` to the series of the containers in `samples`; see
    /// [`MetricStore::record`].
    pub fn record(&self, step: Step, samples: &[ContainerSample]) {
        self.store().record(step, samples);
    }

    /// What [`Metrics::read`] answers, as of `step` instead of now.
    #[must_use]
    pub fn read_as_of(&self, query: &MetricQuery, step: Step) -> Option<MetricReading> {
        self.store().read(query, step)
    }

    /// How many containers the store holds series of.
    #[must_use]
    pub fn containers(&self) -> usize {
        self.store().containers()
    }

    fn store(&self) -> MutexGuard<'_, MetricStore> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Metrics for NodeMetrics {
    fn read(&self, query: &MetricQuery) -> Option<MetricReading> {
        self.read_as_of(query, self.now())
    }

    fn held(&self, subject: &Subject, metric: Metric, condition: Condition) -> Duration {
        let now = self.now();
        self.store().held(subject, metric, condition, now)
    }

    fn tally(&self, metric: Metric, containers: &[String]) -> Tally {
        self.store().tally(metric, containers)
    }

    fn record_service(&self, service: &str, metric: Metric, tally: Tally) -> bool {
        let now = self.now();
        self.store().record_service(service, metric, now, tally)
    }

    fn forget_service(&self, service: &str) -> bool {
        self.store().forget_service(service)
    }
}

/// What one round of sampling found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sampled {
    /// The containers the node lists as running.
    pub containers: usize,
    /// How many of them the host's counters were read for.
    pub from_counters: usize,
    /// How many of them the engine had to be asked about.
    pub from_engine: usize,
}

/// What the sampler remembers between two rounds.
#[derive(Default)]
struct Earlier {
    /// Each container's counters of the round before: CPU is what was
    /// used between two readings.
    readings: HashMap<String, UsageReading>,
    /// Whether the journal already says that the engine is being asked.
    engine_noted: bool,
}

/// Samples the CPU and memory of the node's running containers once every
/// step and records them.
///
/// The containers are the ones `mmd` already lists, so a round asks the
/// engine nothing. Their counters come from the host's files, and only for
/// a container whose counters the host does not show is the engine asked.
/// Both block, so the sampler runs on a thread of its own
/// ([`MetricsSampler::start`]).
pub struct MetricsSampler {
    containers: Arc<dyn MeteredContainers>,
    counters: Arc<dyn UsageCounters>,
    engine: Option<Arc<dyn EngineUsage>>,
    metrics: Arc<NodeMetrics>,
    earlier: Mutex<Earlier>,
}

impl std::fmt::Debug for MetricsSampler {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetricsSampler")
            .finish_non_exhaustive()
    }
}

impl MetricsSampler {
    /// A sampler that fills a new, empty store. It does nothing until
    /// [`MetricsSampler::start`] or [`MetricsSampler::sample`] is called.
    /// Without `engine`, a container whose counters the host does not
    /// show has no samples.
    #[must_use]
    pub fn new(
        containers: Arc<dyn MeteredContainers>,
        counters: Arc<dyn UsageCounters>,
        engine: Option<Arc<dyn EngineUsage>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            containers,
            counters,
            engine,
            metrics: NodeMetrics::new(),
            earlier: Mutex::new(Earlier::default()),
        })
    }

    /// The store this sampler fills.
    #[must_use]
    pub fn metrics(&self) -> Arc<NodeMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Samples on a thread of its own until the process ends. The thread
    /// sleeps between steps, and on a node that runs no container it
    /// reads no file.
    ///
    /// A thread, not a task: reading the counters is file I/O, the
    /// engine fallback waits for a command, and neither may hold up a
    /// thread that serves requests. A thread that only ever sleeps and
    /// samples also keeps its rhythm whatever else the daemon is busy
    /// with.
    pub fn start(self: &Arc<Self>) {
        let sampler = Arc::clone(self);
        let thread = std::thread::Builder::new()
            .name("mm-metrics".to_owned())
            .spawn(move || sampler.run());
        if let Err(error) = thread {
            tracing::warn!(
                %error,
                "cannot start the thread that samples container metrics, so this node keeps none"
            );
        }
    }

    /// Samples every step, for ever.
    fn run(&self) {
        let mut sampled = None;
        loop {
            let step = self.metrics.now();
            if sampled < Some(step) {
                let began = Instant::now();
                let round = self.sample(step);
                tracing::debug!(
                    step = step.0,
                    containers = round.containers,
                    from_counters = round.from_counters,
                    from_engine = round.from_engine,
                    series = self.metrics.containers(),
                    took_us = u64::try_from(began.elapsed().as_micros()).unwrap_or(u64::MAX),
                    "sampled container metrics"
                );
                sampled = Some(step);
            }
            std::thread::sleep(self.metrics.until_next_step());
        }
    }

    /// One round: reads every running container once and records what it
    /// used as the step `step`. Blocks while it reads.
    pub fn sample(&self, step: Step) -> Sampled {
        let containers = self.containers.running();
        let readings = if containers.is_empty() {
            Vec::new()
        } else {
            self.counters.read(&containers)
        };
        let node_memory = self.counters.node_memory();
        let mut samples = Vec::with_capacity(containers.len());
        let mut unread = Vec::new();
        let mut earlier = self.earlier.lock().unwrap_or_else(PoisonError::into_inner);
        let mut next = HashMap::with_capacity(containers.len());
        for (container, reading) in containers.iter().zip(readings) {
            let Some(reading) = reading else {
                unread.push(container.clone());
                continue;
            };
            samples.push(ContainerSample {
                container: container.clone(),
                cpu: earlier
                    .readings
                    .get(container)
                    .and_then(|earlier| reading.cpu_since(earlier)),
                memory: reading.memory(node_memory),
            });
            next.insert(container.clone(), reading);
        }
        earlier.readings = next;
        let from_counters = samples.len();
        // What the files gave is in the store before the engine is asked.
        self.record(step, &samples);
        let from_engine = self.ask_engine(step, &unread, &mut earlier);
        Sampled {
            containers: containers.len(),
            from_counters,
            from_engine,
        }
    }

    /// Records what the engine says about the containers the host shows
    /// no counters for. A container nobody has numbers for is recorded
    /// without a sample: it runs, so its series stay.
    fn ask_engine(&self, step: Step, unread: &[String], earlier: &mut Earlier) -> usize {
        if unread.is_empty() {
            return 0;
        }
        let answered = match &self.engine {
            Some(engine) => engine.usage(unread).unwrap_or_else(|error| {
                tracing::debug!(%error, "the engine gave no container usage");
                Vec::new()
            }),
            None => Vec::new(),
        };
        if !earlier.engine_noted {
            earlier.engine_noted = true;
            tracing::warn!(
                containers = unread.len(),
                "this host shows no cgroup v2 counters for some containers, so their CPU and memory are read from the engine, which costs two engine commands every {STEP_SECONDS} seconds"
            );
        }
        let samples = unread
            .iter()
            .map(|container| {
                answered
                    .iter()
                    .find(|sample| &sample.container == container)
                    .cloned()
                    .unwrap_or_else(|| ContainerSample {
                        container: container.clone(),
                        cpu: None,
                        memory: None,
                    })
            })
            .collect::<Vec<_>>();
        self.record(step, &samples);
        samples
            .iter()
            .filter(|sample| sample.cpu.is_some() || sample.memory.is_some())
            .count()
    }

    fn record(&self, step: Step, samples: &[ContainerSample]) {
        for sample in samples {
            tracing::trace!(
                container = %sample.container,
                cpu = %shown(sample.cpu),
                memory = %shown(sample.memory),
                "container sample"
            );
        }
        self.metrics.record(step, samples);
    }
}

/// A sample as a log line shows it: `82.5% of limit`, or `none`.
fn shown(measured: Option<Measured>) -> String {
    measured.map_or_else(
        || "none".to_owned(),
        |measured| format!("{} of {}", measured.share, measured.of.as_str()),
    )
}
