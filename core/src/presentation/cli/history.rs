//! `mm history`: the commands run in this project on this machine.

use std::io::Write;

use super::{Cli, OutputFormat};
use crate::domain::command_log::{
    CommandFilter, CommandOutcome, CommandPrincipal, CommandRecord, CommandSummary,
};
use crate::domain::error::MmError;
use crate::presentation::output::terminal_safe;
use crate::presentation::record;

#[derive(Debug, Clone, clap::Args)]
pub struct HistoryArgs {
    #[command(subcommand)]
    pub command: Option<HistoryCommand>,
    /// Only commands that ended this way: succeeded, failed or interrupted.
    #[arg(long, value_name = "OUTCOME")]
    pub outcome: Option<String>,
    /// Only commands that started within this long, such as 30m, 2h or 3d.
    #[arg(long, value_name = "AGE")]
    pub since: Option<String>,
    /// How many commands to list.
    #[arg(long, value_name = "N", default_value_t = 20)]
    pub limit: usize,
    /// List the commands that started before this one.
    #[arg(long, value_name = "ID")]
    pub before: Option<String>,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum HistoryCommand {
    /// Show one command: what it printed, its steps and its result.
    Show {
        /// The command's ID from `mm history`, or its first characters.
        #[arg(value_name = "ID")]
        id: String,
    },
}

pub(super) fn run(cli: &Cli, args: &HistoryArgs) -> Result<u8, MmError> {
    let start = match &cli.root {
        Some(root) => root.clone(),
        None => super::terminal::current_directory()?,
    };
    let history = crate::composition::command_history(&start, cli.mm_home.as_deref())?;
    if let Some(HistoryCommand::Show { id }) = &args.command {
        let record = history.get(id)?;
        return write_record(&record, cli.output).map(|()| 0);
    }
    let filter = CommandFilter {
        environment: cli.environment.clone(),
        outcome: args.outcome.as_deref().map(outcome).transpose()?,
        since_unix_ms: args
            .since
            .as_deref()
            .map(|since| age(since).map(|age| now_ms() - age))
            .transpose()?,
    };
    let (commands, next) = history.list(&filter, args.before.as_deref(), args.limit)?;
    write_list(&commands, next.as_deref(), cli.output).map(|()| 0)
}

fn outcome(text: &str) -> Result<CommandOutcome, MmError> {
    match text {
        "succeeded" => Ok(CommandOutcome::Succeeded),
        "failed" => Ok(CommandOutcome::Failed),
        "interrupted" => Ok(CommandOutcome::Interrupted),
        other => Err(MmError::Environment {
            message: format!("--outcome is succeeded, failed or interrupted, not '{other}'"),
        }),
    }
}

/// `30m`, `2h`, `3d` in milliseconds.
fn age(text: &str) -> Result<i64, MmError> {
    let invalid = || MmError::Environment {
        message: format!("--since takes an age such as 30m, 2h or 3d, not '{text}'"),
    };
    let split = text.len().checked_sub(1).ok_or_else(invalid)?;
    let (amount, unit) = text.split_at(split);
    let amount = amount.parse::<i64>().map_err(|_| invalid())?;
    let unit_ms = match unit {
        "s" => 1_000,
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        _ => return Err(invalid()),
    };
    amount.checked_mul(unit_ms).ok_or_else(invalid)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

fn outcome_word(outcome: CommandOutcome) -> &'static str {
    match outcome {
        CommandOutcome::Succeeded => "ok",
        CommandOutcome::Failed => "failed",
        CommandOutcome::Interrupted => "interrupted",
    }
}

fn who(principal: &CommandPrincipal) -> String {
    match principal {
        CommandPrincipal::User { name } => name.clone(),
        CommandPrincipal::Agent { name } => format!("agent {name}"),
    }
}

fn duration(ms: u64) -> String {
    crate::presentation::inline::duration(std::time::Duration::from_millis(ms))
}

fn write_list(
    commands: &[CommandSummary],
    next: Option<&str>,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return crate::presentation::output::write_json(
            &serde_json::json!({"commands": commands, "next": next}),
        );
    }
    let mut out = record::stdout();
    let mut text = String::new();
    if commands.is_empty() {
        text.push_str("No command recorded yet.\n");
    }
    for command in commands {
        text.push_str(&format!(
            "{}  {:<11} {:>7}  {:<12} mm {}\n",
            command.id,
            outcome_word(command.outcome),
            duration(command.duration_ms),
            command.environment.as_deref().unwrap_or("-"),
            terminal_safe(&command.arguments.join(" "))
        ));
    }
    if let Some(next) = next {
        text.push_str(&format!("More: mm history --before {next}\n"));
    }
    out.write_all(text.as_bytes()).map_err(output_error)
}

fn write_record(record: &CommandRecord, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return crate::presentation::output::write_json(record);
    }
    let mut text = format!(
        "mm {}\n  started   {} by {}\n  took      {}\n  outcome   {}{}\n",
        terminal_safe(&record.arguments.join(" ")),
        crate::domain::rollout::utc_time(record.started_at_unix_ms),
        terminal_safe(&who(&record.principal)),
        duration(record.summary().duration_ms),
        outcome_word(record.outcome),
        match (record.exit_code, &record.error_code) {
            (Some(code), Some(error)) => format!(" (exit {code}, {error})"),
            (Some(code), None) => format!(" (exit {code})"),
            (None, _) => String::new(),
        }
    );
    if let Some(environment) = &record.environment {
        text.push_str(&format!("  env       {}\n", terminal_safe(environment)));
    }
    text.push('\n');
    for line in record.output.lines() {
        text.push_str(&terminal_safe(line));
        text.push('\n');
    }
    if record.output_truncated {
        text.push_str("[output cut: the log keeps 256 KiB of each command]\n");
    }
    record::stdout()
        .write_all(text.as_bytes())
        .map_err(output_error)
}

fn output_error(source: std::io::Error) -> MmError {
    MmError::Io {
        context: "writing command output".to_owned(),
        source,
    }
}
