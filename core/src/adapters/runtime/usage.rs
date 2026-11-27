//! What containers use, as the engine itself reports it, for a node's
//! metrics when the host shows no control group for a container.
//!
//! Two engine commands: one `inspect` for the limits and one
//! `stats --no-stream`, which takes the engine more than a second. The
//! node's sampler reads the kernel's files instead wherever it can.

use std::time::Duration;

use serde_json::Value;

use crate::adapters::runtime::project_services::ContainerEngine;
use crate::domain::error::MmError;
use crate::domain::metrics::{Basis, ContainerSample, Measured, Percent};

const TIMEOUT: Duration = Duration::from_secs(10);

/// What each of `containers` uses now: CPU as a share of its limit, or of
/// one core, and memory as a share of its limit, or of the host's memory.
/// A container the engine prints no numbers for is left out.
pub(crate) async fn container_usage(
    engine: &dyn ContainerEngine,
    containers: &[String],
) -> Result<Vec<ContainerSample>, MmError> {
    if containers.is_empty() {
        return Ok(Vec::new());
    }
    let inspected =
        crate::adapters::runtime::project_services::inspect_containers(engine, containers, TIMEOUT)
            .await?;
    let limits = parse_limits(inspected.trim(), engine.engine_name())?;
    let mut arguments = [
        "stats",
        "--no-stream",
        "--no-trunc",
        "--format",
        "{{json .}}",
    ]
    .map(str::to_owned)
    .to_vec();
    arguments.extend(containers.iter().cloned());
    let stats = engine.engine(arguments, TIMEOUT).await?;
    let stats = parse_stats(stats.stdout.trim(), engine.engine_name())?;
    Ok(containers
        .iter()
        .filter_map(|container| {
            let of = |id: &str| id.starts_with(container.as_str());
            let stats = stats.iter().find(|stats| of(&stats.id))?;
            let limits = limits.iter().find(|limits| of(&limits.id));
            Some(sample(container, stats, limits))
        })
        .filter(|sample| sample.cpu.is_some() || sample.memory.is_some())
        .collect())
}

/// A container's limits as the engine holds them.
struct Limits {
    id: String,
    /// The CPU limit in billionths of a core; zero for none.
    nano_cpus: u64,
    memory: u64,
}

/// One line of the engine's `stats`.
struct Stats {
    id: String,
    /// In hundredths of a percent of one core.
    cpu: Option<u64>,
    /// In hundredths of a percent of the limit, or of the host's memory.
    memory: Option<u64>,
}

fn sample(container: &str, stats: &Stats, limits: Option<&Limits>) -> ContainerSample {
    let nano_cpus = limits.map_or(0, |limits| limits.nano_cpus);
    let cpu = stats.cpu.map(|of_one_core| {
        if nano_cpus == 0 {
            return Measured {
                share: Percent::from_hundredths(of_one_core),
                of: Basis::OneCore,
            };
        }
        let of_limit = u128::from(of_one_core) * 1_000_000_000 / u128::from(nano_cpus);
        Measured {
            share: Percent::from_hundredths(u64::try_from(of_limit).unwrap_or(u64::MAX)),
            of: Basis::Limit,
        }
    });
    let memory = stats.memory.map(|share| Measured {
        share: Percent::from_hundredths(share),
        of: if limits.is_some_and(|limits| limits.memory > 0) {
            Basis::Limit
        } else {
            Basis::NodeMemory
        },
    });
    ContainerSample {
        container: container.to_owned(),
        cpu,
        memory,
    }
}

fn documents(output: &str, engine: &str, what: &str) -> Result<Vec<Value>, MmError> {
    serde_json::Deserializer::from_str(output)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| MmError::RuntimeResponse {
            message: format!("{engine} returned invalid container {what} JSON: {error}"),
        })
}

/// Reads the engine's inspect output: one JSON array, or one array per
/// line. A limit is `--cpus` (`NanoCpus`) or a quota in a period.
fn parse_limits(output: &str, engine: &str) -> Result<Vec<Limits>, MmError> {
    let limits = |row: &Value| {
        let host = &row["HostConfig"];
        let number = |key: &str| host[key].as_u64().unwrap_or_default();
        let quota = number("CpuQuota");
        let period = match number("CpuPeriod") {
            0 => 100_000,
            period => period,
        };
        Some(Limits {
            id: row["Id"].as_str()?.to_owned(),
            nano_cpus: match number("NanoCpus") {
                0 => quota.saturating_mul(1_000_000_000) / period,
                nano_cpus => nano_cpus,
            },
            memory: number("Memory"),
        })
    };
    Ok(documents(output, engine, "inspect")?
        .iter()
        .flat_map(|document| match document {
            Value::Array(rows) => rows.iter().filter_map(limits).collect::<Vec<_>>(),
            row => limits(row).into_iter().collect(),
        })
        .collect())
}

/// Reads the engine's stats output: one JSON object per line. Docker
/// prints `ID`, `CPUPerc` and `MemPerc` with percentages as text such as
/// `12.50%`; Podman prints `ContainerID`, `CPU` and `MemPerc` as numbers.
/// A container that does not run has no numbers: the engine prints `--`
/// for it, or zeros and no process, and neither is a sample.
fn parse_stats(output: &str, engine: &str) -> Result<Vec<Stats>, MmError> {
    let hundredths = |value: &Value| {
        let percent = match value {
            Value::Number(number) => number.as_f64()?,
            text => match text.as_str()?.trim().strip_suffix('%')?.parse::<f64>() {
                Ok(percent) => percent,
                // `--`: the container does not run.
                Err(_) => return None,
            },
        };
        (percent.is_finite() && percent >= 0.0).then(|| (percent * 100.0).round() as u64)
    };
    Ok(documents(output, engine, "stats")?
        .iter()
        .filter_map(|row| {
            let runs = match &row["PIDs"] {
                Value::Number(processes) => processes.as_u64() != Some(0),
                processes => processes.as_str().is_none_or(|processes| processes != "0"),
            };
            let id = row["ID"].as_str().or_else(|| row["ContainerID"].as_str())?;
            let cpu = if row["CPUPerc"].is_null() {
                &row["CPU"]
            } else {
                &row["CPUPerc"]
            };
            Some(Stats {
                id: id.to_owned(),
                cpu: hundredths(cpu).filter(|_| runs),
                memory: hundredths(&row["MemPerc"]).filter(|_| runs),
            })
        })
        .collect())
}
