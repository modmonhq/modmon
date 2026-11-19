//! Jobs and schedules of `mm.yaml`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct JobConfig {
    pub module: String,
    pub service: String,
    pub command: Vec<String>,
    /// The environments the job belongs to; every environment when left
    /// out. `environment:` with one name is read the same way.
    #[serde(
        default,
        alias = "environment",
        skip_serializing_if = "Option::is_none"
    )]
    pub environments: Option<JobEnvironments>,
    /// When a deployment runs the job by itself: `before-deploy` (before
    /// its service is started or replaced, after what the service depends
    /// on is healthy), `after-deploy` (once the deployment converged), or
    /// `once` (the first time the environment is deployed with it). Left
    /// out, the job runs only when asked (`mm job run`) or scheduled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<JobWhen>,
    #[serde(default = "default_job_timeout")]
    pub timeout: DurationSpec,
    #[serde(default)]
    pub retry: RetryPolicyConfig,
    #[serde(default = "default_job_output_bytes")]
    pub output_limit_bytes: u32,
    #[serde(default = "default_job_history_entries")]
    pub history_limit: u32,
    /// On a cluster target, the node the job runs on: a node name, or a
    /// group whose first node that runs the service is taken. It must run
    /// the job's service. Without it, the first node that does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<String>,
    /// Who the command runs as in the container (`uid`, `uid:gid`, or a
    /// name the image knows), in place of the service's own user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// `on` (the default), `optional` or `off` in the development
    /// environment. A job of a service that is off is off too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<crate::domain::development_units::Development>,
    /// Never read from a file. `mmd` on a cluster node sets it to the node
    /// that runs this job when that is another node.
    #[serde(skip)]
    #[schemars(skip)]
    pub placed_elsewhere: Option<String>,
    /// Never read from a file: set when the job does not run in the
    /// development environment on this machine.
    #[serde(skip)]
    #[schemars(skip)]
    pub off_locally: Option<crate::domain::development_units::OffLocally>,
}

/// When a deployment runs a job.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum JobWhen {
    BeforeDeploy,
    AfterDeploy,
    Once,
}

impl JobWhen {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforeDeploy => "before-deploy",
            Self::AfterDeploy => "after-deploy",
            Self::Once => "once",
        }
    }
}

/// One environment name, or a list of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum JobEnvironments {
    One(String),
    Several(Vec<String>),
}

impl JobEnvironments {
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        match self {
            Self::One(name) => vec![name.as_str()],
            Self::Several(names) => names.iter().map(String::as_str).collect(),
        }
    }
}

impl JobConfig {
    /// Whether the job belongs to `environment`.
    #[must_use]
    pub fn runs_in(&self, environment: &str) -> bool {
        self.environments
            .as_ref()
            .is_none_or(|names| names.names().contains(&environment))
            && self
                .off_locally
                .as_ref()
                .is_none_or(|off| off.environment != environment)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct ScheduleConfig {
    pub job: String,
    pub cron: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "super::default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(default = "default_zero_duration")]
    pub jitter: DurationSpec,
    #[serde(default)]
    pub concurrency: ScheduleConcurrency,
    #[serde(default)]
    pub misfire: MisfirePolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicyConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<DurationSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct DurationSpec(pub String);

impl DurationSpec {
    pub fn parse(&self) -> Result<std::time::Duration, String> {
        crate::domain::scheduler::parse_duration(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct RetryPolicyConfig {
    #[serde(default = "default_retry_attempts")]
    pub attempts: u16,
    #[serde(default = "default_retry_backoff")]
    pub backoff: DurationSpec,
    #[serde(default = "default_retry_max_backoff")]
    pub max_backoff: DurationSpec,
}

impl Default for RetryPolicyConfig {
    fn default() -> Self {
        Self {
            attempts: default_retry_attempts(),
            backoff: default_retry_backoff(),
            max_backoff: default_retry_max_backoff(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleConcurrency {
    Allow,
    #[default]
    Forbid,
    Replace,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MisfirePolicy {
    Skip,
    #[default]
    RunOnce,
    BoundedCatchUp {
        maximum: u16,
    },
}

fn default_job_timeout() -> DurationSpec {
    DurationSpec("10m".to_owned())
}

fn default_zero_duration() -> DurationSpec {
    DurationSpec("0s".to_owned())
}

fn default_retry_backoff() -> DurationSpec {
    DurationSpec("5s".to_owned())
}

fn default_retry_max_backoff() -> DurationSpec {
    DurationSpec("5m".to_owned())
}

const fn default_retry_attempts() -> u16 {
    1
}

const fn default_job_output_bytes() -> u32 {
    64 * 1024
}

const fn default_job_history_entries() -> u32 {
    100
}

fn default_timezone() -> String {
    "UTC".to_owned()
}
