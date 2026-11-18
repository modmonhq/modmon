//! Turning an optional unit on or off from the Services view, through the
//! service `mm dev enable` and `mm dev disable` use.

use super::TuiUseCases;
use super::model::{Model, Notice};

pub(super) fn toggle(model: &mut Model, unit: &str, enable: bool, use_cases: &TuiUseCases<'_>) {
    let names = [unit.to_owned()];
    model.notice = Some(
        match use_cases
            .development
            .set(&use_cases.selection.start, &names, enable)
        {
            Ok(change) => {
                use_cases.refresh_now.notify_one();
                Notice::info(notice_text(unit, &change))
            }
            Err(error) => Notice::error(crate::presentation::output::error_line(&error)),
        },
    );
}

fn notice_text(unit: &str, change: &crate::domain::development_units::DevelopmentChange) -> String {
    let mut text = match (
        change.enable,
        change.started.is_empty() && change.stopped.is_empty(),
    ) {
        (true, false) => format!("{unit} is on for this machine. Press u to start it."),
        (false, false) => {
            format!("{unit} is off for this machine. Press u to stop it and remove its routes.")
        }
        (_, true) => format!("{unit} runs as before."),
    };
    if let Some(note) = change.notes.first() {
        text.push(' ');
        text.push_str(note);
        text.push('.');
    }
    text
}
