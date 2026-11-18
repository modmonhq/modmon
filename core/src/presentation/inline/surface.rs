//! The one place that decides how a command shows itself.

use std::ffi::OsString;
use std::io::IsTerminal;

use crate::presentation::cli::OutputFormat;
use crate::presentation::tui::{ColorDepth, detect_depth};

/// How a command shows its progress and its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// A terminal that takes colour and cursor movement.
    Live,
    /// Anything else a person or a script reads.
    Plain,
    /// `--output json`.
    Quiet,
}

impl Surface {
    /// The surface of this process for `format`.
    #[must_use]
    pub fn of(format: OutputFormat) -> Self {
        Self::decide(
            format,
            std::io::stdout().is_terminal() && std::io::stderr().is_terminal(),
            |name| std::env::var_os(name),
        )
    }

    /// The rule itself.
    pub fn decide(
        format: OutputFormat,
        terminals: bool,
        variable: impl Fn(&str) -> Option<OsString>,
    ) -> Self {
        if format == OutputFormat::Json {
            return Self::Quiet;
        }
        let set = |name: &str| variable(name).is_some_and(|value| !value.is_empty());
        let plain = !terminals
            || set("CI")
            || set("MM_PLAIN")
            || detect_depth(&variable) == ColorDepth::None;
        if plain { Self::Plain } else { Self::Live }
    }

    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Live)
    }
}
