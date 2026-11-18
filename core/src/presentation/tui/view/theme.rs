use std::ffi::OsString;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

/// How many colors the terminal can show; chosen once per process from the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorDepth {
    None,
    Basic,
    Indexed,
    TrueColor,
}

/// Status meaning shared by badges, glyphs, and accents across every view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Warn,
    Bad,
    Info,
    Idle,
}

impl Tone {
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Ok => "●",
            Self::Warn => "▲",
            Self::Bad => "✗",
            Self::Info => "◆",
            Self::Idle => "○",
        }
    }
}

pub(super) struct Theme {
    pub(super) depth: ColorDepth,
    accent: Color,
    warn: Color,
    bad: Color,
    info: Color,
    ink: Color,
    ink_soft: Color,
    muted: Color,
    border: Color,
    selection: Color,
}

impl Theme {
    pub(super) fn new(depth: ColorDepth) -> Self {
        let pick = |rgb: (u8, u8, u8), indexed: u8, basic: Color| match depth {
            ColorDepth::TrueColor => Color::Rgb(rgb.0, rgb.1, rgb.2),
            ColorDepth::Indexed => Color::Indexed(indexed),
            ColorDepth::Basic => basic,
            ColorDepth::None => Color::Reset,
        };
        Self {
            depth,
            accent: pick((198, 242, 78), 191, Color::LightGreen),
            warn: pick((255, 181, 71), 215, Color::Yellow),
            bad: pick((255, 100, 100), 203, Color::LightRed),
            info: pick((143, 211, 255), 117, Color::LightBlue),
            ink: pick((241, 243, 242), 255, Color::White),
            ink_soft: pick((185, 192, 188), 250, Color::Gray),
            muted: pick((127, 136, 131), 244, Color::DarkGray),
            border: pick((58, 63, 68), 238, Color::DarkGray),
            selection: pick((36, 42, 30), 235, Color::Reset),
        }
    }

    const fn colorless(&self) -> bool {
        matches!(self.depth, ColorDepth::None)
    }

    pub(super) fn text(&self) -> Style {
        Style::new().fg(self.ink)
    }

    pub(super) fn strong(&self) -> Style {
        self.text().add_modifier(Modifier::BOLD)
    }

    pub(super) fn soft(&self) -> Style {
        Style::new().fg(self.ink_soft)
    }

    pub(super) fn muted(&self) -> Style {
        let style = Style::new().fg(self.muted);
        if self.colorless() {
            style.add_modifier(Modifier::DIM)
        } else {
            style
        }
    }

    pub(super) fn accent(&self) -> Style {
        Style::new().fg(self.accent)
    }

    pub(super) fn key(&self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    pub(super) fn tone(&self, tone: Tone) -> Style {
        Style::new().fg(match tone {
            Tone::Ok => self.accent,
            Tone::Warn => self.warn,
            Tone::Bad => self.bad,
            Tone::Info => self.info,
            Tone::Idle => self.muted,
        })
    }

    /// The colour of the cluster node at `index` of the target's nodes, so
    /// the lines of each node are told apart. Nodes beyond the palette
    /// share a colour and differ by name.
    pub(super) fn node(&self, index: usize) -> Style {
        let palette = [self.info, self.accent, self.ink_soft, self.warn];
        Style::new().fg(palette[index % palette.len()])
    }

    pub(super) fn border(&self) -> Style {
        Style::new().fg(self.border)
    }

    pub(super) fn border_focus(&self) -> Style {
        self.accent()
    }

    /// Selected rows keep their own colors and gain a tinted background, or reverse video
    /// when the terminal cannot tint.
    pub(super) fn selected(&self) -> Style {
        match self.depth {
            ColorDepth::TrueColor | ColorDepth::Indexed => {
                Style::new().bg(self.selection).add_modifier(Modifier::BOLD)
            }
            ColorDepth::Basic | ColorDepth::None => Style::new().add_modifier(Modifier::REVERSED),
        }
    }

    /// A solid label in a status color, for context that must not be missed.
    pub(super) fn chip(&self, tone: Tone) -> Style {
        if self.colorless() {
            return Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        let Some(background) = self.tone(tone).fg else {
            return self.strong();
        };
        Style::new()
            .fg(Color::Black)
            .bg(background)
            .add_modifier(Modifier::BOLD)
    }

    /// A tinted label for context that is the calm default.
    pub(super) fn chip_quiet(&self) -> Style {
        self.selected().patch(self.accent())
    }

    /// The solid wordmark block in the header.
    pub(super) fn brand(&self) -> Style {
        if self.colorless() {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
                .fg(Color::Black)
                .bg(self.accent)
                .add_modifier(Modifier::BOLD)
        }
    }
}

pub(super) fn theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| Theme::new(detect_depth(|name| std::env::var_os(name))))
}

pub(crate) fn detect_depth(variable: impl Fn(&str) -> Option<OsString>) -> ColorDepth {
    let set = |name: &str| variable(name).is_some_and(|value| !value.is_empty());
    if set("NO_COLOR") {
        return ColorDepth::None;
    }
    let colorterm = variable("COLORTERM").unwrap_or_default();
    if colorterm == "truecolor" || colorterm == "24bit" || set("WT_SESSION") {
        return ColorDepth::TrueColor;
    }
    let term = variable("TERM").unwrap_or_default();
    if term == "dumb" {
        ColorDepth::None
    } else if term.to_string_lossy().contains("256color") {
        ColorDepth::Indexed
    } else {
        ColorDepth::Basic
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{ColorDepth, detect_depth};

    fn depth(pairs: &[(&str, &str)]) -> ColorDepth {
        detect_depth(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        })
    }

    #[test]
    fn color_depth_follows_terminal_capabilities_and_no_color() {
        assert_eq!(depth(&[("COLORTERM", "truecolor")]), ColorDepth::TrueColor);
        assert_eq!(depth(&[("WT_SESSION", "1")]), ColorDepth::TrueColor);
        assert_eq!(depth(&[("TERM", "xterm-256color")]), ColorDepth::Indexed);
        assert_eq!(depth(&[("TERM", "xterm")]), ColorDepth::Basic);
        assert_eq!(depth(&[("TERM", "dumb")]), ColorDepth::None);
        assert_eq!(
            depth(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")]),
            ColorDepth::None
        );
        assert_eq!(
            depth(&[("NO_COLOR", ""), ("TERM", "xterm")]),
            ColorDepth::Basic
        );
    }
}
