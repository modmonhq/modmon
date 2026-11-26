//! When each secret's revision was written and by whom, as `mm secret
//! status` and the terminal UI show it.

use super::*;
use crate::domain::secret::SecretBindingStatus;

const MISSING: &str = "—";
const BY_WIDTH: usize = 20;
const KEY_WIDTH: usize = 28;
const SUMMARY_BY_WIDTH: usize = 48;

/// `3 days ago (2026-11-21)`, or `—` when the provider reports no time.
pub(crate) fn set_when(set_at_unix_ms: Option<i64>, now_unix_ms: i64) -> String {
    let Some(at) = set_at_unix_ms else {
        return MISSING.to_owned();
    };
    let date = chrono::DateTime::from_timestamp_millis(at)
        .map_or_else(String::new, |time| time.format("%Y-%m-%d").to_string());
    format!("{} ({date})", ago((now_unix_ms - at).max(0) / 1_000))
}

/// Who wrote the revision, cut to `width` characters, or `—`.
pub(crate) fn set_by_label(set_by: Option<&str>, width: usize) -> String {
    let Some(set_by) = set_by else {
        return MISSING.to_owned();
    };
    let safe = terminal_safe(set_by);
    if safe.chars().count() <= width {
        return safe;
    }
    let mut cut = safe
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>();
    cut.push('…');
    cut
}

/// `3 days ago (2026-11-21) by alex`, with whichever half is known.
pub(crate) fn set_summary(
    set_at_unix_ms: Option<i64>,
    set_by: Option<&str>,
    now_unix_ms: i64,
) -> String {
    match (set_at_unix_ms, set_by) {
        (None, None) => MISSING.to_owned(),
        (Some(_), None) => set_when(set_at_unix_ms, now_unix_ms),
        (None, Some(_)) => format!("by {}", set_by_label(set_by, SUMMARY_BY_WIDTH)),
        (Some(_), Some(_)) => format!(
            "{} by {}",
            set_when(set_at_unix_ms, now_unix_ms),
            set_by_label(set_by, SUMMARY_BY_WIDTH)
        ),
    }
}

fn ago(seconds: i64) -> String {
    let (count, unit) = match seconds {
        ..60 => return "just now".to_owned(),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        86_400..2_592_000 => (seconds / 86_400, "day"),
        2_592_000..31_536_000 => (seconds / 2_592_000, "month"),
        _ => (seconds / 31_536_000, "year"),
    };
    format!("{count} {unit}{} ago", if count == 1 { "" } else { "s" })
}

/// The secrets table: the key is never cut, the writer is.
pub(crate) fn write_secret_rows(
    writer: &mut impl Write,
    secrets: &[SecretBindingStatus],
    now_unix_ms: i64,
) -> Result<(), MmError> {
    let rows = secrets
        .iter()
        .map(|secret| {
            [
                terminal_safe(&secret.key),
                terminal_safe(&secret.provider),
                secret_binding_status(secret).to_owned(),
                set_when(secret.set_at_unix_ms, now_unix_ms),
                set_by_label(secret.set_by.as_deref(), BY_WIDTH),
                terminal_safe(secret.location.as_deref().unwrap_or("-")),
            ]
        })
        .collect::<Vec<_>>();
    let header = ["KEY", "PROVIDER", "STATUS", "SET", "BY", "LOCATION"];
    let mut widths = header.map(|title| title.chars().count());
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    widths[0] = widths[0].min(KEY_WIDTH);
    let line = |cells: [&str; 6]| {
        let mut line = String::new();
        for (cell, width) in cells.iter().zip(widths).take(5) {
            line.push_str(&format!("  {cell:<width$}"));
        }
        line.push_str("  ");
        line.push_str(cells[5]);
        line
    };
    writeln!(writer, "{}", line(header)).map_err(output_error)?;
    for row in &rows {
        writeln!(writer, "{}", line(row.each_ref().map(String::as_str))).map_err(output_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_an_age_and_a_date_and_writers_are_cut() {
        let day = 86_400_000;
        let now = 1_795_262_400_000;
        assert_eq!(set_when(None, now), "—");
        assert_eq!(set_when(Some(now + 5_000), now), "just now (2026-11-21)");
        assert_eq!(
            set_when(Some(now - 60_000), now),
            "1 minute ago (2026-11-21)"
        );
        assert_eq!(
            set_when(Some(now - 3 * day), now),
            "3 days ago (2026-11-18)"
        );
        assert_eq!(
            set_when(Some(now - 400 * day), now),
            "1 year ago (2025-10-17)"
        );
        assert_eq!(set_by_label(None, 8), "—");
        assert_eq!(set_by_label(Some("alex"), 8), "alex");
        assert_eq!(set_by_label(Some("agent:claude (alex)"), 8), "agent:c…");
        assert_eq!(set_summary(None, None, now), "—");
        assert_eq!(
            set_summary(Some(now - 2 * 3_600_000), Some("alex"), now),
            "2 hours ago (2026-11-21) by alex"
        );
    }
}
