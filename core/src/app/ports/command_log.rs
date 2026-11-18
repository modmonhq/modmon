use crate::domain::command_log::{CommandFilter, CommandRecord, CommandSummary};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

/// Where the commands of each project on this machine are kept.
pub trait CommandLog: Send + Sync {
    fn append(&self, project: ProjectId, record: &CommandRecord) -> Result<(), MmError>;
    /// The newest records first, from before `cursor` when given; returns
    /// the cursor of the next page.
    fn list(
        &self,
        project: ProjectId,
        filter: &CommandFilter,
        page: (Option<&str>, usize),
    ) -> Result<(Vec<CommandSummary>, Option<String>), MmError>;
    fn get(&self, project: ProjectId, id: &str) -> Result<Option<CommandRecord>, MmError>;
}
