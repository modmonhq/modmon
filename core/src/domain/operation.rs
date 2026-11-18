use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::workspace::WorkspaceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct OperationId(pub Uuid);

impl OperationId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Build,
    Bootstrap,
    Deploy,
    Down,
    Reconcile,
    Rollback,
    Job,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Planned,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Validate,
    Render,
    Plan,
    Build,
    Apply,
    Verify,
    Cleanup,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationEvent {
    Started {
        operation_id: OperationId,
        kind: OperationKind,
    },
    PhaseChanged {
        operation_id: OperationId,
        phase: OperationPhase,
    },
    Completed {
        operation_id: OperationId,
        state: OperationState,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OperationSummary {
    pub id: OperationId,
    pub kind: OperationKind,
    pub environment: String,
    pub state: OperationState,
    pub requested_at_unix_ms: i64,
    pub finished_at_unix_ms: Option<i64>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OperationRequest {
    pub workspace_id: WorkspaceId,
    pub kind: OperationKind,
    pub environment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_plan_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewOperation {
    pub id: OperationId,
    pub workspace_id: WorkspaceId,
    pub kind: OperationKind,
    pub environment: String,
    pub requested_at_unix_ms: i64,
    pub plan_hash: Option<String>,
}
