//! `mm workspace`: the project folders this machine's state knows, which of
//! them are gone, and forgetting them through a reviewed plan.

use std::path::Path;
use std::sync::Arc;

use crate::app::ports::{ReadinessProbe, StateInspector, StateStore};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::workspace::WorkspaceRecord;
use crate::domain::workspace_registry::{
    FORGET_STALE_COMMAND, RegisteredWorkspace, WorkspaceForgetPlan, WorkspaceForgetResult,
    WorkspacePresence, WorkspaceSelector,
};

const STALE_WORKSPACE_CODE: &str = "MM_WORKSPACE_STALE";

fn workspace_presence(workspace: &WorkspaceRecord) -> WorkspacePresence {
    if gone(&workspace.root) {
        WorkspacePresence::RootMissing
    } else if gone(&workspace.config_path) {
        WorkspacePresence::ConfigMissing
    } else {
        WorkspacePresence::Present
    }
}

fn gone(path: &Path) -> bool {
    matches!(path.try_exists(), Ok(false))
}

/// At most `limit` registered workspaces, the most recently used first.
pub fn registered_workspaces(
    state: &dyn StateStore,
    limit: usize,
) -> Result<Vec<RegisteredWorkspace>, MmError> {
    Ok(state
        .workspace_usage(limit)?
        .into_iter()
        .map(|usage| {
            let presence = workspace_presence(&usage.workspace);
            RegisteredWorkspace::new(usage, presence)
        })
        .collect())
}

pub struct WorkspaceRegistry {
    state: Option<Arc<dyn StateStore>>,
}

impl WorkspaceRegistry {
    /// `state` is `None` on a machine where mm has kept no state yet.
    #[must_use]
    pub fn new(state: Option<Arc<dyn StateStore>>) -> Self {
        Self { state }
    }

    pub fn list(&self) -> Result<Vec<RegisteredWorkspace>, MmError> {
        self.state.as_deref().map_or_else(
            || Ok(Vec::new()),
            |state| registered_workspaces(state, usize::MAX),
        )
    }

    pub fn forget_plan(
        &self,
        selector: &WorkspaceSelector,
    ) -> Result<WorkspaceForgetPlan, MmError> {
        let selected = self
            .list()?
            .into_iter()
            .filter(|workspace| selector.names(workspace))
            .collect::<Vec<_>>();
        if let WorkspaceSelector::One { given, .. } = selector {
            one_named(given, &selected)?;
        }
        Ok(WorkspaceForgetPlan::new(selected))
    }

    pub fn forget(
        &self,
        selector: &WorkspaceSelector,
        reviewed: &str,
    ) -> Result<WorkspaceForgetResult, MmError> {
        let plan = self.forget_plan(selector)?;
        plan.check(reviewed, &selector.command())
            .map_err(|message| MmError::Security { message })?;
        let operations = match &self.state {
            Some(state) if !plan.workspaces.is_empty() => state.forget_workspaces(
                &plan
                    .workspaces
                    .iter()
                    .map(RegisteredWorkspace::usage)
                    .collect::<Vec<_>>(),
            )?,
            _ => 0,
        };
        Ok(WorkspaceForgetResult {
            forgotten: plan.workspaces,
            operations,
        })
    }
}

fn one_named(given: &str, selected: &[RegisteredWorkspace]) -> Result<(), MmError> {
    match selected {
        [_] => Ok(()),
        [] => Err(MmError::Environment {
            message: format!(
                "no registered workspace has the root or ID '{given}'; `mm workspace list` lists them"
            ),
        }),
        several => Err(MmError::Environment {
            message: format!(
                "'{given}' names {} registered workspaces ({}); name one by its root, or run `{FORGET_STALE_COMMAND}` to forget the ones that are gone",
                several.len(),
                several
                    .iter()
                    .map(|workspace| workspace.root.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}

fn stale_workspace_diagnostic(workspaces: &[RegisteredWorkspace]) -> Option<Diagnostic> {
    const SHOWN: usize = 3;
    let stale = workspaces
        .iter()
        .filter(|workspace| workspace.stale)
        .map(|workspace| workspace.root.display().to_string())
        .collect::<Vec<_>>();
    let message = match stale.as_slice() {
        [] => return None,
        [root] => format!("the registered project folder {root} is gone"),
        roots => format!(
            "{} registered project folders are gone: {}{}",
            roots.len(),
            roots
                .iter()
                .take(SHOWN)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            if roots.len() > SHOWN {
                format!(" and {} more", roots.len() - SHOWN)
            } else {
                String::new()
            }
        ),
    };
    Some(
        Diagnostic::warning(STALE_WORKSPACE_CODE, message).with_help(format!(
            "`{FORGET_STALE_COMMAND}` forgets the registered folders that are gone, with their operation history on this machine; `mm workspace list` lists every registered folder"
        )),
    )
}

/// `mm doctor`'s check of the registered workspaces.
pub struct RegisteredWorkspacesCheck {
    state: Arc<dyn StateInspector>,
}

impl RegisteredWorkspacesCheck {
    #[must_use]
    pub fn new(state: Arc<dyn StateInspector>) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl ReadinessProbe for RegisteredWorkspacesCheck {
    fn name(&self) -> &'static str {
        "workspaces"
    }

    async fn inspect(
        &self,
        _selection: &crate::app::WorkspaceSelection,
    ) -> Result<Vec<Diagnostic>, MmError> {
        let workspaces = WorkspaceRegistry::new(self.state.inspect()?).list()?;
        Ok(stale_workspace_diagnostic(&workspaces)
            .into_iter()
            .collect())
    }
}
