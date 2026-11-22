//! How a deployment replaces what runs: `update:` on a service and
//! `deploy:` on an environment. Every key is optional and has a default
//! that works: stop the old container first, one node at a time, wait ten
//! minutes for health, and roll back on failure.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::DurationSpec;

/// How long a stack may take on a node to become healthy when the
/// environment sets no `deploy.convergence-timeout`.
pub const DEFAULT_CONVERGENCE_TIMEOUT: Duration = Duration::from_secs(600);

/// How long a client that deploys waits for a sign of progress from a
/// host when the environment sets no `deploy.progress-timeout`.
pub const DEFAULT_PROGRESS_TIMEOUT: Duration = Duration::from_secs(120);

/// How long a host may take to pull the images a stack or a job needs
/// when the environment sets no `deploy.pull-timeout`.
pub const DEFAULT_PULL_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// The shortest `deploy.progress-timeout`: a host is asked how it is doing
/// every 15 seconds, and one late answer is not yet a stuck host.
pub const MIN_PROGRESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Whether a service's new container starts before or after the old one
/// stops.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateOrder {
    /// The engine stops the old container, then starts the new one.
    #[default]
    StopFirst,
    /// The new container starts next to the old one, and the old one stops
    /// once the new one is healthy and the other nodes resolve it. Not for
    /// a service that publishes a host port or sets `container_name`.
    StartFirst,
}

/// What a deployment does when this service does not become healthy.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateFailure {
    /// Every node the deployment changed returns to what it ran before.
    #[default]
    Rollback,
    /// The deployment stops where it is and changes nothing more, so the
    /// failed container can be looked at. `mm deploy rollback` returns the
    /// nodes to what they ran before.
    Pause,
}

/// `modules.<module>.services.<service>.update`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct UpdateConfig {
    /// `stop-first` (the default) or `start-first`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<UpdateOrder>,
    /// How many nodes of a cluster target replace the service's stack at
    /// the same time. The default is 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub parallelism: Option<u32>,
    /// `rollback` (the default) or `pause`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<UpdateFailure>,
}

/// `environments.<environment>.deploy`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct DeployConfig {
    /// How long a stack may take on a host to become healthy before the
    /// deployment counts as failed, such as `90s` or `15m`. The default is
    /// `10m`. The plan shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convergence_timeout: Option<DurationSpec>,
    /// How long `mm deploy apply` waits without any sign of progress from
    /// a host before it stops with a timeout that names the host, the step
    /// and the operation, such as `5m`. The default is `2m`; at least `30s`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_timeout: Option<DurationSpec>,
    /// How long a host may take to pull the images of a stack, or of a
    /// job, before anything of it starts, such as `45m`. The default is
    /// `20m`; at least `1s`. Starting the containers keeps its own limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_timeout: Option<DurationSpec>,
    /// The releases of mm that may plan deployments here, and that the
    /// hosts' mmd must be: `0.9` is any 0.9 release, `=0.9.3` exactly that
    /// one. A plan from another mm, or for a host that runs another mmd,
    /// is refused, and `mm doctor` reports both. Not set: any mm that
    /// speaks the hosts' protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mm_version: Option<String>,
}

impl DeployConfig {
    /// The configured window, or the default.
    pub fn convergence_timeout(config: Option<&Self>) -> Result<Duration, String> {
        match config.and_then(|deploy| deploy.convergence_timeout.as_ref()) {
            None => Ok(DEFAULT_CONVERGENCE_TIMEOUT),
            Some(spec) => {
                let timeout = spec.parse()?;
                if timeout < Duration::from_secs(1) {
                    return Err("must be at least 1s".to_owned());
                }
                Ok(timeout)
            }
        }
    }

    /// The configured time to pull images, or the default.
    pub fn pull_timeout(config: Option<&Self>) -> Result<Duration, String> {
        match config.and_then(|deploy| deploy.pull_timeout.as_ref()) {
            None => Ok(DEFAULT_PULL_TIMEOUT),
            Some(spec) => {
                let timeout = spec.parse()?;
                if timeout < Duration::from_secs(1) {
                    return Err("must be at least 1s".to_owned());
                }
                Ok(timeout)
            }
        }
    }

    /// The configured time without progress, or the default.
    pub fn progress_timeout(config: Option<&Self>) -> Result<Duration, String> {
        match config.and_then(|deploy| deploy.progress_timeout.as_ref()) {
            None => Ok(DEFAULT_PROGRESS_TIMEOUT),
            Some(spec) => {
                let timeout = spec.parse()?;
                if timeout < MIN_PROGRESS_TIMEOUT {
                    return Err(format!(
                        "must be at least {}s",
                        MIN_PROGRESS_TIMEOUT.as_secs()
                    ));
                }
                Ok(timeout)
            }
        }
    }
}
