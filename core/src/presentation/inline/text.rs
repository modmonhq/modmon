//! Fitting text to a width, and the short forms of times, sizes and ids.

use std::time::Duration;

use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// The ellipsis that ends a value cut to fit.
pub const ELLIPSIS: char = '…';

/// Columns `text` takes on a terminal.
#[must_use]
pub fn width(text: &str) -> usize {
    text.chars()
        .map(|character| character.width().unwrap_or(0))
        .sum()
}

/// `text` cut to at most `max` columns, ending in an ellipsis when cut.
#[must_use]
pub fn clip(text: &str, max: usize) -> String {
    let text = crate::presentation::output::terminal_safe(text);
    if width(&text) <= max {
        return text;
    }
    if max == 0 {
        return String::new();
    }
    let mut kept = String::new();
    let mut used = 0;
    for character in text.chars() {
        let columns = character.width().unwrap_or(0);
        if used + columns > max - 1 {
            break;
        }
        kept.push(character);
        used += columns;
    }
    kept.push(ELLIPSIS);
    kept
}

/// `text` padded with spaces to `columns`, or cut to it.
#[must_use]
pub fn pad(text: &str, columns: usize) -> String {
    let clipped = clip(text, columns);
    let missing = columns.saturating_sub(width(&clipped));
    format!("{clipped}{}", " ".repeat(missing))
}

/// Spans cut so that together they take at most `max` columns.
#[must_use]
pub fn fit(spans: Vec<Span<'static>>, max: usize) -> Vec<Span<'static>> {
    let mut used = 0;
    let mut fitted = Vec::with_capacity(spans.len());
    for span in spans {
        let content = crate::presentation::output::terminal_safe(&span.content);
        let columns = width(&content);
        if used + columns <= max {
            used += columns;
            fitted.push(Span::styled(content, span.style));
            continue;
        }
        let room = max.saturating_sub(used);
        if room > 0 {
            fitted.push(Span::styled(clip(&content, room), span.style));
        }
        break;
    }
    fitted
}

/// One row of `width` columns.
#[must_use]
pub fn row(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let right_width: usize = right.iter().map(|span| self::width(&span.content)).sum();
    if right_width == 0 || right_width + 2 > width / 2 {
        return Line::from(fit(left, width));
    }
    let mut spans = fit(left, width - right_width - 2);
    let used: usize = spans.iter().map(|span| self::width(&span.content)).sum();
    spans.push(Span::raw(" ".repeat(width - right_width - used)));
    spans.extend(right);
    Line::from(spans)
}

/// A short, steady form of a duration: `0.4s`, `12s`, `2m05s`, `1h02m`.
#[must_use]
pub fn duration(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 10_000 {
        let tenths = (millis + 50) / 100;
        return format!("{}.{}s", tenths / 10, tenths % 10);
    }
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{:02}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// The first twelve characters of a hash or ID.
#[must_use]
pub fn short(id: &str) -> &str {
    id.get(..12).unwrap_or(id)
}

/// A key fingerprint shortened to `SHA256:` and twelve characters.
#[must_use]
pub fn short_fingerprint(fingerprint: &str) -> String {
    match fingerprint.split_once(':') {
        Some((kind, digest)) if digest.len() > 12 => {
            format!("{kind}:{}{ELLIPSIS}", digest.get(..12).unwrap_or(digest))
        }
        _ => fingerprint.to_owned(),
    }
}

/// `1 node`, `3 nodes`.
#[must_use]
pub fn count(amount: usize, one: &str, many: &str) -> String {
    format!("{amount} {}", if amount == 1 { one } else { many })
}
