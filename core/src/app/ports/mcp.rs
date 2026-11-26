use async_trait::async_trait;

use crate::domain::error::MmError;
use crate::domain::mcp::{
    McpAccessOverview, McpAuditRecord, McpAuthorizationError, McpAuthorizationRequest,
    McpExecRequest, McpExecResult, McpJobAction, McpLogRequest, McpReadKind, McpRequestContext,
};
use crate::domain::onboarding::ConfigMutationRequest;

pub trait McpAuditJournal: Send + Sync {
    fn append_mcp_audit(&self, record: &McpAuditRecord) -> Result<(), MmError>;
}

/// The application behind the MCP tools. Every use case takes the context
/// `authorize` returned for it and checks the grant again.
#[async_trait]
pub trait McpUseCases: Send + Sync {
    /// Resolves the project, environment, target, and runtime of a tool
    /// call and checks the agent's grant for the action. A refusal says why,
    /// so the server can hand the action to the user.
    async fn authorize(
        &self,
        request: McpAuthorizationRequest,
    ) -> Result<McpRequestContext, McpAuthorizationError>;

    /// What the agent may do in every environment of the project.
    async fn access(&self) -> Result<McpAccessOverview, MmError>;

    async fn read(
        &self,
        context: &McpRequestContext,
        kind: McpReadKind,
        cursor: Option<String>,
        limit: usize,
    ) -> Result<serde_json::Value, MmError>;

    /// The project's command log: a page of commands, or one command whole.
    async fn commands(
        &self,
        _context: &McpRequestContext,
        _request: &crate::domain::mcp::McpReadRequest,
    ) -> Result<serde_json::Value, MmError> {
        Err(MmError::State {
            message: "this MCP server keeps no command log".to_owned(),
        })
    }

    async fn logs(
        &self,
        context: &McpRequestContext,
        request: McpLogRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<Vec<crate::domain::observability::LogRecord>, MmError>;

    async fn exec(
        &self,
        context: &McpRequestContext,
        request: McpExecRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<McpExecResult, MmError>;

    async fn plan_config(
        &self,
        context: &McpRequestContext,
        mutation: ConfigMutationRequest,
    ) -> Result<crate::domain::onboarding::ConfigMutationPlan, MmError>;

    async fn apply_config(
        &self,
        context: &McpRequestContext,
        mutation: ConfigMutationRequest,
        plan_hash: &str,
    ) -> Result<crate::domain::onboarding::ConfigMutationResult, MmError>;

    async fn plan_build(
        &self,
        context: &McpRequestContext,
        publish: bool,
    ) -> Result<crate::domain::build::ImageBuildPlan, MmError>;

    async fn apply_build(
        &self,
        context: &McpRequestContext,
        publish: bool,
        plan_hash: &str,
    ) -> Result<crate::domain::build::ImageBuildResult, MmError>;

    /// Stages a secret or variable change on every host of a remote
    /// environment. The result names keys, revisions and hosts, never a
    /// value.
    async fn stage_value(
        &self,
        _context: &McpRequestContext,
        _request: crate::domain::mcp::McpStageValueRequest,
    ) -> Result<serde_json::Value, MmError> {
        Err(MmError::Environment {
            message: "this server cannot stage secrets or variables".to_owned(),
        })
    }

    /// Turns an optional unit on or off for the development environment on
    /// this machine, as `mm dev enable` and `mm dev disable` do.
    async fn development_unit(
        &self,
        _context: &McpRequestContext,
        _request: crate::domain::mcp::McpDevelopmentUnitRequest,
    ) -> Result<serde_json::Value, MmError> {
        Err(MmError::Environment {
            message: "this server cannot turn optional units on or off".to_owned(),
        })
    }

    /// A deployment plan to review: of this checkout, or the plan `mm
    /// deploy rollback` or `mm deploy redeploy` makes for a remote
    /// environment.
    async fn plan_deployment(
        &self,
        context: &McpRequestContext,
        (kind, release): (
            crate::domain::mcp::McpDeployPlanKind,
            &crate::domain::release_facts::ReleaseRequest,
        ),
    ) -> Result<serde_json::Value, MmError>;

    /// Applies a reviewed deployment plan: the deployment result, or, on a
    /// cluster target, what each node did. `release` is what a plan on
    /// this machine was made with.
    async fn apply_deployment(
        &self,
        context: &McpRequestContext,
        plan_hash: &str,
        release: &crate::domain::release_facts::ReleaseRequest,
    ) -> Result<serde_json::Value, MmError>;

    async fn plan_job(
        &self,
        context: &McpRequestContext,
        action: McpJobAction,
    ) -> Result<serde_json::Value, MmError>;

    async fn apply_job(
        &self,
        context: &McpRequestContext,
        action: McpJobAction,
        plan_hash: &str,
    ) -> Result<serde_json::Value, MmError>;

    /// `mm_read` kinds `env` and `secrets`, of every key or of `key` alone.
    async fn read_values(
        &self,
        _context: &McpRequestContext,
        _kind: McpReadKind,
        _key: Option<&str>,
    ) -> Result<serde_json::Value, MmError> {
        Err(MmError::Environment {
            message: "this server cannot read variables or secrets".to_owned(),
        })
    }

    /// What restarting `service` touches, with its `plan_hash`.
    async fn plan_service_restart(
        &self,
        _context: &McpRequestContext,
        _service: &str,
    ) -> Result<crate::domain::mcp::McpServiceRestartPlan, MmError> {
        Err(MmError::Environment {
            message: "this server cannot restart services".to_owned(),
        })
    }

    /// Restarts `service` as `mm service restart` does, when its plan is
    /// still `plan_hash`.
    async fn apply_service_restart(
        &self,
        _context: &McpRequestContext,
        _service: &str,
        _plan_hash: &str,
    ) -> Result<crate::domain::observability::RestartOutcome, MmError> {
        Err(MmError::Environment {
            message: "this server cannot restart services".to_owned(),
        })
    }

    /// The plan of a step that sets up this machine, with its `plan_hash`.
    async fn plan_setup(
        &self,
        _context: &McpRequestContext,
        _step: &crate::domain::mcp::McpSetupStep,
    ) -> Result<serde_json::Value, MmError> {
        Err(MmError::Environment {
            message: "this server cannot set up this machine".to_owned(),
        })
    }

    /// Applies a reviewed setup step when its plan is still `plan_hash`.
    async fn apply_setup(
        &self,
        _context: &McpRequestContext,
        _step: &crate::domain::mcp::McpSetupStep,
        _plan_hash: &str,
    ) -> Result<serde_json::Value, MmError> {
        Err(MmError::Environment {
            message: "this server cannot set up this machine".to_owned(),
        })
    }
}
