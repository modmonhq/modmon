use super::*;
use crate::domain::development_units::{OptionalUnit, UnitKind};

/// The optional units that are off on this machine, greyed out below the
/// services they would add.
pub(super) fn optional_lines(model: &Model, stacks: bool) -> Vec<Row<'static>> {
    let theme = theme();
    model
        .optional_rows()
        .into_iter()
        .map(|unit| {
            let mut cells = vec![
                Cell::from(Line::from(badge(Tone::Idle, "off locally"))),
                Cell::from(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(terminal_safe(&unit.name), theme.muted()),
                    Span::styled(format!("  {} · o turns it on", kind(unit)), theme.muted()),
                ])),
                Cell::from(""),
                Cell::from(Span::styled(covers(unit), theme.muted())),
                Cell::from(""),
            ];
            if !stacks {
                cells.remove(2);
            }
            Row::new(cells)
        })
        .collect()
}

fn kind(unit: &OptionalUnit) -> &'static str {
    match unit.kind {
        UnitKind::Module => "optional module",
        UnitKind::Service => "optional service",
        UnitKind::Job => "optional job",
    }
}

fn covers(unit: &OptionalUnit) -> String {
    match (unit.services.len(), unit.jobs.len()) {
        (services, 0) => plural(services, "service"),
        (0, jobs) => plural(jobs, "job"),
        (services, jobs) => format!("{} · {}", plural(services, "service"), plural(jobs, "job")),
    }
}
