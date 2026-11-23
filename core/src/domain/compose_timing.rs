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

/// How the Compose provider stops and starts a project's services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeOrder {
    /// Docker Compose: along `depends_on`, services that do not depend on
    /// each other at the same time.
    Dependencies,
    /// Any provider, such as the Python `podman-compose`, which takes one
    /// service after the other.
    OneByOne,
}

#[derive(Debug, Default)]
struct Service {
    stop_grace: Option<Duration>,
    healthcheck: BTreeMap<String, Value>,
    depends_on: BTreeSet<String>,
}

type Variables<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What Compose waits for while it starts or stops the services the
/// Compose files `models` define, later files overriding earlier ones:
/// each service's `stop_grace_period` and, when another service waits for
/// it to be healthy, its health check's longest time to decide. With
/// [`ComposeOrder::Dependencies`] that is the longest `depends_on` chain,
/// with [`ComposeOrder::OneByOne`] the sum. `variables` answers the
/// `${NAME}` the values use; an unknown one takes its `:-` default.
#[must_use]
pub fn compose_wait(models: &[Value], variables: Variables<'_>, order: ComposeOrder) -> Duration {
    let (services, awaited_healthy) = merged_services(models, variables);
    let own = |name: &str, service: &Service| {
        let grace = service.stop_grace.unwrap_or(DEFAULT_STOP_GRACE);
        if awaited_healthy.contains(name) {
            grace.saturating_add(health_decision(&service.healthcheck, variables))
        } else {
            grace
        }
    };
    if order == ComposeOrder::OneByOne {
        return services
            .iter()
            .fold(Duration::ZERO, |sum, (name, service)| {
                sum.saturating_add(own(name, service))
            });
    }
    let mut longest = BTreeMap::new();
    services
        .keys()
        .map(|name| chain(name, &services, &own, &mut longest, &mut Vec::new()))
        .max()
        .unwrap_or_default()
}

fn merged_services(
    models: &[Value],
    variables: Variables<'_>,
) -> (BTreeMap<String, Service>, BTreeSet<String>) {
    let mut services = BTreeMap::<String, Service>::new();
    let mut awaited_healthy = BTreeSet::new();
    for model in models {
        let mut model = model.clone();
        if model.apply_merge().is_err() {
            continue;
        }
        if let Some(root) = model.as_mapping_mut() {
            crate::domain::compose_extends::resolve_extends(root);
        }
        let Some(defined) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, definition) in defined {
            let Some(name) = name.as_str() else {
                continue;
            };
            let service = services.entry(name.to_owned()).or_default();
            if let Some(grace) = definition.get("stop_grace_period") {
                service.stop_grace = interpolated(grace, variables)
                    .as_ref()
                    .and_then(compose_duration);
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
    (services, awaited_healthy)
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
    own: &dyn Fn(&str, &Service) -> Duration,
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
        .map(|dependency| chain(dependency, services, own, longest, path))
        .max()
        .unwrap_or_default();
    path.pop();
    let total = below.saturating_add(own(name, service));
    longest.insert(name.to_owned(), total);
    total
}

/// The longest a health check takes to call its container healthy or
/// unhealthy: its start period, then every retry at its interval and
/// timeout.
fn health_decision(check: &BTreeMap<String, Value>, variables: Variables<'_>) -> Duration {
    let value = |key: &str| {
        check
            .get(key)
            .and_then(|value| interpolated(value, variables))
    };
    let disabled = value("disable")
        .is_some_and(|disable| disable.as_bool() == Some(true) || disable.as_str() == Some("true"));
    let none = check
        .get("test")
        .and_then(Value::as_sequence)
        .and_then(|test| test.first())
        .and_then(Value::as_str)
        == Some("NONE");
    if disabled || none {
        return Duration::ZERO;
    }
    let duration = |key: &str, default: Duration| {
        value(key)
            .as_ref()
            .and_then(compose_duration)
            .unwrap_or(default)
    };
    let retries = value("retries")
        .and_then(|retries| {
            retries
                .as_u64()
                .or_else(|| retries.as_str().and_then(|text| text.trim().parse().ok()))
        })
        .and_then(|retries| u32::try_from(retries).ok())
        .unwrap_or(DEFAULT_RETRIES);
    duration("start_period", Duration::ZERO).saturating_add(
        duration("interval", DEFAULT_INTERVAL)
            .saturating_add(duration("timeout", DEFAULT_TIMEOUT))
            .saturating_mul(retries),
    )
}

/// `value` with the variables of a string interpolated as Compose does.
fn interpolated(value: &Value, variables: Variables<'_>) -> Option<Value> {
    match value {
        Value::String(text) => interpolate(text, variables).map(Value::String),
        other => Some(other.clone()),
    }
}

/// `text` with `$NAME`, `${NAME}`, `${NAME:-default}`, `${NAME-default}`,
/// `${NAME:+other}`, `${NAME+other}` and `$$` replaced. `None` for a
/// required variable (`:?`, `?`) that is unset, or text Compose refuses.
#[must_use]
pub fn interpolate(text: &str, variables: Variables<'_>) -> Option<String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(after) = rest.strip_prefix('$') {
            out.push('$');
            rest = after;
        } else if let Some(body) = rest.strip_prefix('{') {
            let end = closing_brace(body)?;
            out.push_str(&expand(&body[..end], variables)?);
            rest = &body[end + 1..];
        } else {
            let end = rest
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .unwrap_or(rest.len());
            if end == 0 {
                return None;
            }
            out.push_str(&variables(&rest[..end]).unwrap_or_default());
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    Some(out)
}

fn closing_brace(body: &str) -> Option<usize> {
    let mut depth = 0_usize;
    for (index, character) in body.char_indices() {
        match character {
            '{' => depth += 1,
            '}' if depth == 0 => return Some(index),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn expand(body: &str, variables: Variables<'_>) -> Option<String> {
    let split = body
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(body.len());
    let (name, modifier) = body.split_at(split);
    if name.is_empty() {
        return None;
    }
    let value = variables(name);
    let set = value.is_some();
    let non_empty = value.as_deref().is_some_and(|value| !value.is_empty());
    let word = |prefix: usize| interpolate(&modifier[prefix..], variables);
    match modifier.get(..2).unwrap_or(modifier) {
        "" => Some(value.unwrap_or_default()),
        ":-" => {
            if non_empty {
                value
            } else {
                word(2)
            }
        }
        ":+" => {
            if non_empty {
                word(2)
            } else {
                Some(String::new())
            }
        }
        ":?" => value.filter(|_| non_empty),
        _ => match modifier.get(..1) {
            Some("-") => {
                if set {
                    value
                } else {
                    word(1)
                }
            }
            Some("+") => {
                if set {
                    word(1)
                } else {
                    Some(String::new())
                }
            }
            Some("?") => value,
            _ => None,
        },
    }
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
