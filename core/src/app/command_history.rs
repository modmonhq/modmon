//! The commands run in a project on this machine, for people and agents.

use std::sync::Arc;

use crate::app::ports::CommandLog;
use crate::domain::command_log::{CommandFilter, CommandRecord, CommandSummary};
use crate::domain::error::MmError;
use crate::domain::workspace::ProjectId;

/// The most records one page lists.
pub const MAX_PAGE: usize = 100;

/// Records, lists and reads one project's commands.
#[derive(Clone)]
pub struct CommandHistory {
    log: Arc<dyn CommandLog>,
    project: ProjectId,
}

impl std::fmt::Debug for CommandHistory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommandHistory")
            .field("project", &self.project)
            .finish_non_exhaustive()
    }
}

impl CommandHistory {
    #[must_use]
    pub fn new(log: Arc<dyn CommandLog>, project: ProjectId) -> Self {
        Self { log, project }
    }

    /// Keeps `record`, redacted and bounded.
    pub fn record(&self, record: CommandRecord) -> Result<(), MmError> {
        self.log.append(self.project, &record.bounded())
    }

    pub fn list(
        &self,
        filter: &CommandFilter,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<CommandSummary>, Option<String>), MmError> {
        self.log
            .list(self.project, filter, (cursor, limit.clamp(1, MAX_PAGE)))
    }

    /// One record, found by its ID or a unique start of it.
    pub fn get(&self, id: &str) -> Result<CommandRecord, MmError> {
        if let Some(record) = self.log.get(self.project, id)? {
            return Ok(record);
        }
        Err(MmError::Environment {
            message: format!(
                "no command '{id}' in this project's history; `mm history` lists them"
            ),
        })
    }
}
