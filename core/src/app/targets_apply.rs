use std::sync::Arc;

use crate::app::ports::{
    Clock, ProjectDocumentStore, StateStore, TargetBootstrapExecutor, TargetOperationLock,
};
use crate::app::{BootstrapAccessRequest, TargetManager, WorkspaceSelection};
use crate::domain::error::MmError;
use crate::domain::operation::{
    NewOperation, OperationEvent, OperationId, OperationKind, OperationPhase, OperationState,
};
use crate::domain::target::{TargetBootstrapPlan, TargetBootstrapResult};
use crate::domain::workspace::WorkspaceRegistration;

#[derive(Clone)]
pub struct TargetBootstrapExecutionPorts {
    pub documents: Arc<dyn ProjectDocumentStore>,
    pub state: Arc<dyn StateStore>,
    pub locks: Arc<dyn TargetOperationLock>,
    pub executor: Arc<dyn TargetBootstrapExecutor>,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for TargetBootstrapExecutionPorts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TargetBootstrapExecutionPorts")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct TargetBootstrapService {
    planner: TargetManager,
    ports: TargetBootstrapExecutionPorts,
}

impl std::fmt::Debug for TargetBootstrapService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TargetBootstrapService")
            .finish_non_exhaustive()
    }
}

impl TargetBootstrapService {
    #[must_use]
    pub fn new(planner: TargetManager, ports: TargetBootstrapExecutionPorts) -> Self {
        Self { planner, ports }
    }

    pub async fn plan(
        &self,
        selection: &WorkspaceSelection,
        access: Option<&BootstrapAccessRequest>,
    ) -> Result<TargetBootstrapPlan, MmError> {
        self.planner
            .bootstrap_plan_on_host(selection, Some(self.ports.state.as_ref()), access)
            .await
    }

    /// Applies the plan only if `selection` and `access` still produce
    /// `expected_plan_hash`.
    pub async fn apply_reviewed(
        &self,
        selection: &WorkspaceSelection,
        expected_plan_hash: &str,
        access: Option<&BootstrapAccessRequest>,
    ) -> Result<TargetBootstrapResult, MmError> {
        let preview = self.plan(selection, access).await?;
        require_plan_hash(&preview, expected_plan_hash)?;
        let _guard = self.ports.locks.acquire(preview.target.definition.id)?;
        let plan = self.plan(selection, access).await?;
        require_plan_hash(&plan, expected_plan_hash)?;
        plan.validate()
            .map_err(|message| MmError::Security { message })?;

        let document = self.ports.documents.load_from(&selection.start)?;
        if document.config.project.id != plan.project_id {
            return Err(MmError::Security {
                message: "workspace identity changed after the reviewed bootstrap plan".to_owned(),
            });
        }
        let now = self.ports.clock.now_unix_ms();
        let workspace = self.ports.state.register_workspace(
            &WorkspaceRegistration {
                project_id: plan.project_id,
                root: document.root.clone(),
                config_path: document.path,
            },
            now,
        )?;
        for operation in self.ports.state.running_operations(workspace.id)? {
            if operation.kind != OperationKind::Bootstrap {
                continue;
            }
            let snapshot = self.ports.state.target_bootstrap_plan(operation.id)?;
            if snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.target.definition.id == plan.target.definition.id)
            {
                self.ports.state.finish_operation(
                    operation.id,
                    OperationState::Interrupted,
                    now,
                    Some("MM_BOOTSTRAP_INTERRUPTED"),
                )?;
            }
        }

        let operation_id = OperationId::new();
        self.ports.state.begin_operation(&NewOperation {
            id: operation_id,
            workspace_id: workspace.id,
            kind: OperationKind::Bootstrap,
            environment: plan.environment.clone(),
            requested_at_unix_ms: now,
            plan_hash: Some(plan.plan_hash.clone()),
        })?;
        if let Err(error) = self
            .ports
            .state
            .save_target_bootstrap_plan(operation_id, &plan)
        {
            if let Err(journal) =
                self.finish(operation_id, OperationState::Failed, Some(error.code()))
            {
                return Err(MmError::operation_failure(
                    operation_id.0.to_string(),
                    crate::domain::error::MutationStatus::NotStarted,
                    error,
                    vec![journal.to_string()],
                    "repair the incomplete bootstrap journal before retrying",
                ));
            }
            return Err(error);
        }

        let result = self.execute(operation_id, &document.root, &plan).await;
        self.finish_execution(operation_id, result)
    }

    fn finish_execution(
        &self,
        operation_id: OperationId,
        result: Result<TargetBootstrapResult, MmError>,
    ) -> Result<TargetBootstrapResult, MmError> {
        match result {
            Ok(result) => {
                if let Err(error) = self
                    .ports
                    .state
                    .save_target_bootstrap_result(operation_id, &result)
                {
                    if let Err(journal) =
                        self.finish(operation_id, OperationState::Failed, Some(error.code()))
                    {
                        return Err(MmError::operation_failure(
                            operation_id.0.to_string(),
                            crate::domain::error::MutationStatus::Applied,
                            error,
                            vec![journal.to_string()],
                            "bootstrap mutation completed; repair its result journal before retrying",
                        ));
                    }
                    return Err(error);
                }
                self.finish(operation_id, OperationState::Succeeded, None)?;
                Ok(result)
            }
            Err(error) => {
                if let Err(journal_error) =
                    self.finish(operation_id, OperationState::Failed, Some(error.code()))
                {
                    return Err(MmError::operation_failure(
                        operation_id.0.to_string(),
                        crate::domain::error::MutationStatus::Unknown,
                        error,
                        vec![journal_error.to_string()],
                        "inspect target state and repair the bootstrap journal before retrying",
                    ));
                }
                Err(error)
            }
        }
    }

    async fn execute(
        &self,
        operation_id: OperationId,
        project_root: &std::path::Path,
        plan: &TargetBootstrapPlan,
    ) -> Result<TargetBootstrapResult, MmError> {
        self.record_phase(operation_id, OperationPhase::Validate)?;
        self.record_phase(operation_id, OperationPhase::Apply)?;
        let execution = self.ports.executor.execute(project_root, plan).await?;
        self.record_phase(operation_id, OperationPhase::Verify)?;
        if execution.capabilities != plan.required_capabilities {
            return Err(MmError::Security {
                message: "bootstrap executor did not verify every reviewed capability".to_owned(),
            });
        }
        if plan.access.is_some() != execution.access.is_some() {
            return Err(MmError::Security {
                message: "bootstrap executor did not report the reviewed remote access layer"
                    .to_owned(),
            });
        }
        if let Some(report) = &execution.access {
            report
                .validate()
                .map_err(|message| MmError::Security { message })?;
        }
        let next_command = if execution.access.is_some() {
            format!("mm --environment {} team sync", plan.environment)
        } else {
            format!("mm --environment {} deploy plan", plan.environment)
        };
        Ok(TargetBootstrapResult {
            operation_id,
            plan_hash: plan.plan_hash.clone(),
            target: plan.target.name.clone(),
            target_id: plan.target.definition.id,
            verified_capabilities: execution.capabilities,
            next_command,
            access: execution.access,
        })
    }

    fn record_phase(
        &self,
        operation_id: OperationId,
        phase: OperationPhase,
    ) -> Result<(), MmError> {
        self.ports.state.append_event(
            operation_id,
            self.ports.clock.now_unix_ms(),
            &OperationEvent::PhaseChanged {
                operation_id,
                phase,
            },
        )
    }

    fn finish(
        &self,
        operation_id: OperationId,
        state: OperationState,
        error_code: Option<&str>,
    ) -> Result<(), MmError> {
        self.ports.state.finish_operation(
            operation_id,
            state,
            self.ports.clock.now_unix_ms(),
            error_code,
        )
    }
}

fn require_plan_hash(plan: &TargetBootstrapPlan, expected: &str) -> Result<(), MmError> {
    if plan.plan_hash == expected {
        Ok(())
    } else {
        Err(MmError::State {
            message: format!(
                "target bootstrap plan changed (expected {expected}, current {}); review the new plan",
                plan.plan_hash
            ),
        })
    }
}
