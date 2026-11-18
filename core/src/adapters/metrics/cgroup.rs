//! A container's CPU and memory counters, read from its control group
//! (cgroup v2).
//!
//! The kernel keeps a directory of small files for every container. Five
//! of them are read for a sample: `cpu.stat` (CPU time used), `cpu.max`
//! (the CPU limit), `memory.current`, `memory.stat` (for the file cache
//! that does not count as in use), and `memory.max` (the memory limit).
//! No engine is asked and no process is started.
//!
//! Where a container's directory is depends on the engine's cgroup driver:
//!
//! - `systemd`, Docker's default on a host that runs systemd:
//!   `/sys/fs/cgroup/system.slice/docker-<id>.scope`;
//! - `cgroupfs`: `/sys/fs/cgroup/docker/<id>`;
//! - rootful Podman with `systemd`, its default:
//!   `/sys/fs/cgroup/machine.slice/libpod-<id>.scope`;
//! - rootful Podman with `cgroupfs`: `/sys/fs/cgroup/libpod_parent/libpod-<id>`.
//!
//! All are looked in, so neither the engine nor the driver need be known. A container that
//! is in neither has no counters here, and the caller asks the engine
//! instead. That is the case on a host that still mounts cgroup v1, for a
//! container with a `cgroup_parent` of its own, and for an engine that
//! keeps its containers elsewhere (a rootless one).
//!
//! A control group always counts CPU time. It counts memory only where
//! the memory controller is handed down to it; where it is not (an engine
//! that itself runs in a container can be set up so), a container has CPU
//! samples and no memory samples, and the engine has none either.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use crate::app::ports::UsageCounters;
use crate::domain::metrics::{CpuLimit, UsageReading};

/// A directory that holds one control group per container, and how a
/// container's id is wrapped in its group's name.
struct Place {
    directory: &'static str,
    prefix: &'static str,
    suffix: &'static str,
}

const PLACES: [Place; 4] = [
    Place {
        directory: "system.slice",
        prefix: "docker-",
        suffix: ".scope",
    },
    Place {
        directory: "docker",
        prefix: "",
        suffix: "",
    },
    Place {
        directory: "machine.slice",
        prefix: "libpod-",
        suffix: ".scope",
    },
    Place {
        directory: "libpod_parent",
        prefix: "libpod-",
        suffix: "",
    },
];

/// The engine itself takes the first twelve characters of an id for the
/// container; fewer could name two.
const SHORTEST_ID: usize = 12;

/// The counters of a host's containers.
pub struct CgroupCounters {
    root: PathBuf,
    node_memory: Option<u64>,
    started: Instant,
    /// Where each container's control group was found.
    found: Mutex<HashMap<String, PathBuf>>,
}

impl std::fmt::Debug for CgroupCounters {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CgroupCounters")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl CgroupCounters {
    /// The counters of this host, under `/sys/fs/cgroup`.
    #[must_use]
    pub fn host() -> Self {
        Self::at("/sys/fs/cgroup", host_memory())
    }

    /// The counters under `root`, on a node with `node_memory` bytes of
    /// memory.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>, node_memory: Option<u64>) -> Self {
        Self {
            root: root.into(),
            node_memory,
            started: Instant::now(),
            found: Mutex::new(HashMap::new()),
        }
    }

    /// Looks for the control groups of `missing` in every place an engine
    /// puts them: one directory listing per place, however many
    /// containers are new.
    fn find(&self, missing: &[&String], found: &mut HashMap<String, PathBuf>) {
        for place in &PLACES {
            let Ok(entries) = std::fs::read_dir(self.root.join(place.directory)) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name();
                let Some(id) = name
                    .to_str()
                    .and_then(|name| name.strip_prefix(place.prefix))
                    .and_then(|name| name.strip_suffix(place.suffix))
                else {
                    continue;
                };
                for container in missing {
                    if container.len() >= SHORTEST_ID && id.starts_with(container.as_str()) {
                        found
                            .entry((*container).clone())
                            .or_insert_with(|| entry.path());
                    }
                }
            }
        }
    }

    fn reading(&self, group: &Path, text: &mut String) -> Option<UsageReading> {
        let at = self.started.elapsed();
        let cpu_usec = field(file(group, "cpu.stat", text)?, "usage_usec")?;
        // Without the CPU controller there is no limit to read.
        let cpu_limit = file(group, "cpu.max", text).and_then(cpu_limit);
        // Nor is there memory to read without the memory controller: the
        // engine could not tell either, so the CPU alone is a reading.
        let in_use =
            file(group, "memory.current", text).and_then(|bytes| bytes.parse::<u64>().ok());
        let cache = file(group, "memory.stat", text)
            .and_then(|stat| field(stat, "inactive_file"))
            .unwrap_or_default();
        let memory_limit = file(group, "memory.max", text).and_then(|max| max.parse().ok());
        Some(UsageReading {
            at,
            cpu_usec,
            cpu_limit,
            memory_bytes: in_use.map(|in_use| in_use.saturating_sub(cache)),
            memory_limit,
        })
    }
}

impl UsageCounters for CgroupCounters {
    fn read(&self, containers: &[String]) -> Vec<Option<UsageReading>> {
        // Only a cgroup v2 hierarchy has this file at its root.
        if !self.root.join("cgroup.controllers").is_file() {
            return vec![None; containers.len()];
        }
        let mut found = self.found.lock().unwrap_or_else(PoisonError::into_inner);
        let mut known = HashMap::with_capacity(containers.len());
        for container in containers {
            if let Some(group) = found.remove(container) {
                known.insert(container.clone(), group);
            }
        }
        let missing = containers
            .iter()
            .filter(|container| !known.contains_key(*container))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.find(&missing, &mut known);
        }
        let mut text = String::new();
        let readings = containers
            .iter()
            .map(|container| {
                let reading = self.reading(known.get(container)?, &mut text);
                if reading.is_none() {
                    // The group is gone, or was made again: look once more.
                    known.remove(container);
                }
                reading
            })
            .collect();
        *found = known;
        readings
    }

    fn node_memory(&self) -> Option<u64> {
        self.node_memory
    }
}

/// The trimmed text of one file of a control group.
fn file<'text>(group: &Path, name: &str, text: &'text mut String) -> Option<&'text str> {
    text.clear();
    std::fs::File::open(group.join(name))
        .ok()?
        .read_to_string(text)
        .ok()?;
    Some(text.trim())
}

/// The number after `name` in a file of `name value` lines.
fn field(text: &str, name: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(' ')?;
        (key == name).then(|| value.trim().parse().ok())?
    })
}

/// `cpu.max` holds `<quota> <period>` in microseconds, or `max <period>`
/// for a container without a limit.
fn cpu_limit(text: &str) -> Option<CpuLimit> {
    let (quota, period) = text.split_once(' ')?;
    Some(CpuLimit {
        quota_usec: quota.parse().ok()?,
        period_usec: period.trim().parse().ok()?,
    })
}

/// This host's memory in bytes. `/proc/meminfo` says it; under the
/// daemon's unit that file is hidden (`ProcSubset=pid`), and the same
/// number is under `/sys`, one file per memory node.
fn host_memory() -> Option<u64> {
    let total = |text: &str| {
        text.lines()
            .filter_map(|line| {
                let (_, rest) = line.split_once("MemTotal:")?;
                rest.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()
            })
            .sum::<u64>()
    };
    let mut kilobytes = std::fs::read_to_string("/proc/meminfo")
        .map(|text| total(&text))
        .unwrap_or_default();
    if kilobytes == 0 {
        let nodes = std::fs::read_dir("/sys/devices/system/node").ok()?;
        kilobytes = nodes
            .flatten()
            .filter_map(|node| std::fs::read_to_string(node.path().join("meminfo")).ok())
            .map(|text| total(&text))
            .sum();
    }
    (kilobytes > 0).then(|| kilobytes.saturating_mul(1024))
}
