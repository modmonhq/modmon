use std::io::Write;

use super::*;
use crate::domain::development_units::DevelopmentChange;

/// What `mm dev enable` or `mm dev disable` changed, and what to run next.
pub(crate) fn write_development_change(
    change: &DevelopmentChange,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(change);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for line in development_change_lines(change) {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    Ok(())
}

#[must_use]
pub fn development_change_lines(change: &DevelopmentChange) -> Vec<String> {
    let units = change.units.join(", ");
    let state = if change.enable { "on" } else { "off" };
    let mut lines = vec![format!(
        "{units}: {state} in {} on this machine",
        change.environment
    )];
    if !change.started.is_empty() {
        lines.push(format!("  runs now: {}", change.started.join(", ")));
    }
    if !change.stopped.is_empty() {
        lines.push(format!("  off now: {}", change.stopped.join(", ")));
    }
    lines.extend(change.notes.iter().map(|note| format!("  note: {note}")));
    if change.enable && !change.started.is_empty() {
        lines.push("Run `mm dev up` to start it.".to_owned());
    }
    lines
}

/// Before the deployment that stops units `mm dev disable` turned off.
pub(crate) fn write_development_stopping(services: &[String]) -> Result<(), MmError> {
    let stderr = std::io::stderr();
    let mut writer = stderr.lock();
    writeln!(
        writer,
        "==> {} still running; the development deployment stops it and removes its routes",
        terminal_safe(&services.join(", "))
    )
    .map_err(output_error)
}
