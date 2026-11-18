//! The stable library surface contains [`Application`], its request and
//! response types, and [`MmError`]. The `unstable` feature exposes internal
//! modules for integration testing and is not covered by compatibility
//! guarantees. Source builds of the `mm` binary explicitly select that feature.

#[cfg(feature = "unstable")]
pub mod adapters;
#[cfg(all(test, not(feature = "unstable")))]
mod adapters;
#[cfg(not(feature = "unstable"))]
mod app;
#[cfg(feature = "unstable")]
pub mod app;
#[cfg(feature = "unstable")]
#[doc(hidden)]
pub mod composition;
#[cfg(not(feature = "unstable"))]
use mm_domain::domain;
#[cfg(feature = "unstable")]
pub use mm_domain::domain;
#[cfg(feature = "unstable")]
pub mod presentation;

pub use app::{Application, JobSchedulerService, RuntimeOperationsService, WorkspaceSelection};
pub use domain::error::{MmError, MutationStatus};
pub use domain::observability::{
    ApprovalContext, ExecEnvironmentReference, ExecEvent, ExecOutcome, ExecRequest, ExecStdinMode,
    ExecTtyMode, LogLimits, LogMode, LogOutputPolicy, LogRecord, LogRequest, OperationCancellation,
    ServiceSelector,
};
pub use domain::runtime::RuntimeKind;
pub use domain::scheduler::{JobRun, JobRunState, ReconciliationPlan, ScheduleRecord};
pub use domain::workspace::{
    DoctorReport, InitializedProject, ValidationSnapshot, WorkspaceSnapshot,
};

#[cfg(test)]
pub(crate) fn test_project_documents() -> std::sync::Arc<dyn app::ports::ProjectDocumentStore> {
    std::sync::Arc::new(adapters::config::ConfigLoader)
}

#[cfg(test)]
pub(crate) mod test_support;
