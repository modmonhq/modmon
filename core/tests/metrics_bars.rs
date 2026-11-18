//! The bars the metric store is held to: the memory four hours of a
//! thousand containers take, and what adding a step and asking a question
//! cost. Run with `--nocapture` to see the measured numbers.
//!
//! One test, alone in its file: what the process grows by is measured, and
//! a second store in the same process would be counted with it.

use std::time::{Duration, Instant};

use mm::domain::metrics::{
    Basis, Condition, ContainerSample, Measured, Metric, MetricQuery, MetricStore, Percent,
    RETAINED_STEPS, Step, Subject, Tally, Window,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const CONTAINERS: usize = 1_000;
/// Services that are followed besides: every tenth container a service of
/// its own, both metrics, which is far more than autoscale rules need.
const SERVICES: usize = 100;
const MEBIBYTE: usize = 1024 * 1024;
const BUDGET: usize = 32 * MEBIBYTE;
/// Four hours and one step more, so that the oldest step has been written
/// over.
const FILLED_STEPS: u64 = RETAINED_STEPS as u64 + 1;

/// A container id as the engine gives it: 64 characters, five times what
/// a node lists a container under.
fn id(container: usize) -> String {
    format!("{container:064x}")
}

fn service(index: usize) -> String {
    format!("0123456789ab-production/service-{index}")
}

/// One step of every container: values that differ by container and step.
fn step_of(ids: &[String], step: u64) -> Vec<ContainerSample> {
    let mut samples = ids
        .iter()
        .map(|id| ContainerSample {
            container: id.clone(),
            cpu: None,
            memory: None,
        })
        .collect::<Vec<_>>();
    set_step(&mut samples, step);
    samples
}

fn set_step(samples: &mut [ContainerSample], step: u64) {
    for (container, sample) in samples.iter_mut().enumerate() {
        let value = (container as u64 * 37 + step * 11) % 20_000;
        sample.cpu = Some(Measured {
            share: Percent::from_hundredths(value),
            of: Basis::OneCore,
        });
        sample.memory = Some(Measured {
            share: Percent::from_hundredths(value / 2),
            of: Basis::Limit,
        });
    }
}

/// A store that holds `steps` steps of a thousand containers and a
/// hundred services.
fn filled(ids: &[String], steps: u64) -> MetricStore {
    let mut store = MetricStore::new();
    let names = (0..SERVICES).map(service).collect::<Vec<_>>();
    let mut samples = step_of(ids, 0);
    for step in 0..steps {
        set_step(&mut samples, step);
        store.record(Step(step), &samples);
        for (index, name) in names.iter().enumerate() {
            let tally = Tally {
                sum: ((index as u64 * 53 + step * 7) % 30_000) as u32,
                count: 3,
            };
            for metric in Metric::ALL {
                store.record_service(name, metric, Step(step), tally);
            }
        }
    }
    store
}

/// The memory this process holds, in bytes, where the system says.
fn resident() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kilobytes = line.split_whitespace().nth(1)?.parse::<usize>().ok()?;
    Some(kilobytes * 1024)
}

#[test]
fn four_hours_of_a_thousand_containers_fit_the_memory_and_the_time_they_may_take() -> TestResult {
    let ids = (0..CONTAINERS).map(id).collect::<Vec<_>>();
    let store = stays_under_32_mebibytes(&ids);
    takes_well_under_a_microsecond(&ids, store)
}

fn stays_under_32_mebibytes(ids: &[String]) -> MetricStore {
    // The arithmetic: what a series takes is fixed when it is created.
    let containers = CONTAINERS * (MetricStore::CONTAINER_BYTES + 64);
    let services = SERVICES * Metric::ALL.len() * MetricStore::SERVICE_METRIC_BYTES;
    println!(
        "a container (two metrics): {} bytes; one metric of a service: {} bytes",
        MetricStore::CONTAINER_BYTES,
        MetricStore::SERVICE_METRIC_BYTES
    );
    println!(
        "by arithmetic: {CONTAINERS} containers {:.2} MiB + {SERVICES} services {:.2} MiB",
        containers as f64 / MEBIBYTE as f64,
        services as f64 / MEBIBYTE as f64
    );
    assert!(containers + services < BUDGET);
    // A sample is two bytes, and everything kept next to the samples adds
    // less than a third.
    assert!(MetricStore::CONTAINER_BYTES < 2 * 2 * RETAINED_STEPS as usize * 4 / 3);

    // The store itself, full.
    let before = resident();
    let store = filled(ids, FILLED_STEPS);
    assert_eq!(store.containers(), CONTAINERS);
    assert_eq!(store.services(), SERVICES);
    let held = store.memory_bytes();
    println!(
        "the full store accounts for {:.2} MiB",
        held as f64 / MEBIBYTE as f64
    );
    assert!(held < BUDGET, "{held} bytes");
    assert!(held >= containers + services);
    // And what the process grew by, where the system says: this counts
    // what the allocator adds, and the samples this test made on the way.
    if let (Some(before), Some(after)) = (before, resident()) {
        let grown = after.saturating_sub(before);
        println!(
            "the process grew by {:.2} MiB",
            grown as f64 / MEBIBYTE as f64
        );
        assert!(grown < BUDGET, "{grown} bytes");
    }
    // The store is still what it was filled with.
    let reading = store.read(
        &MetricQuery {
            subject: Subject::Container(id(7)),
            metric: Metric::Cpu,
            window: Window::LONGEST,
            samples: false,
        },
        Step(u64::from(RETAINED_STEPS)),
    );
    assert_eq!(
        reading.map(|reading| reading.sampled_seconds),
        Some(4 * 60 * 60)
    );
    store
}

/// The nanoseconds one of `operations` takes: the best of five rounds, so
/// that a busy machine does not decide the number.
fn nanoseconds(operations: usize, mut round: impl FnMut()) -> f64 {
    let mut best = Duration::MAX;
    for _ in 0..5 {
        let started = Instant::now();
        round();
        best = best.min(started.elapsed());
    }
    best.as_nanos() as f64 / operations as f64
}

fn takes_well_under_a_microsecond(ids: &[String], mut store: MetricStore) -> TestResult {
    // An unoptimised build runs the same code some ten times slower; the
    // bar is the optimised build's.
    let limit = if cfg!(debug_assertions) {
        10_000.0
    } else {
        1_000.0
    };
    let mut step = FILLED_STEPS;

    // Adding: one step of a thousand containers is two thousand samples.
    let prepared = (0..20)
        .map(|round| step_of(ids, step + round))
        .collect::<Vec<_>>();
    let mut rounds = prepared.iter();
    let append = nanoseconds(2 * CONTAINERS, || {
        if let Some(samples) = rounds.next() {
            store.record(Step(step), samples);
            step += 1;
        }
    });
    let now = Step(step - 1);
    let names = (0..SERVICES).map(service).collect::<Vec<_>>();
    let service_append = nanoseconds(SERVICES, || {
        for name in &names {
            let tally = Tally { sum: 500, count: 2 };
            store.record_service(name, Metric::Cpu, Step(step), tally);
        }
        step += 1;
    });

    // Asking: the average over a short, a long, and the longest window,
    // of a different series each time.
    let mut asked = 0_u64;
    let mut ask = |seconds: u64, name: fn(usize) -> Subject| -> Result<f64, String> {
        let window = Window::of(Duration::from_secs(seconds)).ok_or("no such window")?;
        let queries = (0..CONTAINERS)
            .map(|index| MetricQuery {
                subject: name(index),
                metric: Metric::ALL[index % 2],
                window,
                samples: false,
            })
            .collect::<Vec<_>>();
        Ok(nanoseconds(queries.len(), || {
            for query in &queries {
                asked += u64::from(store.read(query, now).is_some());
            }
        }))
    };
    let container = |index: usize| Subject::Container(id(index));
    let service_of = |index: usize| Subject::Service(service(index % SERVICES));
    let short = ask(30, container)?;
    let long = ask(5 * 60, container)?;
    let longest = ask(4 * 60 * 60, container)?;
    let service_short = ask(30, service_of)?;
    let service_longest = ask(4 * 60 * 60, service_of)?;
    assert_eq!(
        asked,
        5 * 5 * CONTAINERS as u64,
        "every question had an answer"
    );

    // Asking how long a condition has held, again and again.
    let conditions = [
        Condition::Greater(Percent::from_hundredths(7_500)),
        Condition::Less(Percent::from_hundredths(4_000)),
    ];
    let subjects = (0..CONTAINERS).map(container).collect::<Vec<_>>();
    let mut total = Duration::ZERO;
    let held = nanoseconds(2 * CONTAINERS, || {
        for subject in &subjects {
            for condition in conditions {
                total += store.held(subject, Metric::Cpu, condition, now);
            }
        }
    });

    assert!(
        total > Duration::ZERO,
        "some condition holds for some container"
    );
    println!("adding a sample to a container's series: {append:.0} ns");
    println!("adding a step to a service's series: {service_append:.0} ns");
    println!(
        "average of a container over 30 s: {short:.0} ns, 5 min: {long:.0} ns, 4 h: {longest:.0} ns"
    );
    println!("average of a service over 30 s: {service_short:.0} ns, 4 h: {service_longest:.0} ns");
    println!("how long a condition has held: {held:.0} ns");
    for (what, took) in [
        ("adding a sample", append),
        ("adding a service's step", service_append),
        ("a 30 s average", short),
        ("a 5 min average", long),
        ("a 4 h average", longest),
        ("a service's 30 s average", service_short),
        ("a service's 4 h average", service_longest),
        ("how long a condition has held", held),
    ] {
        assert!(took < limit, "{what} took {took:.0} ns, over {limit} ns");
    }
    // A window's cost does not grow with its length: the longest window
    // is 480 times the short one.
    assert!(
        longest < short * 10.0 + 500.0,
        "{longest:.0} ns against {short:.0} ns"
    );
    Ok(())
}
