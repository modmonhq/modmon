use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::agents::{AgentAccess, AgentAction, AgentsConfig};
use crate::domain::error::MmError;
use crate::domain::onboarding::ConfigMutationRequest;
use crate::domain::release_facts::ReleaseRequest;
use crate::domain::runtime::RuntimeKind;

pub const MCP_TOOL_SCHEMA_VERSION: &str = "mm-mcp-tools-v2";
pub const MCP_RESOURCE_SCHEMA_VERSION: &str = "mm-mcp-resources-v2";
pub const MCP_CONFIG_SCHEMA_VERSION: u32 = 2;
pub const MCP_PROJECT_SCHEMA_VERSION: &str = "mm-project-v1";
pub const MCP_MAX_PAGE_SIZE: usize = 100;
pub const MCP_MAX_LOG_RECORDS: u32 = 200;
pub const MCP_MAX_LOG_BYTES: usize = 256 * 1024;
pub const MCP_MAX_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MCP_MAX_REQUEST_TIMEOUT_MS: u64 = 30_000;
pub const MCP_PLAN_TTL_MS: i64 = 10 * 60 * 1_000;
pub const MCP_MAX_EXEC_SECONDS: u64 = 300;
pub const MCP_MAX_EXEC_BYTES: usize = 256 * 1024;
pub const MCP_MAX_EXEC_ARGUMENTS: usize = 64;
/// The source every MCP request is attributed to.
pub const MCP_SOURCE: &str = "mcp-stdio";

/// Optional private limits for this device's MCP server (`MM_HOME/mcp.json`).
/// What an agent may do comes from the project's reviewed `agents:` block;
/// this file can only narrow it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpServerConfig {
    pub schema_version: u32,
    /// The name audit records attribute agent requests to.
    #[serde(default = "default_principal")]
    pub principal: String,
    /// The most an agent may do on this device, per environment name or `*`.
    /// An environment without an entry is limited only by the project.
    #[serde(default)]
    pub ceiling: AgentsConfig,
    #[serde(default = "default_page_size")]
    pub maximum_page_size: usize,
    #[serde(default = "default_response_bytes")]
    pub maximum_response_bytes: usize,
    #[serde(default = "default_timeout_ms")]
    pub request_timeout_ms: u64,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            schema_version: MCP_CONFIG_SCHEMA_VERSION,
            principal: default_principal(),
            ceiling: AgentsConfig::new(),
            maximum_page_size: default_page_size(),
            maximum_response_bytes: default_response_bytes(),
            request_timeout_ms: default_timeout_ms(),
        }
    }
}

impl McpServerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != MCP_CONFIG_SCHEMA_VERSION {
            return Err(format!(
                "unsupported MCP configuration schema {}; expected {MCP_CONFIG_SCHEMA_VERSION}. Version 1 listed environments and capabilities here; they now come from the project's `agents:` block, so delete the file or keep only `schema_version`, `principal`, `ceiling`, and the bounds",
                self.schema_version
            ));
        }
        if self.principal.trim().is_empty()
            || self.principal.len() > 255
            || self.principal.chars().any(char::is_control)
        {
            return Err("MCP principal must be 1-255 bytes without control characters".to_owned());
        }
        if self
            .ceiling
            .keys()
            .any(|name| name != "*" && !valid_scope_name(name))
        {
            return Err("MCP ceiling names an invalid environment".to_owned());
        }
        if let Some(problem) = self.ceiling.iter().find(|(_, access)| {
            access.access().level == crate::domain::host_policy::AccessLevel::Admin
        }) {
            return Err(format!(
                "MCP ceiling '{}' cannot be admin; use view, operate, or deploy",
                problem.0
            ));
        }
        if self.maximum_page_size == 0
            || self.maximum_page_size > MCP_MAX_PAGE_SIZE
            || self.maximum_response_bytes == 0
            || self.maximum_response_bytes > MCP_MAX_RESPONSE_BYTES
            || self.request_timeout_ms == 0
            || self.request_timeout_ms > MCP_MAX_REQUEST_TIMEOUT_MS
        {
            return Err("MCP policy bounds exceed the server maximums".to_owned());
        }
        Ok(())
    }

    /// `granted` limited by this device's ceiling for `environment`.
    #[must_use]
    pub fn limit(&self, environment: &str, granted: &AgentAccess) -> AgentAccess {
        self.ceiling
            .get(environment)
            .or_else(|| self.ceiling.get("*"))
            .map_or_else(
                || granted.clone(),
                |ceiling| granted.within(&ceiling.access()),
            )
    }

    /// How long a call of `operation` may take: `request_timeout_ms`, except
    /// for applying a deployment, a build, or a job.
    #[must_use]
    pub fn call_bound(&self, operation: &str) -> Option<std::time::Duration> {
        (!matches!(
            operation,
            "deploy_apply" | "build_apply" | "job_apply" | "setup_apply"
        ))
        .then(|| std::time::Duration::from_millis(self.request_timeout_ms))
    }
}

/// Where a tool call applies. Only the environment is required; the target
/// and runtime default to what the project and this device select.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpScope {
    pub environment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeKind>,
}

/// A scope with its target and runtime resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpSelectedScope {
    pub environment: String,
    pub target: String,
    pub runtime: RuntimeKind,
}

/// What a tool call asks permission for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuthorizationRequest {
    pub scope: McpScope,
    pub action: AgentAction,
    /// The service an `exec` runs in.
    pub service: Option<String>,
}

/// An authorized request. Only `McpUseCases::authorize` creates one, so a
/// use case that receives it knows the grant was checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRequestContext {
    pub request_id: uuid::Uuid,
    pub principal: String,
    pub source: String,
    pub project_root: PathBuf,
    pub scope: McpSelectedScope,
    /// The environment runs on a remote host, which enforces its own copy of
    /// the agent's limits.
    pub remote: bool,
    /// What the agent may do in this environment.
    pub access: AgentAccess,
}

impl McpRequestContext {
    /// Fails unless the authorized grant covers `action`.
    pub fn require(&self, action: AgentAction, service: Option<&str>) -> Result<(), MmError> {
        if self.source != MCP_SOURCE || !self.access.permits(action, service) {
            return Err(MmError::Security {
                message: format!(
                    "agent access '{}' in environment '{}' does not include {}",
                    self.access.summary(),
                    self.scope.environment,
                    action.requirement()
                ),
            });
        }
        Ok(())
    }
}

/// Why an agent may not do something itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpDenialReason {
    /// The `agents:` block does not grant the action in this environment.
    NotGranted,
    /// The `agents:` block changed since the user last trusted it.
    PolicyNotTrusted,
    /// The environment is remote and this device has no agent key.
    AgentKeyMissing,
    /// Users and agent access are never edited through MCP.
    AccessEdit,
}

/// A refused action, with what the user needs to know to do it themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpDenial {
    pub request_id: uuid::Uuid,
    pub reason: McpDenialReason,
    pub project_root: PathBuf,
    pub environment: String,
    pub action: AgentAction,
    pub service: Option<String>,
    /// What the agent may do in this environment now.
    pub access: AgentAccess,
}

#[derive(Debug)]
pub enum McpAuthorizationError {
    Denied(Box<McpDenial>),
    Failed(MmError),
}

impl From<MmError> for McpAuthorizationError {
    fn from(error: MmError) -> Self {
        Self::Failed(error)
    }
}

/// One environment's entry in the access overview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpEnvironmentAccess {
    pub environment: String,
    /// `view`, `operate +exec[api]`, and so on.
    pub access: String,
    pub level: crate::domain::host_policy::AccessLevel,
    /// Services `mm_exec` may run in: `true`, `false`, or names.
    pub exec: crate::domain::host_policy::ServiceSelection,
}

/// What the agent may do in every environment of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpAccessOverview {
    pub schema_version: String,
    pub project_root: PathBuf,
    /// False when the `agents:` block changed since the user trusted it;
    /// every environment is then `view` until `mm agent trust`.
    pub policy_trusted: bool,
    pub default_environment: Option<String>,
    pub environments: Vec<McpEnvironmentAccess>,
    /// How to proceed with anything outside these grants.
    pub outside_the_grant: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpReadKind {
    Project,
    Environments,
    Targets,
    Plans,
    Services,
    Routes,
    Health,
    Operations,
    Jobs,
    Schedules,
    Gates,
    /// Every node of a cluster target: whether it answers, its `mmd`
    /// version, and the commit it last applied.
    Nodes,
    /// The WireGuard mesh between a cluster target's nodes: per node and
    /// peer the endpoint, last handshake, and traffic, and what is wrong.
    Mesh,
    /// The shared volumes of a cluster target: the node that keeps them,
    /// each volume's size and the nodes that have it mounted, whether every
    /// node reaches the storage node, and what is wrong: what
    /// `mm target storage status` prints.
    Storage,
    /// What each service name resolves to on the networks that span a
    /// cluster target's nodes: every container behind the name, its node
    /// and health, and what keeps a node from answering names.
    Names,
    /// What each host of a remote environment last deployed, what it ran
    /// before, and a rollout that is running or was left unfinished: what
    /// `mm deploy status` prints.
    Deployments,
    /// Each secret and variable of the environment: on a remote one, per
    /// host what is live and what is staged for the next deployment, by
    /// whom; on this machine, what the local store holds. Never a value.
    Secrets,
    /// The managed secret store of a cluster environment: each member's
    /// OpenBao release, seal and Raft role, the leader, and what is wrong:
    /// what `mm secret provider status` prints for that store.
    SecretStore,
    /// The commands run in the project on this machine, newest first: what
    /// `mm history` lists. With `command`, that one command's plain output,
    /// progress events and result.
    Commands,
    /// The job runs of the last deployment that ran any, with their
    /// output: what `mm deploy logs` prints.
    DeployLogs,
    /// The `development: optional` modules, services and jobs, and which
    /// run in the development environment on this machine.
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpReadRequest {
    pub scope: McpScope,
    pub kind: McpReadKind,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_size")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: usize,
    /// With kind `commands`: the ID of one command to read whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// With kind `commands`: which commands to list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<crate::domain::command_log::CommandFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpLogRequest {
    pub scope: McpScope,
    pub service: String,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default = "default_log_tail")]
    #[schemars(range(min = 1, max = 200))]
    pub tail: u32,
    #[serde(default)]
    pub since_unix_ms: Option<i64>,
    #[serde(default)]
    pub until_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpExecRequest {
    pub scope: McpScope,
    pub service: String,
    #[serde(default)]
    pub stack: Option<String>,
    /// The executable and its arguments, exactly as given. No shell runs
    /// unless the first element is one.
    #[schemars(length(min = 1, max = 64))]
    pub argv: Vec<String>,
    #[serde(default)]
    pub workdir: Option<PathBuf>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default = "default_exec_seconds")]
    #[schemars(range(min = 1, max = 300))]
    pub timeout_seconds: u64,
}

/// A finished command. Output is exactly what the command printed, up to
/// the byte bound; it is not redacted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpExecResult {
    pub schema_version: String,
    pub service: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// Output passed the byte bound and the command was stopped.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpConfigPlanRequest {
    pub scope: McpScope,
    pub mutation: ConfigMutationRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpConfigApplyRequest {
    pub scope: McpScope,
    pub mutation: ConfigMutationRequest,
    pub plan_hash: String,
    pub approval_token: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpPlanRequest {
    pub scope: McpScope,
    #[serde(default)]
    pub publish: bool,
}

/// A deployment to review: of this checkout's commit, or, with `rollback`,
/// a return to what the remote environment ran before, as `mm deploy
/// rollback` plans it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpDeployPlanRequest {
    pub scope: McpScope,
    /// Has no effect on a deployment plan; accepted so that a request made
    /// for an earlier version of this tool still reads.
    #[serde(default)]
    pub publish: bool,
    /// Plan a return to the last finished deployment, or undo a rollout
    /// that was left unfinished. Remote environments only.
    #[serde(default)]
    pub rollback: bool,
    /// Plan the commit the hosts run now again, so that the secrets and
    /// variables staged on them go live. Remote environments only.
    #[serde(default)]
    pub redeploy: bool,
    /// The release's name and the inputs `release-inputs:` declares, as
    /// `mm deploy plan --release NAME --release-input NAME=VALUE` takes
    /// them. A rollback returns to the ones it ran with, and a redeploy keeps
    /// the ones the hosts run.
    #[serde(default, skip_serializing_if = "ReleaseRequest::is_empty")]
    pub release: ReleaseRequest,
}

/// `mm_dev_unit`: turn an optional unit on or off for the development
/// environment on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpDevelopmentUnitRequest {
    pub scope: McpScope,
    /// A module, service (or `module/service`) or job with
    /// `development: optional`.
    pub unit: String,
    /// `true` turns it on, `false` off.
    pub on: bool,
}

/// A step that sets up this machine for an environment that runs here, as
/// the command line takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpSetupStep {
    /// The shared local services `mm dev setup --services` installs:
    /// ingress, Mailpit, and the dashboard. Trusting mm's local certificate
    /// authority stays the user's (`mm dev setup`).
    DevServices {
        /// Publish SMTP on this loopback port.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        smtp_port: Option<u16>,
    },
    /// Install a managed provider declared under `providers:` on this
    /// machine, as `mm secret provider plan NAME --operation install` plans it.
    SecretProvider { name: String },
    /// Create the project's local age store, as `mm secret init age` does.
    SecretInit,
    /// Create a configured secret with a generator that has no value yet, as
    /// `mm secret generate NAME` does, or every missing one without `name`
    /// (`--missing`). An existing value is never replaced or shown.
    SecretGenerate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

impl McpSetupStep {
    /// What applying the step needs.
    #[must_use]
    pub const fn action(&self) -> AgentAction {
        match self {
            Self::DevServices { .. } => AgentAction::DevelopmentSetup,
            _ => AgentAction::SecretSetup,
        }
    }
}

/// `mm_setup_plan`: review a step that sets up this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpSetupPlanRequest {
    pub scope: McpScope,
    pub step: McpSetupStep,
}

/// `mm_setup_apply`: apply the step just reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpSetupApplyRequest {
    pub scope: McpScope,
    /// The step the plan was made for.
    pub step: McpSetupStep,
    pub plan_hash: String,
    pub approval_token: String,
    pub confirmation: String,
}

/// The review of a secret setup step, which the command line runs without
/// one: whether the local store exists, and the environment's secrets as
/// `mm secret status` shows them, without a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpSecretSetupPlan {
    pub plan_hash: String,
    pub environment: String,
    pub step: McpSetupStep,
    pub store_initialized: bool,
    pub current: Vec<crate::domain::secret::SecretBindingStatus>,
}

impl McpSecretSetupPlan {
    #[must_use]
    pub fn new(
        environment: String,
        step: McpSetupStep,
        store_initialized: bool,
        current: Vec<crate::domain::secret::SecretBindingStatus>,
    ) -> Self {
        let bytes = serde_json::to_vec(&(
            "mm-mcp-secret-setup-v1",
            &environment,
            &step,
            store_initialized,
            &current,
        ))
        .unwrap_or_default();
        Self {
            plan_hash: blake3::hash(&bytes).to_hex().to_string(),
            environment,
            step,
            store_initialized,
            current,
        }
    }
}

/// `mm_stage_value`: a secret or variable change staged on every host of a
/// remote environment for its next deployment.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpStageValueRequest {
    pub scope: McpScope,
    pub kind: crate::domain::staged::ValueKind,
    /// A secret's name under `secrets:`, or a variable's global key.
    pub key: String,
    pub action: McpStageAction,
    /// The new value, with `set`. It is sealed to each host and never
    /// returned or recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl std::fmt::Debug for McpStageValueRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("McpStageValueRequest")
            .field("scope", &self.scope)
            .field("kind", &self.kind)
            .field("key", &self.key)
            .field("action", &self.action)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpStageAction {
    Set,
    Unset,
    Discard,
}

/// What `mm_deploy_plan` plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpDeployPlanKind {
    /// This checkout's commit.
    Checkout,
    Rollback,
    Redeploy,
}

impl McpDeployPlanRequest {
    #[must_use]
    pub const fn kind(&self) -> McpDeployPlanKind {
        match (self.rollback, self.redeploy) {
            (true, _) => McpDeployPlanKind::Rollback,
            (false, true) => McpDeployPlanKind::Redeploy,
            (false, false) => McpDeployPlanKind::Checkout,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpApplyRequest {
    pub scope: McpScope,
    #[serde(default)]
    pub publish: bool,
    pub plan_hash: String,
    pub approval_token: String,
    pub confirmation: String,
    /// For a deployment on this machine: the `release` the plan was made
    /// with, since applying plans it again. A remote plan carries its own.
    #[serde(default, skip_serializing_if = "ReleaseRequest::is_empty")]
    pub release: ReleaseRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpJobAction {
    Reconcile,
    Enqueue { name: String },
    RunSchedule { name: String },
    Cancel { run_id: uuid::Uuid },
    Retry { run_id: uuid::Uuid },
    SetScheduleEnabled { name: String, enabled: bool },
}

impl McpJobAction {
    /// The grant this action needs: running, cancelling, and retrying are
    /// `operate`; changing what is scheduled is `deploy`.
    #[must_use]
    pub const fn agent_action(&self) -> AgentAction {
        match self {
            Self::Enqueue { .. }
            | Self::RunSchedule { .. }
            | Self::Cancel { .. }
            | Self::Retry { .. } => AgentAction::JobRun,
            Self::Reconcile | Self::SetScheduleEnabled { .. } => AgentAction::ScheduleManage,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpJobPlanRequest {
    pub scope: McpScope,
    pub action: McpJobAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpJobApplyRequest {
    pub scope: McpScope,
    pub action: McpJobAction,
    pub plan_hash: String,
    pub approval_token: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpPage {
    pub schema_version: String,
    pub items: Vec<serde_json::Value>,
    pub next_cursor: Option<String>,
    pub total_items: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct McpReviewedPlan {
    pub schema_version: String,
    pub plan_hash: String,
    /// Who applies this plan: `agent` when the grant covers it, else `user`.
    pub applied_by: McpPlanApplier,
    /// The single-use token for the matching apply tool; absent when the
    /// user applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_token: Option<String>,
    pub expires_at_unix_ms: i64,
    pub next_action: String,
    /// What the user runs when the agent may not apply.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_commands: Vec<String>,
    pub plan: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum McpPlanApplier {
    Agent,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpAuditRecord {
    pub id: uuid::Uuid,
    pub request_id: uuid::Uuid,
    pub principal: String,
    pub source: String,
    pub project_root: PathBuf,
    pub environment: String,
    /// Empty when the request was refused before a target was selected.
    pub target: String,
    pub operation: String,
    pub mutation: bool,
    pub plan_hash: Option<String>,
    pub outcome_code: String,
    pub occurred_at_unix_ms: i64,
}

pub fn reject_model_visible_sensitive_fields(value: &serde_json::Value) -> Result<(), String> {
    match value {
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                let normalized = key.to_ascii_lowercase().replace('-', "_");
                if matches!(
                    normalized.as_str(),
                    "password"
                        | "passwd"
                        | "plaintext"
                        | "private_key"
                        | "session_cookie"
                        | "session_token"
                        | "secret_value"
                        | "access_token"
                        | "refresh_token"
                ) {
                    return Err(format!(
                        "field '{key}' is prohibited in MCP arguments; use provider-native or hidden out-of-band input"
                    ));
                }
                reject_model_visible_sensitive_fields(value)?;
            }
            Ok(())
        }
        serde_json::Value::Array(values) => {
            for value in values {
                reject_model_visible_sensitive_fields(value)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn valid_scope_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
}

const fn default_page_size() -> usize {
    50
}

const fn default_response_bytes() -> usize {
    512 * 1024
}

const fn default_timeout_ms() -> u64 {
    15_000
}

const fn default_log_tail() -> u32 {
    100
}

const fn default_exec_seconds() -> u64 {
    30
}

fn default_principal() -> String {
    "agent".to_owned()
}

#[cfg(test)]
mod tests;
