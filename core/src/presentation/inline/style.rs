//! Colour and glyphs of the inline UI.

use ratatui::style::{Color, Modifier, Style};

use crate::presentation::tui::{SPINNER, Tone};

/// The styles every inline component draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    color: bool,
}

impl Palette {
    /// Colour, for a live terminal.
    #[must_use]
    pub const fn color() -> Self {
        Self { color: true }
    }

    /// No styles at all, for plain text.
    #[must_use]
    pub const fn plain() -> Self {
        Self { color: false }
    }

    const fn paint(self, color: Color) -> Style {
        if self.color {
            Style::new().fg(color)
        } else {
            Style::new()
        }
    }

    #[must_use]
    pub const fn text(self) -> Style {
        Style::new()
    }

    #[must_use]
    pub const fn strong(self) -> Style {
        if self.color {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            Style::new()
        }
    }

    /// Secondary text: labels, times, what did not change.
    #[must_use]
    pub const fn dim(self) -> Style {
        if self.color {
            Style::new().add_modifier(Modifier::DIM)
        } else {
            Style::new()
        }
    }

    /// The one accent: what runs now, and commands to copy.
    #[must_use]
    pub const fn accent(self) -> Style {
        self.paint(Color::Cyan)
    }

    #[must_use]
    pub const fn tone(self, tone: Tone) -> Style {
        self.paint(match tone {
            Tone::Ok => Color::Green,
            Tone::Warn => Color::Yellow,
            Tone::Bad => Color::Red,
            Tone::Info => Color::Cyan,
            Tone::Idle => Color::Reset,
        })
    }
}

/// The glyph of a finished step that worked.
pub const DONE: &str = "✓";
/// The glyph of a step or node that failed.
pub const FAILED: &str = "✗";
/// The glyph of a step that had nothing to do.
pub const SKIPPED: &str = "–";
/// The glyph of a step that has not started.
pub const PENDING: &str = "○";
/// The glyph of a node that waits for its turn.
pub const WAITING: &str = "·";

/// The spinner frame for `tick`, the terminal UI's spinner.
#[must_use]
pub fn spinner(tick: u64) -> &'static str {
    let index = usize::try_from(tick % SPINNER.len() as u64).unwrap_or(0);
    SPINNER[index]
}
