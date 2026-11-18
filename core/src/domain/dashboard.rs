use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Severity;
use crate::domain::environment_delivery::ResolvedEnvironmentDelivery;
use crate::domain::ingress::IngressRouteProvenance;
use crate::domain::operation::{OperationKind, OperationState};
use crate::domain::runtime::{RuntimeKind, ServiceState};
use crate::domain::service_stack::ServiceComponentKind;
use crate::domain::workspace::ProjectId;

pub const DASHBOARD_API_VERSION: DashboardApiVersion = DashboardApiVersion::V1;
pub const MAX_DASHBOARD_PROJECTS: usize = 100;
pub const MAX_DASHBOARD_SERVICES: usize = 500;
pub const MAX_DASHBOARD_OPERATIONS: usize = 20;
pub const MAX_DASHBOARD_DIAGNOSTICS: usize = 100;
pub const MAX_DASHBOARD_JOB_RUNS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum DashboardApiVersion {
    #[serde(rename = "1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DashboardScope {
    Installation,
    Project(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardResponse {
    pub api_version: DashboardApiVersion,
    pub generated_at_unix_ms: i64,
    pub scope: DashboardScopeDto,
    pub installation: InstallationDashboard,
    pub projects: Vec<DashboardProjectSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectDashboard>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DashboardScopeDto {
    Installation,
    Project { slug: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InstallationDashboard {
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeKind>,
    pub route: String,
    pub components: Vec<DashboardComponent>,
    pub diagnostics: Vec<DashboardDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardComponent {
    pub kind: ServiceComponentKind,
    pub version: String,
    pub running: bool,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardProjectSummary {
    pub project_id: ProjectId,
    pub name: String,
    pub slug: String,
    pub dashboard_url: String,
    pub environment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectDashboard {
    pub project_id: ProjectId,
    pub name: String,
    pub slug: String,
    pub environment: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeKind>,
    pub runtime_available: bool,
    pub config_revision: String,
    pub environment_delivery: ResolvedEnvironmentDelivery,
    pub services: Vec<DashboardService>,
    pub resources: Vec<DashboardResource>,
    pub operations: Vec<DashboardOperation>,
    pub jobs: Vec<DashboardJob>,
    pub schedules: Vec<DashboardSchedule>,
    pub job_runs: Vec<DashboardJobRun>,
    pub diagnostics: Vec<DashboardDiagnostic>,
    /// The `development: optional` units; the dashboard greys out those
    /// that are off on this machine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional: Vec<crate::domain::development_units::OptionalUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardJob {
    pub name: String,
    pub module: String,
    pub service: String,
    pub timeout: String,
    pub maximum_attempts: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardSchedule {
    pub name: String,
    pub job: String,
    pub enabled: bool,
    pub retired: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_due_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardJobRun {
    pub id: uuid::Uuid,
    pub job: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
    pub state: crate::domain::scheduler::JobRunState,
    pub attempt: u16,
    pub requested_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub output_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardService {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    pub stack: String,
    pub state: ServiceState,
    pub health: DashboardHealth,
    pub desired_count: u32,
    pub running_count: u32,
    pub healthy_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub routes: Vec<DashboardRoute>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DashboardHealth {
    Healthy,
    Degraded,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardRoute {
    pub url: String,
    pub secure: bool,
    pub provenance: IngressRouteProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardResource {
    pub kind: DashboardResourceKind,
    pub logical_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DashboardResourceKind {
    Network,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardOperation {
    pub kind: OperationKind,
    pub state: OperationState,
    pub requested_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DashboardDiagnostic {
    pub code: String,
    pub severity: Severity,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DashboardEvent {
    Connected {
        api_version: DashboardApiVersion,
        sequence: u64,
    },
    Refresh {
        sequence: u64,
        occurred_at_unix_ms: i64,
    },
}

#[derive(JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct DashboardContract {
    pub dashboard: DashboardResponse,
    pub event: DashboardEvent,
}

#[must_use]
pub fn dashboard_contract_schema() -> schemars::Schema {
    let mut schema = schemars::schema_for!(DashboardContract);
    schema.insert(
        "$comment".to_owned(),
        serde_json::Value::String(
            "Generated from DashboardContract; run ./scripts/generate-artifacts.sh. Do not edit."
                .to_owned(),
        ),
    );
    schema
}

#[cfg(test)]
mod tests;
