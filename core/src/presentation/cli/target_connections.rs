//! `mm target connections`: the SSH connections this device keeps open to
//! remote hosts between commands.

use super::OutputFormat;
use crate::composition;
use crate::domain::error::MmError;
use crate::domain::key::KeptConnection;
use crate::presentation::output;

pub(super) fn run(close: bool, invocation: &super::dispatch::Invocation) -> Result<u8, MmError> {
    let kept = if close {
        composition::close_all_kept_connections(invocation.home())?
    } else {
        composition::kept_connections(invocation.home())?
    };
    if invocation.format == OutputFormat::Json {
        return output::write_json(&kept).map(|()| 0);
    }
    if kept.is_empty() {
        crate::outln!("No connection is kept open.");
        return Ok(0);
    }
    crate::outln!(
        "{}",
        if close {
            "Closed:"
        } else {
            "Kept open (each closes after 5 minutes without a command):"
        }
    );
    for connection in &kept {
        crate::outln!("  {}", output::terminal_safe(&line(connection)));
    }
    if !close {
        crate::outln!("Close them: mm target connections --close");
    }
    Ok(0)
}

fn line(connection: &KeptConnection) -> String {
    let peer = connection.peer.as_deref().unwrap_or("unknown host");
    let key = connection
        .key
        .as_ref()
        .map_or_else(|| "unknown key".to_owned(), |key| key.display().to_string());
    let age = connection.age_seconds.map_or_else(String::new, |seconds| {
        if seconds < 120 {
            format!(", opened {seconds} s ago")
        } else {
            format!(", opened {} min ago", seconds / 60)
        }
    });
    let state = if connection.open {
        ""
    } else {
        ", no longer answering"
    };
    format!("{peer}  key {key}{age}{state}")
}
