//! Sampling a node's containers: the counters read from cgroup v2 files
//! under both of the engine's cgroup drivers, for a container with limits
//! and one without; what the engine is asked instead and when; and what a
//! round of sampling leaves in the store.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mm::adapters::metrics::CgroupCounters;
use mm::adapters::runtime::DockerComposeRuntime;
use mm::app::MetricsSampler;
use mm::app::Sampled;
use mm::app::ports::{
    CommandOutput, CommandSpec, EngineUsage, MeteredContainers, ProcessRunner, RuntimeDriver,
    UsageCounters,
};
use mm::domain::error::MmError;
use mm::domain::metrics::{
    Basis, ContainerSample, CpuLimit, Measured, Metric, MetricQuery, MetricReading, Percent, Step,
    Subject, UsageReading, Window,
};
use zeroize::Zeroizing;

mod temp_dirs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A container with half a core and one gibibyte as its limits.
const LIMITED: &str = "4c1f0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a";
/// A container without limits.
const UNLIMITED: &str = "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0f9e8d";
const GIBIBYTE: u64 = 1024 * 1024 * 1024;
/// A container whose counters the host does not show.
const NONE: Option<UsageReading> = None;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cgroup")
        .join(name)
}

fn short(id: &str) -> String {
    id[..12].to_owned()
}

fn percent(hundredths: u64) -> Percent {
    Percent::from_hundredths(hundredths)
}

/// CPU microseconds, the CPU limit, the memory in use, the memory limit.
type Counters = (u64, Option<CpuLimit>, Option<u64>, Option<u64>);

/// The two containers' counters as the fixture files hold them, without
/// the moment they were read.
fn counters(readings: &[Option<UsageReading>]) -> Vec<Option<Counters>> {
    readings
        .iter()
        .map(|reading| {
            reading.map(|reading| {
                (
                    reading.cpu_usec,
                    reading.cpu_limit,
                    reading.memory_bytes,
                    reading.memory_limit,
                )
            })
        })
        .collect()
}

#[test]
fn counters_are_read_under_the_systemd_driver_and_under_cgroupfs() {
    // Docker's places, and rootful Podman's (its conmon's group next to a
    // container's is no container).
    for driver in ["systemd", "cgroupfs", "podman-systemd", "podman-cgroupfs"] {
        let host = CgroupCounters::at(fixture(driver), Some(2 * GIBIBYTE));
        assert_eq!(host.node_memory(), Some(2 * GIBIBYTE));
        // By the twelve characters a node lists a container under, and
        // by the whole id.
        for ids in [
            [short(LIMITED), short(UNLIMITED), "0123456789ab".to_owned()],
            [LIMITED.to_owned(), UNLIMITED.to_owned(), "f".repeat(64)],
        ] {
            let readings = host.read(&ids);
            assert_eq!(
                counters(&readings),
                [
                    Some((
                        33_369_402,
                        Some(CpuLimit {
                            quota_usec: 50_000,
                            period_usec: 100_000
                        }),
                        // 289,873,920 bytes in use, of which 21,438,464
                        // are file cache the kernel gives back at once.
                        Some(256 * 1024 * 1024),
                        Some(GIBIBYTE)
                    )),
                    Some((33_369_402, None, Some(256 * 1024 * 1024), None)),
                    None
                ],
                "{driver}"
            );
            // What a step stores of it: a share of the limit, or of the
            // node's memory.
            let memory = readings
                .iter()
                .flatten()
                .map(|reading| reading.memory(host.node_memory()))
                .collect::<Vec<_>>();
            assert_eq!(
                memory,
                [
                    Some(Measured {
                        share: percent(2_500),
                        of: Basis::Limit
                    }),
                    Some(Measured {
                        share: percent(1_250),
                        of: Basis::NodeMemory
                    })
                ],
                "{driver}"
            );
        }
        // Fewer characters than the engine itself takes name no container.
        assert_eq!(host.read(&[LIMITED[..11].to_owned()]), [NONE], "{driver}");
        assert!(host.read(&[]).is_empty());
    }
}

#[test]
fn a_host_that_mounts_cgroup_v1_shows_no_counters() {
    let host = CgroupCounters::at(fixture("v1"), None);
    assert_eq!(
        host.read(&[short(LIMITED), LIMITED.to_owned()]),
        [NONE, NONE]
    );
    let nowhere = CgroupCounters::at(fixture("no-such-directory"), None);
    assert_eq!(nowhere.read(&[short(LIMITED)]), [NONE]);
}

fn copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[test]
fn a_container_that_restarts_or_changes_its_limits_is_read_again() -> TestResult {
    let directory = temp_dirs::tempdir()?;
    let root = directory.path().join("cgroup");
    copy(&fixture("systemd"), &root)?;
    let group = root.join(format!("system.slice/docker-{LIMITED}.scope"));
    let host = CgroupCounters::at(&root, None);
    let ids = [short(LIMITED)];
    let first = host.read(&ids)[0].ok_or("no reading")?;
    // The counters move on, and the limit is raised to two cores.
    std::fs::write(group.join("cpu.stat"), "usage_usec 34000000\nuser_usec 1\n")?;
    std::fs::write(group.join("cpu.max"), "200000 100000\n")?;
    let second = host.read(&ids)[0].ok_or("no reading")?;
    assert_eq!(second.cpu_usec, 34_000_000);
    assert_eq!(
        second.cpu_limit,
        Some(CpuLimit {
            quota_usec: 200_000,
            period_usec: 100_000
        })
    );
    assert!(second.at >= first.at);
    // The container stops: its control group is gone.
    let kept = root.join("kept");
    std::fs::rename(&group, &kept)?;
    assert_eq!(host.read(&ids), [NONE]);
    // It starts again under the same id, with counters that begin anew.
    std::fs::rename(&kept, &group)?;
    std::fs::write(group.join("cpu.stat"), "usage_usec 1200\n")?;
    let third = host.read(&ids)[0].ok_or("no reading")?;
    assert_eq!(third.cpu_usec, 1_200);
    assert_eq!(third.cpu_since(&second), None, "the counter started over");
    // Where the memory controller is not handed down to the container's
    // group, the group has no memory files: CPU is read all the same.
    for name in ["memory.current", "memory.stat", "memory.max"] {
        std::fs::remove_file(group.join(name))?;
    }
    let fourth = host.read(&ids)[0].ok_or("no reading")?;
    assert_eq!((fourth.cpu_usec, fourth.memory_bytes), (1_200, None));
    assert_eq!(fourth.memory(Some(GIBIBYTE)), None);
    Ok(())
}

/// The containers a node lists, as a test sets them.
struct Listed(Mutex<Vec<String>>);

impl Listed {
    fn new(containers: &[&str]) -> Arc<Self> {
        Arc::new(Self(Mutex::new(
            containers.iter().map(|id| (*id).to_owned()).collect(),
        )))
    }

    fn set(&self, containers: &[&str]) -> Result<(), String> {
        *self.0.lock().map_err(|_| "poisoned")? =
            containers.iter().map(|id| (*id).to_owned()).collect();
        Ok(())
    }
}

impl MeteredContainers for Listed {
    fn running(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|listed| listed.clone())
            .unwrap_or_default()
    }
}

/// A container's id, the CPU microseconds it uses between two reads, its
/// CPU limit, and its memory limit.
type Moving = (String, u64, Option<CpuLimit>, Option<u64>);

/// A host whose counters a test moves: each container's CPU time grows by
/// what the test says between two reads, five seconds apart.
struct Host {
    /// For each container: the CPU microseconds it uses per read, and its
    /// limits. A container that is not here has no counters.
    containers: Mutex<Vec<Moving>>,
    reads: AtomicUsize,
}

impl Host {
    fn new(containers: &[(&str, u64, Option<CpuLimit>, Option<u64>)]) -> Arc<Self> {
        Arc::new(Self {
            containers: Mutex::new(
                containers
                    .iter()
                    .map(|(id, used, cpu, memory)| ((*id).to_owned(), *used, *cpu, *memory))
                    .collect(),
            ),
            reads: AtomicUsize::new(0),
        })
    }
}

impl UsageCounters for Host {
    fn read(&self, containers: &[String]) -> Vec<Option<UsageReading>> {
        let round = self.reads.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        let known = self
            .containers
            .lock()
            .map(|known| known.clone())
            .unwrap_or_default();
        containers
            .iter()
            .map(|container| {
                let (_, used, cpu_limit, memory_limit) =
                    known.iter().find(|(id, ..)| id == container)?;
                Some(UsageReading {
                    at: Duration::from_secs(5 * round),
                    cpu_usec: used * round,
                    cpu_limit: *cpu_limit,
                    memory_bytes: Some(GIBIBYTE / 2),
                    memory_limit: *memory_limit,
                })
            })
            .collect()
    }

    fn node_memory(&self) -> Option<u64> {
        Some(4 * GIBIBYTE)
    }
}

/// An engine that answers with what a test gives it, and counts the asks.
struct Engine {
    answers: Mutex<VecDeque<Result<Vec<ContainerSample>, MmError>>>,
    asked: Mutex<Vec<Vec<String>>>,
}

impl Engine {
    fn new(answers: Vec<Result<Vec<ContainerSample>, MmError>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into()),
            asked: Mutex::new(Vec::new()),
        })
    }
}

impl EngineUsage for Engine {
    fn usage(&self, containers: &[String]) -> Result<Vec<ContainerSample>, MmError> {
        let poisoned = || MmError::State {
            message: "the test engine's lock was poisoned".to_owned(),
        };
        self.asked
            .lock()
            .map_err(|_| poisoned())?
            .push(containers.to_vec());
        self.answers
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .unwrap_or_else(|| Ok(Vec::new()))
    }
}

fn read(
    sampler: &MetricsSampler,
    container: &str,
    metric: Metric,
    steps: u64,
    now: u64,
) -> Result<MetricReading, Box<dyn std::error::Error>> {
    let query = MetricQuery {
        subject: Subject::Container(container.to_owned()),
        metric,
        window: Window::of(Duration::from_secs(steps * 5)).ok_or("no such window")?,
        samples: true,
    };
    Ok(sampler
        .metrics()
        .read_as_of(&query, Step(now))
        .ok_or("the store holds no such series")?)
}

const HALF_A_CORE: Option<CpuLimit> = Some(CpuLimit {
    quota_usec: 50_000,
    period_usec: 100_000,
});

#[test]
fn a_round_records_cpu_between_two_readings_and_memory_at_once() -> TestResult {
    let listed = Listed::new(&["web", "api"]);
    // Each read is five seconds after the one before. `web` uses 2 s of
    // CPU time in them and may use 2.5 s; `api` uses 7.5 s and has no
    // limits.
    let host = Host::new(&[
        ("web", 2_000_000, HALF_A_CORE, Some(GIBIBYTE)),
        ("api", 7_500_000, None, None),
    ]);
    let sampler = MetricsSampler::new(listed.clone(), host.clone(), None);
    assert_eq!(
        sampler.sample(Step(10)),
        Sampled {
            containers: 2,
            from_counters: 2,
            from_engine: 0
        }
    );
    // The first round has nothing to compare the CPU counter with.
    let cpu = read(&sampler, "web", Metric::Cpu, 1, 10)?;
    assert_eq!((cpu.latest, cpu.basis), (None, None));
    let memory = read(&sampler, "web", Metric::Memory, 1, 10)?;
    assert_eq!(
        (memory.latest, memory.basis),
        (Some(percent(5_000)), Some(Basis::Limit))
    );
    sampler.sample(Step(11));
    sampler.sample(Step(12));
    let cpu = read(&sampler, "web", Metric::Cpu, 3, 12)?;
    assert_eq!(
        cpu.samples,
        [None, Some(percent(8_000)), Some(percent(8_000))]
    );
    assert_eq!(cpu.basis, Some(Basis::Limit));
    let cpu = read(&sampler, "api", Metric::Cpu, 3, 12)?;
    assert_eq!(cpu.average, Some(percent(15_000)));
    assert_eq!(cpu.basis, Some(Basis::OneCore));
    let memory = read(&sampler, "api", Metric::Memory, 3, 12)?;
    assert_eq!(
        (memory.average, memory.basis),
        (Some(percent(1_250)), Some(Basis::NodeMemory))
    );
    assert_eq!(memory.sampled_seconds, 15);
    assert_eq!(host.reads.load(Ordering::SeqCst), 3);
    // A container that is listed again after a while starts with a new
    // first reading, and one that is no longer listed is not read.
    listed.set(&["web"])?;
    sampler.sample(Step(13));
    listed.set(&["web", "api"])?;
    sampler.sample(Step(14));
    let cpu = read(&sampler, "api", Metric::Cpu, 3, 14)?;
    assert_eq!(cpu.samples, [Some(percent(15_000)), None, None]);
    sampler.sample(Step(15));
    assert_eq!(
        read(&sampler, "api", Metric::Cpu, 1, 15)?.latest,
        Some(percent(15_000))
    );
    Ok(())
}

#[test]
fn a_node_without_containers_reads_nothing() {
    let host = Host::new(&[]);
    let engine = Engine::new(Vec::new());
    let sampler = MetricsSampler::new(Listed::new(&[]), host.clone(), Some(engine.clone()));
    for step in 0..5 {
        assert_eq!(sampler.sample(Step(step)), Sampled::default());
    }
    assert_eq!(host.reads.load(Ordering::SeqCst), 0);
    assert!(engine.asked.lock().is_ok_and(|asked| asked.is_empty()));
    assert_eq!(sampler.metrics().containers(), 0);
}

#[test]
fn the_engine_is_asked_only_about_a_container_without_counters() -> TestResult {
    let host = Host::new(&[("web", 1_000_000, None, None)]);
    let from_engine = |cpu| ContainerSample {
        container: "odd".to_owned(),
        cpu: Some(Measured {
            share: percent(cpu),
            of: Basis::Limit,
        }),
        memory: Some(Measured {
            share: percent(300),
            of: Basis::NodeMemory,
        }),
    };
    let engine = Engine::new(vec![
        Ok(vec![from_engine(4_000)]),
        Err(MmError::ProcessTimeout {
            program: "the engine's stats".to_owned(),
            seconds: 4,
        }),
        Ok(vec![from_engine(6_000)]),
    ]);
    let sampler = MetricsSampler::new(
        Listed::new(&["web", "odd", "lost"]),
        host,
        Some(engine.clone()),
    );
    assert_eq!(
        sampler.sample(Step(1)),
        Sampled {
            containers: 3,
            from_counters: 1,
            from_engine: 1
        }
    );
    // The engine did not answer in time: a step without a sample.
    assert_eq!(sampler.sample(Step(2)).from_engine, 0);
    assert_eq!(sampler.sample(Step(3)).from_engine, 1);
    let asked = engine.asked.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(asked, vec![vec!["odd".to_owned(), "lost".to_owned()]; 3]);
    let cpu = read(&sampler, "odd", Metric::Cpu, 3, 3)?;
    assert_eq!(
        cpu.samples,
        [Some(percent(4_000)), None, Some(percent(6_000))]
    );
    assert_eq!(cpu.basis, Some(Basis::Limit));
    assert_eq!(
        read(&sampler, "odd", Metric::Memory, 3, 3)?.average,
        Some(percent(300))
    );
    // Nobody has numbers for this one. It runs, so it has series, and
    // every step of them is without a sample.
    let lost = read(&sampler, "lost", Metric::Memory, 3, 3)?;
    assert_eq!((lost.average, lost.sampled_seconds), (None, 0));
    // Without an engine to ask, the same holds for both.
    let alone = MetricsSampler::new(
        Listed::new(&["web", "odd"]),
        Host::new(&[("web", 1_000_000, None, None)]),
        None,
    );
    assert_eq!(
        alone.sample(Step(1)),
        Sampled {
            containers: 2,
            from_counters: 1,
            from_engine: 0
        }
    );
    assert_eq!(read(&alone, "odd", Metric::Cpu, 1, 1)?.latest, None);
    Ok(())
}

#[test]
fn the_sampler_reads_the_files_of_both_drivers() -> TestResult {
    for driver in ["systemd", "cgroupfs", "podman-systemd", "podman-cgroupfs"] {
        let (limited, unlimited) = (short(LIMITED), short(UNLIMITED));
        let sampler = MetricsSampler::new(
            Listed::new(&[&limited, &unlimited]),
            Arc::new(CgroupCounters::at(fixture(driver), Some(2 * GIBIBYTE))),
            None,
        );
        assert_eq!(sampler.sample(Step(1)).from_counters, 2, "{driver}");
        sampler.sample(Step(2));
        let memory = read(&sampler, &limited, Metric::Memory, 2, 2)?;
        assert_eq!(memory.average, Some(percent(2_500)), "{driver}");
        assert_eq!(memory.basis, Some(Basis::Limit));
        let memory = read(&sampler, &unlimited, Metric::Memory, 2, 2)?;
        assert_eq!(memory.average, Some(percent(1_250)), "{driver}");
        assert_eq!(memory.basis, Some(Basis::NodeMemory));
        // The counter did not move between the two rounds: no CPU used,
        // of the limit for one container and of one core for the other.
        let cpu = read(&sampler, &limited, Metric::Cpu, 2, 2)?;
        assert_eq!(cpu.samples, [None, Some(Percent::ZERO)], "{driver}");
        assert_eq!(cpu.basis, Some(Basis::Limit));
        let cpu = read(&sampler, &unlimited, Metric::Cpu, 2, 2)?;
        assert_eq!(cpu.basis, Some(Basis::OneCore));
    }
    Ok(())
}

struct Commands {
    outputs: Mutex<VecDeque<&'static str>>,
    commands: Mutex<Vec<Vec<String>>>,
}

#[async_trait::async_trait]
impl ProcessRunner for Commands {
    async fn run(&self, specification: CommandSpec) -> Result<CommandOutput, MmError> {
        let poisoned = || MmError::State {
            message: "the test engine's lock was poisoned".to_owned(),
        };
        self.commands
            .lock()
            .map_err(|_| poisoned())?
            .push(specification.redacted_record().arguments);
        let stdout = self
            .outputs
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "the test engine has no more output".to_owned(),
            })?;
        Ok(CommandOutput {
            code: Some(0),
            stdout: Zeroizing::new(stdout.to_owned()),
            stderr: Zeroizing::new(String::new()),
            truncated: false,
        })
    }
}

const INSPECTED: &str = r#"[
 {"Id": "4c1f0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a",
  "HostConfig": {"NanoCpus": 500000000, "CpuQuota": 0, "CpuPeriod": 0, "Memory": 1073741824}},
 {"Id": "9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0f9e8d",
  "HostConfig": {"NanoCpus": 0, "CpuQuota": 0, "CpuPeriod": 0, "Memory": 0}},
 {"Id": "77aa0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a",
  "HostConfig": {"NanoCpus": 0, "CpuQuota": 150000, "CpuPeriod": 50000, "Memory": 0}},
 {"Id": "88bb0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a",
  "HostConfig": {"NanoCpus": 0, "CpuQuota": 0, "CpuPeriod": 0, "Memory": 0}},
 {"Id": "99cc0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a",
  "HostConfig": {"NanoCpus": 0, "CpuQuota": 0, "CpuPeriod": 0, "Memory": 0}}
]"#;

const STATS: &str = r#"{"BlockIO":"0B / 0B","CPUPerc":"37.50%","Container":"4c1f0a9e7b3d","ID":"4c1f0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a","MemPerc":"25.00%","MemUsage":"256MiB / 1GiB","Name":"shop-web-1","NetIO":"1.2kB / 0B","PIDs":"6"}
{"BlockIO":"0B / 0B","CPUPerc":"212.34%","Container":"9e8d7c6b5a4f","ID":"9e8d7c6b5a4f3e2d1c0b9a8f7e6d5c4b3a2f1e0d9c8b7a6f5e4d3c2b1a0f9e8d","MemPerc":"0.71%","MemUsage":"14.2MiB / 1.922GiB","Name":"shop-api-1","NetIO":"0B / 0B","PIDs":"3"}
{"BlockIO":"0B / 0B","CPUPerc":"150.00%","Container":"77aa0a9e7b3d","ID":"77aa0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a","MemPerc":"1.00%","MemUsage":"20MiB / 1.922GiB","Name":"shop-jobs-1","NetIO":"0B / 0B","PIDs":"3"}
{"BlockIO":"--","CPUPerc":"--","Container":"88bb0a9e7b3d","ID":"88bb0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a","MemPerc":"--","MemUsage":"-- / --","Name":"shop-old-1","NetIO":"--","PIDs":"--"}
{"BlockIO":"0B / 0B","CPUPerc":"0.00%","Container":"99cc0a9e7b3d","ID":"99cc0a9e7b3d5a2c6e8f0b1d3a5c7e9f1b3d5f7a9c0e2a4c6e8b0d2f4a6c8e0a","MemPerc":"0.00%","MemUsage":"0B / 0B","Name":"shop-new-1","NetIO":"0B / 0B","PIDs":"0"}
"#;

#[tokio::test]
async fn the_engines_own_numbers_take_two_commands() -> TestResult {
    let engine = Arc::new(Commands {
        outputs: Mutex::new(VecDeque::from([INSPECTED, STATS])),
        commands: Mutex::new(Vec::new()),
    });
    let runtime = DockerComposeRuntime::new(engine.clone());
    let ids = [
        "4c1f0a9e7b3d",
        "9e8d7c6b5a4f",
        "77aa0a9e7b3d",
        "88bb0a9e7b3d",
        "99cc0a9e7b3d",
    ]
    .map(str::to_owned);
    let samples = runtime.container_usage(&ids).await?;
    let commands = engine.commands.lock().map_err(|_| "poisoned")?.clone();
    assert_eq!(commands.len(), 2);
    assert_eq!(
        commands[0][..4],
        ["container", "inspect", "--format", "json"]
    );
    assert_eq!(commands[0][4..], ids);
    assert_eq!(
        commands[1][..5],
        [
            "stats",
            "--no-stream",
            "--no-trunc",
            "--format",
            "{{json .}}"
        ]
    );
    assert_eq!(commands[1][5..], ids);
    let measured = |share, of| {
        Some(Measured {
            share: percent(share),
            of,
        })
    };
    assert_eq!(
        samples,
        [
            // 37.5% of one core is three quarters of half a core.
            ContainerSample {
                container: "4c1f0a9e7b3d".to_owned(),
                cpu: measured(7_500, Basis::Limit),
                memory: measured(2_500, Basis::Limit),
            },
            ContainerSample {
                container: "9e8d7c6b5a4f".to_owned(),
                cpu: measured(21_234, Basis::OneCore),
                memory: measured(71, Basis::NodeMemory),
            },
            // A quota of 150 ms in every 50 ms: three cores.
            ContainerSample {
                container: "77aa0a9e7b3d".to_owned(),
                cpu: measured(5_000, Basis::Limit),
                memory: measured(100, Basis::NodeMemory),
            },
            // The two containers that do not run have no numbers, in
            // either form the engine prints that: left out.
        ]
    );
    // Nothing to ask about: no command.
    assert!(runtime.container_usage(&[]).await?.is_empty());
    assert_eq!(engine.commands.lock().map_err(|_| "poisoned")?.len(), 2);
    Ok(())
}
