//! `mm_read` kind `commands`: the project's command log, as `mm history`
//! reads it.

use std::path::Path;

use crate::domain::command_log::CommandFilter;
use crate::domain::error::MmError;
use crate::domain::mcp::MCP_MAX_RESPONSE_BYTES;

/// A page of commands, or one command whole. A result too large for an
/// answer is left out and said so.
pub(crate) fn read(
    project_root: &Path,
    mm_home: &Path,
    (command, filter, cursor, limit): (Option<&str>, Option<&CommandFilter>, Option<&str>, usize),
) -> Result<serde_json::Value, MmError> {
    let history = super::command_history(project_root, Some(mm_home))?;
    if let Some(id) = command {
        let mut record = history.get(id)?;
        let result_bytes = record.result.as_ref().map_or(0, |result| {
            serde_json::to_vec(result).map_or(usize::MAX, |bytes| bytes.len())
        });
        let omitted = result_bytes > MCP_MAX_RESPONSE_BYTES / 4;
        if omitted {
            record.result = None;
        }
        return Ok(serde_json::json!({ "command": record, "result-omitted": omitted }));
    }
    let (commands, next) = history.list(&filter.cloned().unwrap_or_default(), cursor, limit)?;
    Ok(serde_json::json!({ "commands": commands, "next": next }))
}

/// A page of commands, unfiltered.
pub(crate) fn page(
    context: &crate::domain::mcp::McpRequestContext,
    mm_home: &Path,
    (cursor, limit): (Option<String>, usize),
) -> Result<serde_json::Value, MmError> {
    read(
        &context.project_root,
        mm_home,
        (None, None, cursor.as_deref(), limit),
    )
}

/// The MCP audit journal, which also lists each finished agent mutation in
/// the project's command log, pointing at its audit entry.
pub(crate) struct RecordingAudit {
    inner: std::sync::Arc<dyn crate::app::ports::McpAuditJournal>,
    mm_home: std::path::PathBuf,
    started: std::sync::Mutex<std::collections::HashMap<uuid::Uuid, i64>>,
}

impl RecordingAudit {
    pub(crate) fn new(
        inner: std::sync::Arc<dyn crate::app::ports::McpAuditJournal>,
        mm_home: std::path::PathBuf,
    ) -> Self {
        Self {
            inner,
            mm_home,
            started: std::sync::Mutex::default(),
        }
    }

    fn record(&self, entry: &crate::domain::mcp::McpAuditRecord) -> Result<(), MmError> {
        use crate::domain::command_log::{CommandOutcome, CommandRecord};
        let started = self
            .started
            .lock()
            .ok()
            .and_then(|mut started| started.remove(&entry.request_id))
            .unwrap_or(entry.occurred_at_unix_ms);
        let succeeded = entry.outcome_code == "OK";
        let time = chrono::DateTime::from_timestamp_millis(started).unwrap_or_default();
        let history = super::command_history(&entry.project_root, Some(&self.mm_home))?;
        let users = crate::adapters::config::ConfigLoader
            .load_from(&entry.project_root)
            .map(|document| document.config.users)
            .unwrap_or_default();
        history.record(CommandRecord {
            id: format!(
                "{}-{}",
                time.format("%Y%m%dT%H%M%S%3fZ"),
                entry.id.simple().to_string().get(..4).unwrap_or("0000")
            ),
            arguments: vec!["mcp".to_owned(), entry.operation.clone()],
            environment: Some(entry.environment.clone()),
            target: (!entry.target.is_empty()).then(|| entry.target.clone()),
            runtime: None,
            principal: super::agent_principal(&entry.principal, Some(&self.mm_home), &users),
            started_at_unix_ms: started,
            finished_at_unix_ms: entry.occurred_at_unix_ms,
            outcome: if succeeded {
                CommandOutcome::Succeeded
            } else {
                CommandOutcome::Failed
            },
            exit_code: None,
            error_code: (!succeeded).then(|| entry.outcome_code.clone()),
            output: format!(
                "{} by agent {} through MCP: {}{}\nMCP audit entry {}\n",
                entry.operation,
                entry.principal,
                entry.outcome_code,
                entry
                    .plan_hash
                    .as_deref()
                    .map(|plan| format!(", plan {plan}"))
                    .unwrap_or_default(),
                entry.id
            ),
            output_truncated: false,
            events: Vec::new(),
            result: None,
        })
    }
}

impl crate::app::ports::McpAuditJournal for RecordingAudit {
    fn append_mcp_audit(&self, entry: &crate::domain::mcp::McpAuditRecord) -> Result<(), MmError> {
        self.inner.append_mcp_audit(entry)?;
        if !entry.mutation {
            return Ok(());
        }
        if entry.outcome_code == "STARTED" {
            if let Ok(mut started) = self.started.lock() {
                started.insert(entry.request_id, entry.occurred_at_unix_ms);
            }
            return Ok(());
        }
        if let Err(error) = self.record(entry) {
            tracing::warn!(code = error.code(), %error, "the command log did not keep an agent's operation");
        }
        Ok(())
    }
}
