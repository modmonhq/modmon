//! The store of container metrics: what a read answers (the newest value,
//! the average over a window, the samples), how long a condition has held,
//! what a step without a sample means, when a series is freed, what a
//! service's series holds, and how counters become shares.

use std::time::Duration;

use mm::domain::metrics::{
    Basis, Condition, ContainerSample, CpuLimit, KEPT_UNLISTED_STEPS, Measured, Metric,
    MetricQuery, MetricReading, MetricStore, Percent, RETAINED, RETAINED_STEPS, STEP_SECONDS, Step,
    Subject, Tally, UsageReading, Window,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const WEB: &str = "0123456789ab";
const API: &str = "ba9876543210";

fn percent(hundredths: u64) -> Percent {
    Percent::from_hundredths(hundredths)
}

fn window(seconds: u64) -> Result<Window, String> {
    Window::of(Duration::from_secs(seconds)).ok_or_else(|| format!("no window of {seconds} s"))
}

/// A container's step: this much CPU of one core, and half of that as
/// memory of its limit.
fn sample(container: &str, cpu: Option<u64>) -> ContainerSample {
    ContainerSample {
        container: container.to_owned(),
        cpu: cpu.map(|cpu| Measured {
            share: percent(cpu),
            of: Basis::OneCore,
        }),
        memory: cpu.map(|cpu| Measured {
            share: percent(cpu / 2),
            of: Basis::Limit,
        }),
    }
}

fn query(
    subject: Subject,
    metric: Metric,
    seconds: u64,
    samples: bool,
) -> Result<MetricQuery, String> {
    Ok(MetricQuery {
        subject,
        metric,
        window: window(seconds)?,
        samples,
    })
}

fn container(id: &str) -> Subject {
    Subject::Container(id.to_owned())
}

fn service(name: &str) -> Subject {
    Subject::Service(name.to_owned())
}

fn read(
    store: &MetricStore,
    subject: Subject,
    metric: Metric,
    seconds: u64,
    now: u64,
) -> Result<MetricReading, Box<dyn std::error::Error>> {
    Ok(store
        .read(&query(subject, metric, seconds, true)?, Step(now))
        .ok_or("the store holds no such series")?)
}

/// Numbers that look random and are the same on every run.
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

#[test]
fn a_percentage_is_kept_in_hundredths_and_stops_at_its_largest() -> TestResult {
    assert_eq!(Percent::of(3, 4), Some(percent(7_500)));
    assert_eq!(Percent::of(1, 3), Some(percent(3_333)));
    assert_eq!(Percent::of(1, 0), None);
    assert_eq!(Percent::of(u64::MAX, 1), Some(Percent::MAX));
    assert_eq!(Percent::MAX.hundredths(), 65_534);
    assert_eq!(percent(1_000_000), Percent::MAX);
    assert_eq!(percent(7_512).to_string(), "75.12%");
    assert_eq!(percent(7_500).to_string(), "75%");
    assert_eq!(percent(5).to_string(), "0.05%");
    assert_eq!(serde_json::to_string(&percent(7_512))?, "75.12");
    assert_eq!(serde_json::from_str::<Percent>("75.12")?, percent(7_512));
    assert_eq!(serde_json::from_str::<Percent>("40")?, percent(4_000));
    assert_eq!(serde_json::from_str::<Percent>("9000.5")?, Percent::MAX);
    for hundredths in 0..=u64::from(Percent::MAX.hundredths()) {
        let text = serde_json::to_string(&percent(hundredths))?;
        assert_eq!(serde_json::from_str::<Percent>(&text)?, percent(hundredths));
    }
    assert!(serde_json::from_str::<Percent>("-1").is_err());
    Ok(())
}

#[test]
fn a_window_is_whole_steps_and_at_most_four_hours() -> TestResult {
    assert_eq!(STEP_SECONDS, 5);
    assert_eq!(RETAINED, Duration::from_secs(4 * 60 * 60));
    assert_eq!(window(30)?.steps(), 6);
    assert_eq!(window(1)?.steps(), 1);
    assert_eq!(window(12)?.steps(), 3);
    assert_eq!(window(12)?.duration(), Duration::from_secs(15));
    assert_eq!(window(5 * 60)?.steps(), 60);
    assert_eq!(Window::of(RETAINED), Some(Window::LONGEST));
    assert_eq!(Window::LONGEST.steps(), RETAINED_STEPS);
    assert_eq!(Window::of(Duration::from_millis(1)), Some(Window::NEWEST));
    assert_eq!(Window::of(Duration::ZERO), None);
    assert_eq!(Window::of(RETAINED + Duration::from_secs(1)), None);
    assert_eq!(serde_json::to_string(&window(30)?)?, "30");
    assert_eq!(serde_json::from_str::<Window>("300")?, window(300)?);
    let refused = serde_json::from_str::<Window>("14401")
        .err()
        .ok_or("a window of more than four hours was read")?;
    assert!(refused.to_string().contains("four hours"), "{refused}");
    assert_eq!(Step::after(Duration::from_secs(14)), Step(2));
    Ok(())
}

#[test]
fn one_read_gives_the_newest_value_the_average_and_the_samples() -> TestResult {
    let mut store = MetricStore::new();
    for (step, cpu) in [(100, 1_000), (101, 2_000), (102, 6_000)] {
        store.record(Step(step), &[sample(WEB, Some(cpu)), sample(API, Some(50))]);
    }
    let cpu = read(&store, container(WEB), Metric::Cpu, 15, 102)?;
    assert_eq!(
        cpu,
        MetricReading {
            metric: Metric::Cpu,
            basis: Some(Basis::OneCore),
            latest: Some(percent(6_000)),
            containers: None,
            average: Some(percent(3_000)),
            window_seconds: 15,
            sampled_seconds: 15,
            step_seconds: 5,
            age_seconds: 0,
            samples: vec![
                Some(percent(1_000)),
                Some(percent(2_000)),
                Some(percent(6_000))
            ],
        }
    );
    let memory = read(&store, container(WEB), Metric::Memory, 10, 102)?;
    assert_eq!(memory.basis, Some(Basis::Limit));
    assert_eq!(memory.latest, Some(percent(3_000)));
    assert_eq!(memory.average, Some(percent(2_000)));
    assert_eq!(memory.samples, [Some(percent(1_000)), Some(percent(3_000))]);
    // The other container's series is its own.
    let other = read(&store, container(API), Metric::Cpu, 15, 102)?;
    assert_eq!(other.average, Some(percent(50)));
    // Without the samples the answer is the same and carries none.
    let brief = store
        .read(&query(container(WEB), Metric::Cpu, 15, false)?, Step(102))
        .ok_or("no reading")?;
    assert!(brief.samples.is_empty());
    assert_eq!(brief.average, cpu.average);
    // A container the store never saw has no reading, not an empty one.
    assert_eq!(
        store.read(
            &query(container("unknown"), Metric::Cpu, 15, false)?,
            Step(102)
        ),
        None
    );
    Ok(())
}

#[test]
fn a_read_while_the_step_is_being_sampled_ends_one_step_earlier() -> TestResult {
    let mut store = MetricStore::new();
    store.record(Step(100), &[sample(WEB, Some(1_000))]);
    store.record(Step(101), &[sample(WEB, Some(3_000))]);
    // Step 102 has begun and the sampler has not written it yet.
    let reading = read(&store, container(WEB), Metric::Cpu, 10, 102)?;
    assert_eq!(reading.latest, Some(percent(3_000)));
    assert_eq!(reading.average, Some(percent(2_000)));
    assert_eq!(reading.age_seconds, 5);
    assert_eq!(
        reading.samples,
        [Some(percent(1_000)), Some(percent(3_000))]
    );
    // Two steps without anything new: the newest step has no sample.
    let stale = read(&store, container(WEB), Metric::Cpu, 10, 103)?;
    assert_eq!(stale.latest, None);
    assert_eq!(stale.average, Some(percent(3_000)));
    assert_eq!(stale.sampled_seconds, 5);
    assert_eq!(stale.samples, [Some(percent(3_000)), None]);
    // And a read as of an earlier step answers what was known then.
    let earlier = read(&store, container(WEB), Metric::Cpu, 10, 100)?;
    assert_eq!(earlier.latest, Some(percent(1_000)));
    assert_eq!(earlier.samples, [None, Some(percent(1_000))]);
    Ok(())
}

#[test]
fn a_step_without_a_sample_is_never_zero() -> TestResult {
    let mut store = MetricStore::new();
    store.record(Step(10), &[sample(WEB, Some(8_000))]);
    // The container was listed and could not be read.
    store.record(Step(11), &[sample(WEB, None)]);
    // The sampler missed two steps altogether.
    store.record(Step(14), &[sample(WEB, Some(4_000))]);
    let reading = read(&store, container(WEB), Metric::Cpu, 25, 14)?;
    assert_eq!(
        reading.samples,
        [Some(percent(8_000)), None, None, None, Some(percent(4_000))]
    );
    assert_eq!(reading.average, Some(percent(6_000)));
    assert_eq!(reading.window_seconds, 25);
    assert_eq!(reading.sampled_seconds, 10);
    let empty = read(&store, container(WEB), Metric::Cpu, 15, 13)?;
    assert_eq!(empty.latest, None);
    assert_eq!(empty.average, None);
    assert_eq!(empty.sampled_seconds, 0);
    // A real zero is a sample.
    store.record(Step(15), &[sample(WEB, Some(0))]);
    let zero = read(&store, container(WEB), Metric::Cpu, 5, 15)?;
    assert_eq!(zero.latest, Some(Percent::ZERO));
    assert_eq!(zero.average, Some(Percent::ZERO));
    Ok(())
}

#[test]
fn a_series_shorter_than_the_window_averages_what_it_has() -> TestResult {
    let mut store = MetricStore::new();
    store.record(Step(3), &[sample(WEB, Some(2_000))]);
    store.record(Step(4), &[sample(WEB, Some(4_000))]);
    let reading = read(&store, container(WEB), Metric::Cpu, 30, 4)?;
    assert_eq!(reading.average, Some(percent(3_000)));
    assert_eq!(reading.window_seconds, 30);
    assert_eq!(reading.sampled_seconds, 10);
    // Six steps, of which the first one is before the store's first step.
    assert_eq!(
        reading.samples,
        [
            None,
            None,
            None,
            None,
            Some(percent(2_000)),
            Some(percent(4_000))
        ]
    );
    let longest = store
        .read(
            &MetricQuery {
                subject: container(WEB),
                metric: Metric::Cpu,
                window: Window::LONGEST,
                samples: true,
            },
            Step(4),
        )
        .ok_or("no reading")?;
    assert_eq!(longest.average, Some(percent(3_000)));
    assert_eq!(longest.samples.len(), 2_880);
    Ok(())
}

/// What a read must answer, worked out by walking the samples.
fn walked(samples: &[Option<u64>], end: usize, steps: usize) -> (Option<Percent>, u64) {
    let from = (end + 1).saturating_sub(steps);
    let present = samples[from..=end.min(samples.len() - 1)]
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let count = present.len() as u64;
    let average = (present.iter().sum::<u64>() + count / 2)
        .checked_div(count)
        .map(percent);
    (average, count * STEP_SECONDS)
}

#[test]
fn every_window_averages_what_a_walk_over_the_samples_finds() -> TestResult {
    let mut numbers = Numbers(7);
    // More than two laps of the ring, with holes of every size.
    let mut samples = Vec::<Option<u64>>::new();
    while samples.len() < 7_000 {
        match numbers.next() % 40 {
            0 => samples.extend(vec![None; (numbers.next() % 150) as usize]),
            1 => samples.push(None),
            _ => samples.push(Some(numbers.next() % 30_000)),
        }
    }
    let mut store = MetricStore::new();
    let windows = [1, 2, 6, 60, 63, 64, 65, 127, 128, 700, 2_879, 2_880];
    for (step, value) in samples.iter().enumerate() {
        // A hole is a step the sampler never recorded.
        if value.is_some() {
            store.record(Step(step as u64), &[sample(WEB, *value)]);
        }
        if !step.is_multiple_of(97) && step < 6_900 {
            continue;
        }
        let Some(newest) = samples[..=step].iter().rposition(Option::is_some) else {
            continue;
        };
        for steps in windows {
            // As of this step: the window ends at it when it is written
            // and one step earlier when it is not.
            let end = if newest == step { step } else { step - 1 };
            let reading = read(
                &store,
                container(WEB),
                Metric::Cpu,
                steps * STEP_SECONDS,
                step as u64,
            )?;
            let (average, sampled) = walked(&samples, end, steps as usize);
            assert_eq!(
                (reading.average, reading.sampled_seconds),
                (average, sampled),
                "the last {steps} steps as of step {step}"
            );
            // And as of an earlier step that the ring still holds.
            let earlier = step.saturating_sub(40);
            if newest >= earlier {
                let reading = read(
                    &store,
                    container(WEB),
                    Metric::Cpu,
                    steps.min(2_800) * STEP_SECONDS,
                    earlier as u64,
                )?;
                let (average, sampled) = walked(&samples, earlier, steps.min(2_800) as usize);
                assert_eq!(
                    (reading.average, reading.sampled_seconds),
                    (average, sampled),
                    "the last {steps} steps as of the earlier step {earlier}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_gap_longer_than_four_hours_starts_the_series_over() -> TestResult {
    let mut store = MetricStore::new();
    store.record(Step(5), &[sample(WEB, Some(9_000))]);
    // Listed all the time, unreadable for five hours.
    for step in (6..3_700).step_by(10) {
        store.record(Step(step), &[sample(WEB, None)]);
    }
    store.record(Step(3_700), &[sample(WEB, Some(1_000))]);
    store.record(Step(3_701), &[sample(WEB, Some(3_000))]);
    let reading = store
        .read(
            &MetricQuery {
                subject: container(WEB),
                metric: Metric::Cpu,
                window: Window::LONGEST,
                samples: false,
            },
            Step(3_701),
        )
        .ok_or("no reading")?;
    assert_eq!(reading.average, Some(percent(2_000)));
    assert_eq!(reading.sampled_seconds, 10);
    Ok(())
}

#[test]
fn an_older_step_never_replaces_a_newer_one() -> TestResult {
    let mut store = MetricStore::new();
    store.record(Step(20), &[sample(WEB, Some(5_000))]);
    store.record(Step(20), &[sample(WEB, Some(100))]);
    store.record(Step(19), &[sample(WEB, Some(100))]);
    let reading = read(&store, container(WEB), Metric::Cpu, 10, 20)?;
    assert_eq!(reading.samples, [None, Some(percent(5_000))]);
    Ok(())
}

fn seconds(steps: u64) -> Duration {
    Duration::from_secs(steps * STEP_SECONDS)
}

#[test]
fn a_condition_has_held_since_the_step_that_last_broke_it() -> TestResult {
    let mut store = MetricStore::new();
    let above = Condition::Greater(percent(7_500));
    let below = Condition::Less(percent(4_000));
    let held = |store: &mut MetricStore, condition, now| {
        store.held(&container(WEB), Metric::Cpu, condition, Step(now))
    };
    assert_eq!(held(&mut store, above, 0), Duration::ZERO);
    for (step, cpu) in [(1, 9_000), (2, 8_000), (3, 7_501)] {
        store.record(Step(step), &[sample(WEB, Some(cpu))]);
    }
    assert_eq!(held(&mut store, above, 3), seconds(3));
    assert_eq!(held(&mut store, below, 3), Duration::ZERO);
    // While step 4 is being sampled the answer is the one as of step 3.
    assert_eq!(held(&mut store, above, 4), seconds(3));
    // Exactly the threshold is not above it.
    store.record(Step(4), &[sample(WEB, Some(7_500))]);
    assert_eq!(held(&mut store, above, 4), Duration::ZERO);
    store.record(Step(5), &[sample(WEB, Some(9_900))]);
    store.record(Step(6), &[sample(WEB, Some(9_900))]);
    assert_eq!(held(&mut store, above, 6), seconds(2));
    // A step without a sample breaks it: nobody knows what it was.
    store.record(Step(7), &[sample(WEB, None)]);
    assert_eq!(held(&mut store, above, 7), Duration::ZERO);
    store.record(Step(8), &[sample(WEB, Some(9_900))]);
    assert_eq!(held(&mut store, above, 8), seconds(1));
    // So does a step the sampler missed.
    store.record(Step(10), &[sample(WEB, Some(9_900))]);
    assert_eq!(held(&mut store, above, 10), seconds(1));
    // And a series nothing was added to for two steps holds nothing.
    assert_eq!(held(&mut store, above, 11), seconds(1));
    assert_eq!(held(&mut store, above, 12), Duration::ZERO);
    for step in 13..=30 {
        store.record(Step(step), &[sample(WEB, Some(1_000))]);
    }
    assert_eq!(held(&mut store, below, 30), seconds(18));
    assert_eq!(held(&mut store, above, 30), Duration::ZERO);
    // Memory is its own series.
    let memory = store.held(
        &container(WEB),
        Metric::Memory,
        Condition::Less(percent(501)),
        Step(30),
    );
    assert_eq!(memory, seconds(18));
    assert_eq!(
        store.held(&container("unknown"), Metric::Cpu, below, Step(30)),
        Duration::ZERO
    );
    Ok(())
}

/// How long `condition` has held as of `step`, by walking the samples.
fn walked_hold(samples: &[Option<u64>], step: usize, condition: Condition) -> Duration {
    let held = samples[..=step]
        .iter()
        .rev()
        .take(RETAINED_STEPS as usize)
        .take_while(|sample| condition.holds(sample.map(percent)))
        .count();
    seconds(held as u64)
}

#[test]
fn how_long_a_condition_has_held_does_not_depend_on_when_it_was_first_asked() -> TestResult {
    let mut numbers = Numbers(11);
    let mut samples = Vec::<Option<u64>>::new();
    while samples.len() < 6_500 {
        // Long calm stretches, a busy step now and then, and holes.
        match numbers.next() % 400 {
            0 => samples.extend(vec![None; (numbers.next() % 4) as usize + 1]),
            1 | 2 => samples.push(Some(9_000)),
            _ => samples.push(Some(numbers.next() % 3_000)),
        }
    }
    // More conditions than a series follows at once.
    let conditions = [
        Condition::Less(percent(4_000)),
        Condition::Greater(percent(7_500)),
        Condition::Less(percent(2_990)),
        Condition::Greater(percent(100)),
        Condition::Less(percent(2_000)),
        Condition::Greater(percent(2_500)),
    ];
    // One store is asked at every step, one only now and then.
    let mut always = MetricStore::new();
    let mut seldom = MetricStore::new();
    for (step, value) in samples.iter().enumerate() {
        let now = Step(step as u64);
        if value.is_some() {
            always.record(now, &[sample(WEB, *value)]);
            seldom.record(now, &[sample(WEB, *value)]);
        } else {
            continue;
        }
        for (index, condition) in conditions.into_iter().enumerate() {
            let expected = walked_hold(&samples, step, condition);
            if index < 3 || step.is_multiple_of(5) {
                assert_eq!(
                    always.held(&container(WEB), Metric::Cpu, condition, now),
                    expected,
                    "{condition:?} as of step {step}, asked every step"
                );
            }
            if (step + index).is_multiple_of(389) {
                assert_eq!(
                    seldom.held(&container(WEB), Metric::Cpu, condition, now),
                    expected,
                    "{condition:?} as of step {step}, asked now and then"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_condition_that_always_held_has_held_for_four_hours_at_most() -> TestResult {
    let mut store = MetricStore::new();
    let low = Condition::Less(percent(4_000));
    for step in 0..4_000 {
        store.record(Step(step), &[sample(WEB, Some(100))]);
        if step == 10 {
            assert_eq!(
                store.held(&container(WEB), Metric::Cpu, low, Step(step)),
                seconds(11)
            );
        }
    }
    assert_eq!(
        store.held(&container(WEB), Metric::Cpu, low, Step(3_999)),
        RETAINED
    );
    // Asked for the first time after more than four hours: the same.
    assert_eq!(
        store.held(
            &container(WEB),
            Metric::Cpu,
            Condition::Less(percent(101)),
            Step(3_999)
        ),
        RETAINED
    );
    Ok(())
}

#[test]
fn a_container_that_is_gone_frees_its_series_and_a_restart_keeps_them() -> TestResult {
    let mut store = MetricStore::new();
    let empty = store.memory_bytes();
    store.record(Step(0), &[sample(WEB, Some(100)), sample(API, Some(200))]);
    assert_eq!(store.containers(), 2);
    let both = store.memory_bytes();
    assert!(both - empty >= 2 * MetricStore::CONTAINER_BYTES);
    // The API container restarts: three steps in which it is not listed.
    for step in 1..=3 {
        store.record(Step(step), &[sample(WEB, Some(100))]);
    }
    store.record(Step(4), &[sample(WEB, Some(100)), sample(API, Some(400))]);
    let restarted = read(&store, container(API), Metric::Cpu, 25, 4)?;
    assert_eq!(
        restarted.samples,
        [Some(percent(200)), None, None, None, Some(percent(400))]
    );
    // Then it is removed. Its series stay for a minute and no longer.
    for step in 5..=4 + KEPT_UNLISTED_STEPS {
        store.record(Step(step), &[sample(WEB, Some(100))]);
    }
    assert_eq!(store.containers(), 2);
    store.record(Step(5 + KEPT_UNLISTED_STEPS), &[sample(WEB, Some(100))]);
    assert_eq!(store.containers(), 1);
    assert_eq!(
        store.read(&query(container(API), Metric::Cpu, 5, false)?, Step(17)),
        None
    );
    assert!(store.memory_bytes() <= both - MetricStore::CONTAINER_BYTES);
    // A node that runs nothing any more holds nothing.
    for step in 18..=40 {
        store.record(Step(step), &[]);
    }
    assert_eq!(store.containers(), 0);
    Ok(())
}

#[test]
fn a_node_adds_up_the_newest_step_of_a_services_containers() {
    let mut store = MetricStore::new();
    assert_eq!(store.tally(Metric::Cpu, &[WEB, API]), Tally::NONE);
    store.record(
        Step(7),
        &[
            sample(WEB, Some(3_000)),
            sample(API, Some(5_000)),
            sample("fedcba987654", Some(9_000)),
        ],
    );
    assert_eq!(
        store.tally(Metric::Cpu, &[WEB, API, "unknown"]),
        Tally {
            sum: 8_000,
            count: 2
        }
    );
    assert_eq!(
        store.tally(Metric::Memory, &[WEB.to_owned()]),
        Tally {
            sum: 1_500,
            count: 1
        }
    );
    // A container that gave no sample in the newest step is left out.
    store.record(Step(8), &[sample(WEB, Some(1_000)), sample(API, None)]);
    assert_eq!(
        store.tally(Metric::Cpu, &[WEB, API]),
        Tally {
            sum: 1_000,
            count: 1
        }
    );
}

#[test]
fn a_services_series_holds_the_sum_and_the_count_of_every_step() -> TestResult {
    let mut store = MetricStore::new();
    let shop = "shop/web";
    let step = |sum, count| Tally { sum, count };
    // Two nodes' sums, added up by whoever feeds the store.
    let first = step(6_000, 1).plus(step(8_000, 1));
    assert_eq!(first, step(14_000, 2));
    assert!(store.record_service(shop, Metric::Cpu, Step(50), first));
    assert!(store.record_service(shop, Metric::Cpu, Step(51), step(24_000, 3)));
    // No container gave a sample in step 52; three did in step 53.
    assert!(store.record_service(shop, Metric::Cpu, Step(52), Tally::NONE));
    assert!(store.record_service(shop, Metric::Cpu, Step(53), step(3_000, 3)));
    // The same step again changes nothing.
    assert!(!store.record_service(shop, Metric::Cpu, Step(53), step(60_000, 1)));
    let reading = read(&store, service(shop), Metric::Cpu, 20, 53)?;
    assert_eq!(
        reading,
        MetricReading {
            metric: Metric::Cpu,
            basis: None,
            latest: Some(percent(1_000)),
            containers: Some(3),
            // Every container's sample weighs the same: 41,000 over 8.
            average: Some(percent(5_125)),
            window_seconds: 20,
            sampled_seconds: 15,
            step_seconds: 5,
            age_seconds: 0,
            samples: vec![
                Some(percent(7_000)),
                Some(percent(8_000)),
                None,
                Some(percent(1_000))
            ],
        }
    );
    // The rule's question, on the service's average of each step.
    let above = Condition::Greater(percent(7_500));
    let low = Condition::Less(percent(4_000));
    assert_eq!(
        store.held(&service(shop), Metric::Cpu, low, Step(53)),
        seconds(1)
    );
    for now in 54..60 {
        assert!(store.record_service(shop, Metric::Cpu, Step(now), step(18_000, 2)));
    }
    assert_eq!(
        store.held(&service(shop), Metric::Cpu, above, Step(59)),
        seconds(6)
    );
    assert_eq!(
        store.held(&service(shop), Metric::Cpu, low, Step(59)),
        Duration::ZERO
    );
    // A metric nobody recorded has no series, and takes no memory.
    assert_eq!(
        store.read(&query(service(shop), Metric::Memory, 20, false)?, Step(59)),
        None
    );
    assert_eq!(
        store.held(&service(shop), Metric::Memory, low, Step(59)),
        Duration::ZERO
    );
    let held = store.memory_bytes();
    assert!(store.record_service(shop, Metric::Memory, Step(59), step(100, 1)));
    assert_eq!(
        store.memory_bytes() - held,
        MetricStore::SERVICE_METRIC_BYTES
    );
    // The series lives until the service is forgotten.
    assert_eq!(store.services(), 1);
    assert!(store.forget_service(shop));
    assert!(!store.forget_service(shop));
    assert_eq!(store.services(), 0);
    assert_eq!(
        store.read(&query(service(shop), Metric::Cpu, 20, false)?, Step(59)),
        None
    );
    Ok(())
}

#[test]
fn sums_stop_at_their_largest_instead_of_wrapping() {
    let full = Tally {
        sum: u32::MAX - 10,
        count: u16::MAX - 1,
    };
    let more = full.plus(Tally::one(Percent::MAX));
    assert_eq!(
        more,
        Tally {
            sum: u32::MAX,
            count: u16::MAX
        }
    );
    assert_eq!(more.plus(Tally::one(Percent::MAX)), more);
    // 65,535 containers at the largest sample still fit.
    let mut all = Tally::NONE;
    for _ in 0..u16::MAX {
        all = all.plus(Tally::one(Percent::MAX));
    }
    assert_eq!(all.average(), Some(Percent::MAX));
    assert_eq!(Tally::NONE.average(), None);
    // A sample beyond the largest reads as the largest, in the series too.
    let mut store = MetricStore::new();
    store.record(Step(1), &[sample(WEB, Some(9_999_999))]);
    let reading = store.read(
        &MetricQuery {
            subject: container(WEB),
            metric: Metric::Cpu,
            window: Window::NEWEST,
            samples: false,
        },
        Step(1),
    );
    assert_eq!(
        reading.and_then(|reading| reading.latest),
        Some(Percent::MAX)
    );
}

#[test]
fn a_reading_and_a_query_are_the_same_document_for_every_interface() -> TestResult {
    let reading = MetricReading {
        metric: Metric::Cpu,
        basis: Some(Basis::OneCore),
        latest: Some(percent(8_212)),
        containers: None,
        average: Some(percent(7_500)),
        window_seconds: 30,
        sampled_seconds: 25,
        step_seconds: 5,
        age_seconds: 0,
        samples: vec![Some(percent(100)), None],
    };
    let document = serde_json::to_value(&reading)?;
    assert_eq!(
        document,
        serde_json::json!({
            "metric": "cpu",
            "basis": "one-core",
            "latest": 82.12,
            "average": 75.0,
            "window-seconds": 30,
            "sampled-seconds": 25,
            "step-seconds": 5,
            "age-seconds": 0,
            "samples": [1.0, null],
        })
    );
    assert_eq!(serde_json::from_value::<MetricReading>(document)?, reading);
    let asked: MetricQuery = serde_json::from_value(serde_json::json!({
        "subject": {"service": "shop/web"},
        "metric": "memory",
        "window": 300,
    }))?;
    assert_eq!(
        asked,
        query(service("shop/web"), Metric::Memory, 300, false)?
    );
    assert_eq!(
        serde_json::to_value(query(container(WEB), Metric::Cpu, 30, true)?)?,
        serde_json::json!({
            "subject": {"container": WEB},
            "metric": "cpu",
            "window": 30,
            "samples": true,
        })
    );
    let schema = serde_json::to_value(schemars::schema_for!(MetricReading))?;
    assert_eq!(schema["$defs"]["Percent"]["type"], "number");
    let schema = serde_json::to_value(schemars::schema_for!(MetricQuery))?;
    assert_eq!(schema["$defs"]["MetricWindow"]["maximum"], 14_400);
    Ok(())
}

fn reading(at_ms: u64, cpu_usec: u64) -> UsageReading {
    UsageReading {
        at: Duration::from_millis(at_ms),
        cpu_usec,
        cpu_limit: None,
        memory_bytes: Some(512 * 1024 * 1024),
        memory_limit: None,
    }
}

#[test]
fn cpu_is_a_share_of_the_limit_or_of_one_core() {
    let earlier = reading(1_000, 10_000_000);
    // 2.5 s of CPU time in 5 s: half a core.
    let later = reading(6_000, 12_500_000);
    assert_eq!(
        later.cpu_since(&earlier),
        Some(Measured {
            share: percent(5_000),
            of: Basis::OneCore
        })
    );
    // Three cores busy: 300% of one core.
    assert_eq!(
        reading(6_000, 25_000_000).cpu_since(&earlier),
        Some(Measured {
            share: percent(30_000),
            of: Basis::OneCore
        })
    );
    // The same half core is all of a limit of half a core,
    let limited = |quota_usec, period_usec| UsageReading {
        cpu_limit: Some(CpuLimit {
            quota_usec,
            period_usec,
        }),
        ..later
    };
    assert_eq!(
        limited(50_000, 100_000).cpu_since(&earlier),
        Some(Measured {
            share: percent(10_000),
            of: Basis::Limit
        })
    );
    // and a quarter of a limit of two cores.
    assert_eq!(
        limited(200_000, 100_000).cpu_since(&earlier),
        Some(Measured {
            share: percent(2_500),
            of: Basis::Limit
        })
    );
    // A limit of nothing is no limit.
    assert_eq!(
        limited(0, 100_000).cpu_since(&earlier).map(|cpu| cpu.of),
        Some(Basis::OneCore)
    );
    // An idle container used exactly nothing.
    assert_eq!(
        reading(6_000, 10_000_000)
            .cpu_since(&earlier)
            .map(|cpu| cpu.share),
        Some(Percent::ZERO)
    );
}

#[test]
fn cpu_has_no_sample_across_a_restart_or_a_long_pause() {
    let earlier = reading(1_000, 10_000_000);
    // The counter started over: the container restarted.
    assert_eq!(reading(6_000, 40_000).cpu_since(&earlier), None);
    // The two readings are not one step apart, or not in order.
    assert_eq!(reading(16_001, 12_500_000).cpu_since(&earlier), None);
    assert_eq!(reading(1_000, 12_500_000).cpu_since(&earlier), None);
    assert_eq!(reading(500, 12_500_000).cpu_since(&earlier), None);
    assert!(reading(16_000, 12_500_000).cpu_since(&earlier).is_some());
}

#[test]
fn memory_is_a_share_of_the_limit_or_of_the_nodes_memory() {
    let unlimited = reading(0, 0);
    let node = 2 * 1024 * 1024 * 1024;
    assert_eq!(
        unlimited.memory(Some(node)),
        Some(Measured {
            share: percent(2_500),
            of: Basis::NodeMemory
        })
    );
    assert_eq!(unlimited.memory(None), None);
    let limited = UsageReading {
        memory_limit: Some(1024 * 1024 * 1024),
        ..unlimited
    };
    for node in [Some(node), None] {
        assert_eq!(
            limited.memory(node),
            Some(Measured {
                share: percent(5_000),
                of: Basis::Limit
            })
        );
    }
    // A host that does not account the container's memory.
    let unaccounted = UsageReading {
        memory_bytes: None,
        ..limited
    };
    assert_eq!(unaccounted.memory(Some(node)), None);
}
