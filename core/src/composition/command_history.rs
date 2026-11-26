//! The command log of a project on this machine, and the recording of the
//! command this process runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::command_log::FileCommandLog;
use crate::adapters::config::ConfigLoader;
use crate::adapters::state::MmHome;
use crate::app::CommandHistory;
use crate::domain::command_log::{CommandOutcome, CommandRecord, redact_arguments};
use crate::domain::error::MmError;
use crate::presentation::cli::{Cli, Command, OutputFormat};
use crate::presentation::record;

/// The command log of the project at `start`.
pub fn command_history(
    start: &Path,
    explicit_home: Option<&Path>,
) -> Result<CommandHistory, MmError> {
    let document = ConfigLoader.load_from(start)?;
    let home = MmHome::inspect(explicit_home)?;
    Ok(CommandHistory::new(
        Arc::new(FileCommandLog::new(home.root.join("history"))),
        document.config.project.id,
    ))
}

/// The command this process runs, recorded once it ends.
#[derive(Debug)]
pub struct CommandRecording {
    start: PathBuf,
    home: Option<PathBuf>,
    arguments: Vec<String>,
    environment: Option<String>,
    target: Option<String>,
    runtime: Option<String>,
}

impl CommandRecording {
    /// Starts recording `cli`; `None` for the terminal UI, the MCP server,
    /// reading the log itself and internal commands.
    #[must_use]
    pub fn start(cli: &Cli) -> Option<Self> {
        if matches!(
            cli.command,
            None | Some(
                Command::Mcp { .. }
                    | Command::History { .. }
                    | Command::Relay { .. }
                    | Command::Generate { .. }
                    | Command::DashboardSchema
                    | Command::InstallIngress { .. }
                    | Command::UninstallIngress { .. }
            )
        ) || std::env::var_os("MM_NO_HISTORY").is_some_and(|value| !value.is_empty())
        {
            return None;
        }
        let start = match &cli.root {
            Some(root) => root.clone(),
            None => std::env::current_dir().ok()?,
        };
        record::begin(cli.output == OutputFormat::Json);
        Some(Self {
            start,
            home: cli.mm_home.clone(),
            arguments: redact_arguments(&std::env::args().skip(1).collect::<Vec<_>>()),
            environment: cli.environment.clone(),
            target: cli.target.clone(),
            runtime: cli.runtime.map(|runtime| {
                crate::domain::runtime::RuntimeKind::from(runtime)
                    .name()
                    .to_owned()
            }),
        })
    }

    /// Keeps the record: `exit` is `None` for an interrupted command. A
    /// command outside a project leaves none. Never fails the command.
    pub fn finish(self, exit: Option<u8>, error: Option<&MmError>) {
        let Some(recorded) = record::finish() else {
            return;
        };
        let Ok(document) = ConfigLoader.load_from(&self.start) else {
            return;
        };
        let environment =
            crate::domain::config::resolve_environment(&document, self.environment.as_deref()).ok();
        let mut output = recorded.output;
        if let Some(error) = error {
            output.push_str(&crate::presentation::output::error_line(error));
            output.push('\n');
        }
        let outcome = match (exit, error) {
            (None, _) => CommandOutcome::Interrupted,
            (Some(0), None) => CommandOutcome::Succeeded,
            _ => CommandOutcome::Failed,
        };
        let record = CommandRecord {
            id: record_id(recorded.started_unix_ms),
            arguments: self.arguments,
            environment,
            target: self.target,
            runtime: self.runtime,
            principal: super::user_principal(self.home.as_deref(), &document.config.users),
            started_at_unix_ms: recorded.started_unix_ms,
            finished_at_unix_ms: unix_ms(),
            outcome,
            exit_code: exit,
            error_code: error.map(|error| error.code().to_owned()),
            output,
            output_truncated: recorded.truncated,
            events: recorded.events,
            result: recorded.result,
        };
        let Ok(home) = MmHome::inspect(self.home.as_deref()) else {
            return;
        };
        if !holds_state(&home.root) {
            return;
        }
        let history = CommandHistory::new(
            Arc::new(FileCommandLog::new(home.root.join("history"))),
            document.config.project.id,
        );
        if let Err(error) = history.record(record) {
            tracing::warn!(code = error.code(), %error, "the command log did not keep this command");
        }
    }
}

/// A command never creates the mm home just to be recorded: a read-only
/// command leaves an empty or missing home as it found it.
fn holds_state(root: &Path) -> bool {
    std::fs::read_dir(root).is_ok_and(|mut entries| entries.next().is_some())
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// `20261115T201530123Z-4f2a`: sorts by start time, and two commands that
/// start in the same millisecond differ.
fn record_id(started_unix_ms: i64) -> String {
    let time = chrono::DateTime::from_timestamp_millis(started_unix_ms).unwrap_or_default();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    format!(
        "{}-{}",
        time.format("%Y%m%dT%H%M%S%3fZ"),
        suffix.get(..4).unwrap_or("0000")
    )
}
