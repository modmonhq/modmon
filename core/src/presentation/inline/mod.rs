//! The inline UI of the command line.

mod card;
mod live;
mod print;
mod style;
mod surface;
mod tasks;
mod text;

pub use card::{Card, Mark, Value};
pub use live::{Progress, ROLLOUT, painting, step};
pub use print::{plain, write_lines};
pub use style::Palette;
pub use surface::Surface;
pub use tasks::{StepState, Tasks};
pub use text::{clip, count, duration, short, short_fingerprint};

/// The widest a line is drawn, however wide the terminal.
pub const MAX_WIDTH: usize = 100;

/// Prints `card` to standard output.
pub fn print_card(
    surface: Surface,
    card: impl FnOnce() -> Card,
    plain: impl FnOnce() -> Result<(), crate::domain::error::MmError>,
) -> Result<(), crate::domain::error::MmError> {
    if !surface.is_live() {
        return plain();
    }
    let width = crossterm::terminal::size().map_or(80, |(columns, _)| usize::from(columns));
    print::stdout(
        &card().lines(Palette::color(), width.clamp(40, MAX_WIDTH)),
        true,
    )?;
    crate::presentation::record::muted(plain)
}
