//! The repo's `agents:` key: what an AI agent may do through `mm mcp serve`
//! in each environment. Anything outside the grant is handed to the user as
//! an exact command to run. A remote host applies the same block to keys
//! marked `agent: true`, from the commit it deployed.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::host_policy::{AccessLevel, EnvironmentAccess, ServiceSelection};
use crate::domain::users::ALL_ENVIRONMENTS;

/// What an agent may do in one environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct AgentAccess {
    /// `view`, `operate`, or `deploy`. Agents are never `admin`.
    pub level: AccessLevel,
    /// Services the agent may run commands in: `true`, or a list of names.
    #[serde(default, skip_serializing_if = "ServiceSelection::is_none")]
    pub exec: ServiceSelection,
    /// The agent may set secrets and variables on the environment's hosts
    /// for the next deployment, when its user may. With level `deploy` on an
    /// environment that runs on this machine, it may also create the local
    /// secret store, generate missing secrets, and install a managed secret
    /// provider. It never reads a value.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secrets: bool,
}

impl Default for AgentAccess {
    fn default() -> Self {
        Self::VIEW
    }
}

/// `production: view` or `local: { level: deploy, exec: true }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum AgentAccessConfig {
    Level(AccessLevel),
    Detailed(AgentAccess),
}

impl<'de> Deserialize<'de> for AgentAccessConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        crate::domain::config::word_or_mapping::word_or_mapping(
            deserializer,
            "an access level, or a mapping with `level:`",
            Self::Level,
            Self::Detailed,
        )
    }
}

impl AgentAccessConfig {
    #[must_use]
    pub fn access(&self) -> AgentAccess {
        match self {
            Self::Level(level) => AgentAccess {
                level: *level,
                exec: ServiceSelection::All(false),
                secrets: false,
            },
            Self::Detailed(access) => access.clone(),
        }
    }
}

/// Per environment name, or `*` for every environment not listed.
pub type AgentsConfig = BTreeMap<String, AgentAccessConfig>;

/// One thing an agent asks to do through MCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentAction {
    /// Project, environment, service, route, job, and operation reads.
    Read,
    /// Finite redacted service logs.
    Logs,
    /// A read-only plan of a change.
    Plan,
    /// Run, cancel, or retry a job, or run a schedule now.
    JobRun,
    /// Restart one service's containers.
    ServiceRestart,
    /// Reconcile jobs and schedules, or enable and disable a schedule.
    ScheduleManage,
    /// Apply a reviewed configuration edit.
    ConfigApply,
    /// Apply a reviewed image build.
    BuildApply,
    /// Apply a reviewed deployment.
    DeployApply,
    /// Run a command in a service container.
    Exec,
    /// Set a secret or variable on a remote environment's hosts.
    SetValues,
    /// Turn an optional unit on or off for the development environment.
    DevelopmentUnit,
    /// Install or repair the shared local services on this machine.
    DevelopmentSetup,
    /// Create the local secret store, generate missing secrets, or install
    /// a managed secret provider on this machine.
    SecretSetup,
}

impl AgentAction {
    pub const ALL: [Self; 14] = [
        Self::Read,
        Self::Logs,
        Self::Plan,
        Self::JobRun,
        Self::ServiceRestart,
        Self::ScheduleManage,
        Self::ConfigApply,
        Self::BuildApply,
        Self::DeployApply,
        Self::Exec,
        Self::SetValues,
        Self::DevelopmentUnit,
        Self::DevelopmentSetup,
        Self::SecretSetup,
    ];

    /// The level that includes this action; `None` for the `exec` add-on.
    #[must_use]
    pub const fn required_level(self) -> Option<AccessLevel> {
        match self {
            Self::Read | Self::Logs | Self::Plan => Some(AccessLevel::View),
            Self::JobRun | Self::ServiceRestart => Some(AccessLevel::Operate),
            Self::ScheduleManage
            | Self::ConfigApply
            | Self::BuildApply
            | Self::DeployApply
            | Self::DevelopmentUnit
            | Self::DevelopmentSetup
            | Self::SecretSetup => Some(AccessLevel::Deploy),
            Self::Exec | Self::SetValues => None,
        }
    }

    /// What the `agents:` block must say to allow this action.
    #[must_use]
    pub fn requirement(self) -> String {
        match (self, self.required_level()) {
            (Self::SecretSetup, _) => "level deploy and secrets: true".to_owned(),
            (_, Some(level)) => format!("level {}", level.as_str()),
            (Self::SetValues, None) => "secrets: true".to_owned(),
            (_, None) => "exec".to_owned(),
        }
    }
}

impl AgentAccess {
    pub const VIEW: Self = Self {
        level: AccessLevel::View,
        exec: ServiceSelection::All(false),
        secrets: false,
    };

    /// Whether this grant covers `action`. `service` is the service an
    /// `exec` runs in.
    #[must_use]
    pub fn permits(&self, action: AgentAction, service: Option<&str>) -> bool {
        match action.required_level() {
            Some(level) => {
                self.level >= level && (action != AgentAction::SecretSetup || self.secrets)
            }
            None if action == AgentAction::SetValues => self.secrets,
            None => match (self.exec.services(), service) {
                (None, _) | (Some(_), None) => false,
                (Some(services), Some(service)) => {
                    services.is_empty() || services.contains(service)
                }
            },
        }
    }

    /// Every action this grant covers. `exec` counts when it covers any
    /// service.
    #[must_use]
    pub fn allowed(&self) -> Vec<AgentAction> {
        AgentAction::ALL
            .into_iter()
            .filter(|action| match action {
                AgentAction::Exec => self.exec.services().is_some(),
                action => self.permits(*action, None),
            })
            .collect()
    }

    /// The narrower of this grant and `ceiling`.
    #[must_use]
    pub fn within(&self, ceiling: &Self) -> Self {
        Self {
            level: self.level.min(ceiling.level),
            exec: self.exec.intersect(&ceiling.exec),
            secrets: self.secrets && ceiling.secrets,
        }
    }

    /// This grant as remote access: never `forward` or `admin`.
    #[must_use]
    pub fn environment_access(&self) -> EnvironmentAccess {
        EnvironmentAccess {
            level: self.level.min(AccessLevel::Deploy),
            exec: self.exec.clone(),
            forward: ServiceSelection::All(false),
            secrets: self.secrets,
        }
    }

    /// `deploy +exec[api]`, for one-line display.
    #[must_use]
    pub fn summary(&self) -> String {
        self.environment_access().summary()
    }
}

/// The grant for `environment`: its own entry, else `*`, else `view`.
#[must_use]
pub fn agent_access_in(agents: &AgentsConfig, environment: &str) -> AgentAccess {
    agents
        .get(environment)
        .or_else(|| agents.get(ALL_ENVIRONMENTS))
        .map_or(AgentAccess::VIEW, AgentAccessConfig::access)
}

/// Problems in an `agents:` block.
#[must_use]
pub fn validate_agents(agents: &AgentsConfig, environments: &BTreeSet<String>) -> Vec<String> {
    let mut problems = Vec::new();
    for (environment, access) in agents {
        if environment != ALL_ENVIRONMENTS && !environments.contains(environment) {
            problems.push(format!(
                "agents grants access to unknown environment '{environment}'"
            ));
        }
        if access.access().level == AccessLevel::Admin {
            problems.push(format!(
                "agents '{environment}' cannot be admin; use view, operate, or deploy, and add exec for the services an agent may run commands in"
            ));
        }
    }
    problems
}

/// Identifies one exact `agents:` block. A user trusts this hash with
/// `mm agent trust`; an edited block has another hash and grants only `view`
/// until it is trusted again.
#[must_use]
pub fn agents_policy_hash(agents: &AgentsConfig) -> String {
    let canonical = agents
        .iter()
        .map(|(environment, access)| (environment, access.access()))
        .collect::<BTreeMap<_, _>>();
    let bytes = serde_json::to_vec(&("mm-agents-policy-v1", &canonical)).unwrap_or_default();
    blake3::hash(&bytes).to_hex().to_string()
}

/// The block `mm init` writes: agents work freely in the development
/// environment and only look everywhere else.
#[must_use]
pub fn initial_agents(development_environment: &str) -> AgentsConfig {
    BTreeMap::from([
        (
            ALL_ENVIRONMENTS.to_owned(),
            AgentAccessConfig::Level(AccessLevel::View),
        ),
        (
            development_environment.to_owned(),
            AgentAccessConfig::Detailed(AgentAccess {
                level: AccessLevel::Deploy,
                exec: ServiceSelection::All(true),
                secrets: false,
            }),
        ),
    ])
}

/// Environment variables that coding agents set for the commands they run.
/// `MM_AGENT` is for tools that are not listed.
pub const AGENT_ENVIRONMENT_MARKERS: [&str; 11] = [
    "MM_AGENT",
    "AI_AGENT",
    "CLAUDECODE",
    "OPENCODE",
    "CODEX_CI",
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CURSOR_AGENT",
    "GEMINI_CLI",
    "AGENT",
];

/// The first agent marker among `variables` that has a value.
#[must_use]
pub fn detect_agent<'a>(
    variables: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Option<&'static str> {
    let set = variables
        .into_iter()
        .filter(|(_, value)| !value.trim().is_empty() && *value != "0" && *value != "false")
        .map(|(name, _)| name)
        .collect::<BTreeSet<_>>();
    AGENT_ENVIRONMENT_MARKERS
        .into_iter()
        .find(|marker| set.contains(marker))
}

/// The tool an agent marker stands for, as records name it.
#[must_use]
pub fn agent_label(marker: &str) -> &'static str {
    match marker {
        "CLAUDECODE" => "claude",
        "OPENCODE" => "opencode",
        "CURSOR_AGENT" => "cursor",
        "GEMINI_CLI" => "gemini",
        marker if marker.starts_with("CODEX_") => "codex",
        _ => "agent",
    }
}

#[cfg(test)]
#[path = "agents/tests.rs"]
mod tests;
