use std::sync::Arc;

use crate::app::ports::StateStore;
use crate::domain::error::MmError;

/// Opens existing state for inspection without initializing or migrating it.
pub trait StateInspector: Send + Sync {
    fn inspect(&self) -> Result<Option<Arc<dyn StateStore>>, MmError>;
}

pub trait StateRecoveryStore: Send + Sync {
    fn plan(&self) -> Result<crate::domain::readiness::StateRecoveryPlan, MmError>;
    fn recover(&self, expected_plan: &str) -> Result<std::path::PathBuf, MmError>;
}

/// Optional read-only checks run even when another prerequisite fails. A probe
/// can add checks without changing startup, doctor, or the recovery presenter.
#[async_trait::async_trait]
pub trait ReadinessProbe: Send + Sync {
    fn name(&self) -> &'static str;
    fn dependencies(&self) -> &'static [&'static str] {
        &[]
    }
    async fn inspect(
        &self,
        selection: &crate::app::WorkspaceSelection,
    ) -> Result<Vec<crate::domain::diagnostic::Diagnostic>, MmError>;
}
