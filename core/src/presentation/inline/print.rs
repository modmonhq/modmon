//! Writing lines out of the viewport.

use std::io::{self, Write};

use crossterm::style::{Attribute, PrintStyledContent, SetAttribute, StyledContent};
use ratatui::backend::IntoCrossterm;
use ratatui::text::Line;

/// Writes `lines` without trailing spaces, with escape codes when `styled`.
pub fn write_lines(
    writer: &mut impl Write,
    lines: &[Line<'static>],
    styled: bool,
) -> io::Result<()> {
    for line in lines {
        if styled {
            let spans = trimmed(line);
            for span in spans {
                let style: crossterm::style::ContentStyle =
                    span.style.patch(line.style).into_crossterm();
                crossterm::queue!(
                    writer,
                    PrintStyledContent(StyledContent::new(style, span.content.as_ref()))
                )?;
            }
            crossterm::queue!(writer, SetAttribute(Attribute::Reset))?;
            writeln!(writer)?;
        } else {
            writeln!(writer, "{}", plain(line))?;
        }
    }
    writer.flush()
}

/// The text of a line without styles or trailing spaces.
#[must_use]
pub fn plain(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// The spans of `line` up to its last visible character.
fn trimmed<'a>(line: &'a Line<'a>) -> Vec<ratatui::text::Span<'a>> {
    let mut spans = line.spans.clone();
    while let Some(last) = spans.last_mut() {
        let kept = last.content.trim_end().to_owned();
        if kept.is_empty() {
            spans.pop();
        } else {
            last.content = kept.into();
            break;
        }
    }
    spans
}

/// Writes `lines` to standard output.
pub fn stdout(lines: &[Line<'static>], styled: bool) -> Result<(), crate::domain::error::MmError> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    write_lines(&mut writer, lines, styled).map_err(|source| crate::domain::error::MmError::Io {
        context: "writing command output".to_owned(),
        source,
    })
}
