//! A container whose health check keeps failing, restarted by its node.
//!
//! On a cluster target every service gets this without a line of
//! configuration; `health:` next to a service's `placement` changes it:
//!
//! ```yaml
//! health:
//!   on-failure: restart   # restart (the default) | none
//!   after: 3m             # how long it stays unhealthy first
//!   max-restarts: 5       # within `window`, then stop and report
//!   window: 1h
//! ```
//!
//! Each node's `mmd` looks at its own containers. One that has failed its
//! health check for `after` is restarted the way `mm service restart` does
//! it, one container of a service at a time. Past `max-restarts` within
//! `window` the node stops restarting it and keeps it out of the answers
//! while the service has another container to answer with. A container
//! without a health check is never unhealthy, so it is never restarted.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::DurationSpec;

/// The code `mm service status`, `mm doctor` and the audit journal give a
/// container mm stopped restarting.
pub const RESTART_LIMIT: &str = "MM_SERVICE_RESTART_LIMIT";
/// The code of a `health:` setting mm cannot use.
pub const INVALID: &str = "MM_HEALTH";
pub const DEFAULT_AFTER: Duration = Duration::from_secs(3 * 60);
pub const DEFAULT_MAX_RESTARTS: u32 = 5;
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(60 * 60);
/// The shortest `after`: a health check needs a few runs to fail.
pub const SHORTEST_AFTER: Duration = Duration::from_secs(10);

/// `modules.<m>.services.<s>.health`. Every key has a default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct HealthConfig {
    /// `restart` (the default on a cluster target) restarts a container
    /// that stays unhealthy; `none` leaves it as it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<OnFailure>,
    /// How long a container stays unhealthy before it is restarted, such as
    /// `3m` (the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<DurationSpec>,
    /// How many restarts of one container within `window` before its node
    /// stops restarting it; default 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_restarts: Option<u32>,
    /// Default `1h`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window: Option<DurationSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OnFailure {
    Restart,
    None,
}

/// What a node does with a service's unhealthy containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RestartPolicy {
    pub after_ms: u64,
    pub max_restarts: u32,
    pub window_ms: u64,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            after_ms: millis(DEFAULT_AFTER),
            max_restarts: DEFAULT_MAX_RESTARTS,
            window_ms: millis(DEFAULT_WINDOW),
        }
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl RestartPolicy {
    /// The policy `health` says, with every default filled in: `None` for
    /// `on-failure: none`. Each problem names its key under `entry`.
    pub fn resolve(
        config: Option<&HealthConfig>,
        entry: &str,
    ) -> Result<Option<Self>, Vec<String>> {
        let Some(config) = config else {
            return Ok(Some(Self::default()));
        };
        if config.on_failure == Some(OnFailure::None) {
            return Ok(None);
        }
        let mut problems = Vec::new();
        let mut duration = |key: &str, spec: Option<&DurationSpec>, default: Duration| match spec {
            None => default,
            Some(spec) => match spec.parse() {
                Ok(duration) if !duration.is_zero() => duration,
                Ok(_) => {
                    problems.push(format!(
                        "{entry}.health.{key} is 0; write a time such as 3m"
                    ));
                    default
                }
                Err(problem) => {
                    problems.push(format!("{entry}.health.{key}: {problem}"));
                    default
                }
            },
        };
        let after = duration("after", config.after.as_ref(), DEFAULT_AFTER);
        let window = duration("window", config.window.as_ref(), DEFAULT_WINDOW);
        if after < SHORTEST_AFTER && config.after.is_some() {
            problems.push(format!(
                "{entry}.health.after is shorter than {}s, too short for a health check to fail several times; write 10s or more",
                SHORTEST_AFTER.as_secs()
            ));
        }
        let max_restarts = config.max_restarts.unwrap_or(DEFAULT_MAX_RESTARTS);
        if max_restarts == 0 {
            problems.push(format!(
                "{entry}.health.max-restarts is 0; write at least 1, or `on-failure: none` to never restart"
            ));
        }
        if problems.is_empty() {
            Ok(Some(Self {
                after_ms: millis(after),
                max_restarts,
                window_ms: millis(window),
            }))
        } else {
            Err(problems)
        }
    }

    /// What one look at an unhealthy container decides: `unhealthy_ms` is
    /// how long it has failed its check, `restarts` when it was restarted
    /// before, `now` the time of the look (milliseconds since the epoch).
    #[must_use]
    pub fn step(&self, unhealthy_ms: u64, restarts: &[i64], now: i64) -> HealthStep {
        if unhealthy_ms < self.after_ms {
            return HealthStep::Wait;
        }
        let unhealthy = spoken(unhealthy_ms);
        let recent = self.recent(restarts, now);
        if recent >= self.max_restarts {
            return HealthStep::Limit {
                reason: format!(
                    "unhealthy for {unhealthy} after {recent} restarts within {}",
                    spoken(self.window_ms)
                ),
            };
        }
        HealthStep::Restart {
            reason: format!("unhealthy for {unhealthy}"),
        }
    }

    /// How many of `restarts` fall within the window that ends at `now`.
    #[must_use]
    pub fn recent(&self, restarts: &[i64], now: i64) -> u32 {
        let since = now.saturating_sub(i64::try_from(self.window_ms).unwrap_or(i64::MAX));
        u32::try_from(restarts.iter().filter(|at| **at > since).count()).unwrap_or(u32::MAX)
    }

    /// `restart after 3m unhealthy, at most 5 times in 1h`
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "restart after {} unhealthy, at most {} times in {}",
            spoken(self.after_ms),
            self.max_restarts,
            spoken(self.window_ms)
        )
    }
}

/// What a node does with one unhealthy container now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStep {
    /// Not unhealthy for long enough yet.
    Wait,
    Restart {
        reason: String,
    },
    /// Past `max-restarts` within `window`: leave it, keep it out of the
    /// answers, and report it.
    Limit {
        reason: String,
    },
}

/// `3m`, `1h`, `2m30s`: a time as a configuration writes it.
#[must_use]
pub fn spoken(millis: u64) -> String {
    let seconds = millis / 1_000;
    if seconds == 0 {
        return format!("{millis}ms");
    }
    [
        (seconds / 3_600, "h"),
        (seconds % 3_600 / 60, "m"),
        (seconds % 60, "s"),
    ]
    .into_iter()
    .filter(|(amount, _)| *amount > 0)
    .map(|(amount, unit)| format!("{amount}{unit}"))
    .collect()
}

/// A service of a node's plan that sets `health:`; every other service of
/// the node gets [`RestartPolicy::default`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceHealth {
    pub stack: String,
    pub service: String,
    /// `None` for `on-failure: none`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<RestartPolicy>,
}

/// One restart mm made, or the moment it stopped restarting a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RestartEvent {
    /// The node the container runs on; set once nodes are put together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The engine's name of the container.
    pub container: String,
    pub at_unix_ms: i64,
    /// `unhealthy for 3m`
    pub reason: String,
}

/// What mm did about a service's unhealthy containers: part of the
/// service's status, for `mm service status`, the terminal UI and MCP.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ServiceRestarts {
    /// The restarts within the service's `window`, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restarts: Vec<RestartEvent>,
    /// The containers mm stopped restarting at the service's limit
    /// ([`RESTART_LIMIT`]): they get no requests while another container
    /// of the service can take them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limited: Vec<RestartEvent>,
}

impl ServiceRestarts {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.restarts.is_empty() && self.limited.is_empty()
    }

    /// Two nodes' answers as one.
    #[must_use]
    pub fn merged(mut self, other: Self) -> Self {
        self.restarts.extend(other.restarts);
        self.limited.extend(other.limited);
        self.restarts.sort_by_key(|event| event.at_unix_ms);
        self.limited.sort_by_key(|event| event.at_unix_ms);
        self
    }
}

/// The line `mm service logs` shows for a restart: `mm: restarted web-1
/// after unhealthy for 3m`.
#[must_use]
pub fn log_line(event: &RestartEvent, limit: bool) -> String {
    if limit {
        format!(
            "mm: stopped restarting {}, {} ({RESTART_LIMIT})",
            event.container, event.reason
        )
    } else {
        format!(
            "mm: restarted {} after it was {}",
            event.container, event.reason
        )
    }
}

/// `mm doctor`: each container a node stopped restarting.
#[must_use]
pub fn restart_limit_diagnostics(
    services: &[(&str, &ServiceRestarts)],
) -> Vec<crate::domain::diagnostic::Diagnostic> {
    services
        .iter()
        .flat_map(|(service, restarts)| {
            restarts.limited.iter().map(move |event| {
                crate::domain::diagnostic::Diagnostic::warning(
                    RESTART_LIMIT,
                    format!(
                        "mm stopped restarting container {}{} of service '{service}': {}. It gets no requests while another container of '{service}' can take them. `mm service logs {service}` shows why it fails its health check; fix the service or its check and deploy, or run `mm service restart {service}` to try again",
                        event.container,
                        event
                            .node
                            .as_deref()
                            .map(|node| format!(" on {node}"))
                            .unwrap_or_default(),
                        event.reason
                    ),
                )
            })
        })
        .collect()
}
