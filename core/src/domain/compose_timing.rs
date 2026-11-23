//! How long Compose may take to start or stop a project's containers.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use serde_yaml_ng::Value;

/// The shortest process limit any Compose `up` or `down` gets.
pub const COMPOSE_FLOOR: Duration = Duration::from_secs(120);
/// What a Compose command takes beyond its containers' own waits.
pub const COMPOSE_MARGIN: Duration = Duration::from_secs(60);

const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(10);
const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_RETRIES: u32 = 3;

#[derive(Debug, Default)]
struct Service {
    stop_grace: Option<Duration>,
    healthcheck: BTreeMap<String, Value>,
    depends_on: BTreeSet<String>,
}

/// The longest `depends_on` chain of the services the Compose files
/// `models` define, later files overriding earlier ones: each service with
/// its `stop_grace_period` and, when another service waits for it to be
/// healthy, its health check's longest time to decide.
#[must_use]
pub fn compose_wait(models: &[Value]) -> Duration {
    let mut services = BTreeMap::<String, Service>::new();
    let mut awaited_healthy = BTreeSet::new();
    for model in models {
        let Some(defined) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, definition) in defined {
            let Some(name) = name.as_str() else {
                continue;
            };
            let service = services.entry(name.to_owned()).or_default();
            if let Some(grace) = definition.get("stop_grace_period") {
                service.stop_grace = compose_duration(grace);
            }
            if let Some(check) = definition.get("healthcheck").and_then(Value::as_mapping) {
                for (key, value) in check {
                    if let Some(key) = key.as_str() {
                        service.healthcheck.insert(key.to_owned(), value.clone());
                    }
                }
            }
            match definition.get("depends_on") {
                Some(Value::Sequence(names)) => service
                    .depends_on
                    .extend(names.iter().filter_map(Value::as_str).map(str::to_owned)),
                Some(Value::Mapping(entries)) => {
                    for (dependency, condition) in entries {
                        let Some(dependency) = dependency.as_str() else {
                            continue;
                        };
                        service.depends_on.insert(dependency.to_owned());
                        if condition.get("condition").and_then(Value::as_str)
                            == Some("service_healthy")
                        {
                            awaited_healthy.insert(dependency.to_owned());
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut longest = BTreeMap::new();
    services
        .keys()
        .map(|name| {
            chain(
                name,
                &services,
                &awaited_healthy,
                &mut longest,
                &mut Vec::new(),
            )
        })
        .max()
        .unwrap_or_default()
}

/// The process limit for a Compose `up`, `down` or `stop` that waits for
/// `wait`: `wait` and the margin, never less than `floor`.
#[must_use]
pub fn compose_limit(wait: Duration, floor: Duration) -> Duration {
    wait.saturating_add(COMPOSE_MARGIN).max(floor)
}

fn chain(
    name: &str,
    services: &BTreeMap<String, Service>,
    awaited_healthy: &BTreeSet<String>,
    longest: &mut BTreeMap<String, Duration>,
    path: &mut Vec<String>,
) -> Duration {
    if let Some(known) = longest.get(name) {
        return *known;
    }
    let Some(service) = services.get(name) else {
        return Duration::ZERO;
    };
    if path.iter().any(|seen| seen == name) {
        return Duration::ZERO;
    }
    path.push(name.to_owned());
    let below = service
        .depends_on
        .iter()
        .map(|dependency| chain(dependency, services, awaited_healthy, longest, path))
        .max()
        .unwrap_or_default();
    path.pop();
    let mut own = service.stop_grace.unwrap_or(DEFAULT_STOP_GRACE);
    if awaited_healthy.contains(name) {
        own = own.saturating_add(health_decision(&service.healthcheck));
    }
    let total = below.saturating_add(own);
    longest.insert(name.to_owned(), total);
    total
}

/// The longest a health check takes to call its container healthy or
/// unhealthy: its start period, then every retry at its interval and
/// timeout.
fn health_decision(check: &BTreeMap<String, Value>) -> Duration {
    if check.get("disable").and_then(Value::as_bool) == Some(true)
        || check
            .get("test")
            .and_then(Value::as_sequence)
            .and_then(|test| test.first())
            .and_then(Value::as_str)
            == Some("NONE")
    {
        return Duration::ZERO;
    }
    let duration =
        |key: &str, default: Duration| check.get(key).and_then(compose_duration).unwrap_or(default);
    let retries = check
        .get("retries")
        .and_then(Value::as_u64)
        .and_then(|retries| u32::try_from(retries).ok())
        .unwrap_or(DEFAULT_RETRIES);
    duration("start_period", Duration::ZERO).saturating_add(
        duration("interval", DEFAULT_INTERVAL)
            .saturating_add(duration("timeout", DEFAULT_TIMEOUT))
            .saturating_mul(retries),
    )
}

/// A Compose duration: `90s`, `1m30s`, `1.5h`, `500ms`, or a number of
/// seconds.
#[must_use]
pub fn compose_duration(value: &Value) -> Option<Duration> {
    if let Some(seconds) = value.as_u64() {
        return Some(Duration::from_secs(seconds));
    }
    let mut text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    let mut total = 0.0_f64;
    while !text.is_empty() {
        let number_end = text
            .find(|character: char| !(character.is_ascii_digit() || character == '.'))
            .unwrap_or(text.len());
        let amount = text[..number_end].parse::<f64>().ok()?;
        text = &text[number_end..];
        let unit_end = text
            .find(|character: char| character.is_ascii_digit() || character == '.')
            .unwrap_or(text.len());
        let seconds = match &text[..unit_end] {
            "ns" => 1e-9,
            "us" | "µs" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            "m" => 60.0,
            "h" => 3600.0,
            _ => return None,
        };
        text = &text[unit_end..];
        total += amount * seconds;
    }
    Duration::try_from_secs_f64(total).ok()
}
