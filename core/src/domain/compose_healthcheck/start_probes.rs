//! Health checks that probe quickly while a container starts.
//!
//! Docker Engine 25 and later (API 1.44) run a service's health check every
//! `start_interval` during its `start_period`, and every `interval` after.
//! A deployment waits for the first healthy probe, so mm's overlay gives each
//! service with a health check `start_interval: 1s`, and a `start_period` of
//! `interval` times `retries` when the service sets none. Once a container
//! is healthy, or the period is over, the service's own `interval` applies.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_yaml_ng::{Mapping, Value};

/// The first Docker Engine major release that honours `start_interval`.
pub const START_INTERVAL_DOCKER_MAJOR: u64 = 25;

/// How often a starting container is probed.
pub const START_INTERVAL: &str = "1s";

const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_RETRIES: u64 = 3;

/// Whether a Docker Engine of `version` (as it reports it, `27.3.1`)
/// probes at `start_interval`.
#[must_use]
pub fn docker_probes_at_start(version: &str) -> bool {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|major| major.parse::<u64>().ok())
        .is_some_and(|major| major >= START_INTERVAL_DOCKER_MAJOR)
}

/// The `healthcheck` keys to add to each service of a stack whose Compose
/// files (`models`, in the order Compose merges them) give it a health
/// check without `start_interval`.
#[must_use]
pub fn start_probes(models: &[Value]) -> BTreeMap<String, Mapping> {
    let mut checks = BTreeMap::<String, Mapping>::new();
    for model in models {
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in services {
            let (Some(name), Some(check)) = (
                name.as_str(),
                service.get("healthcheck").and_then(Value::as_mapping),
            ) else {
                continue;
            };
            let merged = checks.entry(name.to_owned()).or_default();
            for (key, value) in check {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    checks
        .into_iter()
        .filter_map(|(service, check)| Some((service, added(&check)?)))
        .collect()
}

fn added(check: &Mapping) -> Option<Mapping> {
    let get = |key: &str| check.get(Value::String(key.to_owned()));
    let disabled = get("disable").and_then(Value::as_bool) == Some(true);
    let none = match get("test") {
        Some(Value::String(test)) => test.trim() == "NONE",
        Some(Value::Sequence(test)) => test.first().and_then(Value::as_str) == Some("NONE"),
        Some(_) => false,
        None => true,
    };
    if disabled || none || get("start_interval").is_some() {
        return None;
    }
    let mut keys = Mapping::new();
    if get("start_period").is_none() {
        let interval = match get("interval") {
            None => DEFAULT_INTERVAL,
            Some(value) => compose_duration(value.as_str()?)?,
        };
        let retries = match get("retries") {
            None => DEFAULT_RETRIES,
            Some(value) => value.as_u64()?,
        };
        let period = interval.checked_mul(u32::try_from(retries.max(1)).ok()?)?;
        keys.insert(
            Value::String("start_period".to_owned()),
            Value::String(format!("{}s", period.as_secs().max(1))),
        );
    }
    keys.insert(
        Value::String("start_interval".to_owned()),
        Value::String(START_INTERVAL.to_owned()),
    );
    Some(keys)
}

/// A Compose duration such as `30s`, `1m30s`, `500ms` or `1h`.
fn compose_duration(text: &str) -> Option<Duration> {
    let mut total = Duration::ZERO;
    let mut rest = text.trim();
    if rest.is_empty() {
        return None;
    }
    while !rest.is_empty() {
        let digits =
            rest.find(|character: char| !character.is_ascii_digit() && character != '.')?;
        let (number, tail) = rest.split_at(digits);
        let unit_end = tail
            .find(|character: char| character.is_ascii_digit())
            .unwrap_or(tail.len());
        let (unit, next) = tail.split_at(unit_end);
        let value = number.parse::<f64>().ok()?;
        let seconds = match unit {
            "h" => value * 3600.0,
            "m" => value * 60.0,
            "s" => value,
            "ms" => value / 1000.0,
            "us" | "µs" => value / 1_000_000.0,
            _ => return None,
        };
        total = total.checked_add(Duration::try_from_secs_f64(seconds).ok()?)?;
        rest = next;
    }
    Some(total)
}
