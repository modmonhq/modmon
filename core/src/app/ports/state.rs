use std::path::Path;
use std::time::Duration;

use crate::domain::build::{ImageBuildPlan, ReusableImage, ReusableImageRecord};
use crate::domain::deployment::DeploymentPlan;
use crate::domain::environment::EnvironmentValueOverride;
use crate::domain::error::MmError;
use crate::domain::ingress::{
    CanonicalMiddleware, IngressRouteClaim, IngressRouteConflict, IngressRouteSet,
};
use crate::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationState, OperationSummary,
};
use crate::domain::target::{TargetBootstrapPlan, TargetId};
use crate::domain::workspace::{ProjectId, WorkspaceId, WorkspaceRecord, WorkspaceRegistration};

pub trait StateStore: Send + Sync {
    /// The most recently used first.
    fn workspace_usage(
        &self,
        limit: usize,
    ) -> Result<Vec<crate::domain::workspace_registry::WorkspaceUsage>, MmError>;
    /// Deletes each workspace with its operations in one transaction, and
    /// nothing if one of them changed since `expected` was read.
    fn forget_workspaces(
        &self,
        expected: &[crate::domain::workspace_registry::WorkspaceUsage],
    ) -> Result<u64, MmError>;
    fn register_workspace(
        &self,
        registration: &WorkspaceRegistration,
        now_unix_ms: i64,
    ) -> Result<WorkspaceRecord, MmError>;
    fn recent_operations(
        &self,
        workspace_id: WorkspaceId,
        limit: usize,
    ) -> Result<Vec<OperationSummary>, MmError>;
    fn running_operations(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<Vec<OperationSummary>, MmError>;
    fn running_operation_ids(&self) -> Result<std::collections::HashSet<OperationId>, MmError>;
    fn begin_operation(&self, operation: &NewOperation) -> Result<(), MmError>;
    fn save_deployment_plan(
        &self,
        operation_id: OperationId,
        plan: &DeploymentPlan,
    ) -> Result<(), MmError>;
    fn deployment_plan(&self, operation_id: OperationId)
    -> Result<Option<DeploymentPlan>, MmError>;
    fn save_image_build_plan(
        &self,
        operation_id: OperationId,
        plan: &ImageBuildPlan,
    ) -> Result<(), MmError>;
    fn save_image_build_result(
        &self,
        operation_id: OperationId,
        result: &crate::domain::build::ImageBuildResult,
    ) -> Result<(), MmError>;
    fn latest_successful_image_build(
        &self,
        project_id: ProjectId,
        project_root: &Path,
        environment: &str,
    ) -> Result<Option<(ImageBuildPlan, crate::domain::build::ImageBuildResult)>, MmError>;
    /// The newest successful build of this checkout and environment that
    /// `filter` accepts.
    fn latest_image_build(
        &self,
        project_id: ProjectId,
        project_root: &Path,
        environment: &str,
        filter: crate::domain::build::ImageBuildFilter,
    ) -> Result<Option<(ImageBuildPlan, crate::domain::build::ImageBuildResult)>, MmError>;
    fn lookup_reusable_image(
        &self,
        project_id: ProjectId,
        environment: &str,
        fingerprint: &str,
        module: &str,
        variant: Option<&str>,
    ) -> Result<Option<ReusableImage>, MmError>;
    fn record_reusable_image(&self, record: ReusableImageRecord<'_>) -> Result<(), MmError>;
    fn save_target_bootstrap_plan(
        &self,
        operation_id: OperationId,
        plan: &TargetBootstrapPlan,
    ) -> Result<(), MmError>;
    fn target_bootstrap_plan(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<TargetBootstrapPlan>, MmError>;
    fn save_target_bootstrap_result(
        &self,
        operation_id: OperationId,
        result: &crate::domain::target::TargetBootstrapResult,
    ) -> Result<(), MmError>;
    fn latest_successful_deployment(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Option<DeploymentPlan>, MmError>;
    fn latest_successful_deployment_for_owner(
        &self,
        project_id: ProjectId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Option<DeploymentPlan>, MmError>;
    /// The deployments of one owner that succeeded, run, or failed, newest
    /// first.
    fn deployment_candidates_for_owner(
        &self,
        project_id: ProjectId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Vec<(OperationState, DeploymentPlan)>, MmError>;
    /// The deployments of one owner that began and neither finished nor
    /// failed, whichever checkout they were made from, newest first.
    fn running_deployments_for_owner(
        &self,
        project_id: ProjectId,
        environment: &str,
        target_id: TargetId,
    ) -> Result<Vec<(OperationId, DeploymentPlan)>, MmError>;
    fn local_project_preference(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<crate::app::LocalSelectionPreference>, MmError>;
    fn local_environment_preference(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Option<crate::app::LocalSelectionPreference>, MmError>;
    fn set_local_project_preference(
        &self,
        project_id: ProjectId,
        runtime: Option<crate::domain::runtime::RuntimeKind>,
        target: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<(), MmError>;
    fn set_local_environment_preference(
        &self,
        project_id: ProjectId,
        environment: &str,
        runtime: Option<crate::domain::runtime::RuntimeKind>,
        target: Option<&str>,
        now_unix_ms: i64,
    ) -> Result<(), MmError>;
    fn unset_local_project_preference(&self, project_id: ProjectId) -> Result<bool, MmError>;
    fn unset_local_environment_preference(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<bool, MmError>;
    fn append_event(
        &self,
        operation_id: OperationId,
        occurred_at_unix_ms: i64,
        event: &OperationEvent,
    ) -> Result<(), MmError>;
    fn finish_operation(
        &self,
        operation_id: OperationId,
        state: OperationState,
        finished_at_unix_ms: i64,
        error_code: Option<&str>,
    ) -> Result<(), MmError>;
    fn try_acquire_lock(
        &self,
        resource: &str,
        owner: OperationId,
        now_unix_ms: i64,
        lease: Duration,
    ) -> Result<bool, MmError>;
    fn release_lock(&self, resource: &str, owner: OperationId) -> Result<(), MmError>;
}

pub trait EventSink: Send + Sync {
    fn publish(&self, event: OperationEvent) -> Result<(), MmError>;
}

pub trait Clock: Send + Sync {
    fn now_unix_ms(&self) -> i64;
}

pub trait EnvironmentValueStore: Send + Sync {
    fn list_environment_overrides(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
    ) -> Result<Vec<EnvironmentValueOverride>, MmError>;
    fn set_environment_override(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
        key: &str,
        value: &str,
        now_unix_ms: i64,
        expected_revision: Option<i64>,
    ) -> Result<EnvironmentValueOverride, MmError>;
    fn unset_environment_override(
        &self,
        workspace_id: WorkspaceId,
        environment: &str,
        key: &str,
    ) -> Result<bool, MmError>;
}

pub struct IngressClaimsReplacement<'a> {
    pub target_id: TargetId,
    pub project_id: ProjectId,
    pub environment: &'a str,
    pub expected_generation: u64,
    pub claims: &'a [IngressRouteClaim],
    pub middlewares: &'a std::collections::BTreeMap<String, CanonicalMiddleware>,
    pub preserve_gate_attachments: bool,
    pub config_digest: Option<&'a str>,
    pub now_unix_ms: i64,
}

pub trait IngressRouteReader: Send + Sync {
    fn read_routes(
        &self,
        target_id: crate::domain::target::TargetId,
    ) -> Result<crate::domain::ingress::IngressRouteSet, MmError>;
}

pub trait IngressRouteRegistry: Send + Sync {
    fn route_set(&self, target_id: TargetId) -> Result<IngressRouteSet, MmError>;
    fn conflicts(
        &self,
        proposed: &[IngressRouteClaim],
    ) -> Result<Vec<IngressRouteConflict>, MmError>;
    fn replace_owner_claims_cas(
        &self,
        replacement: IngressClaimsReplacement<'_>,
    ) -> Result<IngressRouteSet, MmError>;
    fn replace_owner_claims(
        &self,
        target_id: TargetId,
        project_id: ProjectId,
        environment: &str,
        claims: &[IngressRouteClaim],
        middlewares: &std::collections::BTreeMap<String, CanonicalMiddleware>,
        now_unix_ms: i64,
    ) -> Result<(), MmError>;
}
