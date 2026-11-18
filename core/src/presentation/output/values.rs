use super::*;
use crate::domain::staged::{HostValue, RemoteValues, StagedAction, ValueKind};
use crate::presentation::inline::{Card, Value};
use crate::presentation::tui::Tone;

fn short(revision: &str) -> String {
    revision.chars().take(8).collect()
}

fn live(value: &HostValue) -> String {
    match (&value.active, value.origin) {
        (_, Some(crate::domain::staged::ValueOrigin::ExternalStore)) => {
            value.provider.as_deref().map_or_else(
                || "external store".to_owned(),
                |provider| format!("in {provider}"),
            )
        }
        (Some(active), origin) => {
            let mut text = short(&active.revision);
            if let Some(by) = &active.changed_by {
                text.push_str(&format!(" by {by}"));
            } else if let Some(origin) = origin {
                text.push_str(&format!(" ({})", origin.label()));
            }
            text
        }
        (None, _) => "unset".to_owned(),
    }
}

fn staged(value: &HostValue) -> Option<String> {
    value.staged.as_ref().map(|staged| {
        if staged.removal {
            format!("- removal by {}", staged.staged_by)
        } else if value.active.is_some() {
            format!("~ {} by {}", short(&staged.revision), staged.staged_by)
        } else {
            format!("+ {} by {}", short(&staged.revision), staged.staged_by)
        }
    })
}

/// Per key and node, the rows of `mm secret status` for a remote environment.
fn rows<'a>(values: &'a RemoteValues, key: Option<&str>) -> Vec<(String, String, &'a HostValue)> {
    let mut rows = Vec::new();
    for (kind, name) in values.keys() {
        if key.is_some_and(|wanted| wanted != name) {
            continue;
        }
        for node in &values.nodes {
            if let Some(value) = node
                .values
                .iter()
                .find(|value| value.kind == kind && value.key == name)
            {
                let node = node.node.clone().unwrap_or_else(|| values.target.clone());
                rows.push((name.clone(), node, value));
            }
        }
    }
    rows
}

fn kind_label(value: &HostValue) -> &'static str {
    match value.kind {
        ValueKind::Secret => "secret",
        ValueKind::Variable => "variable",
    }
}

fn notes(values: &RemoteValues, key: Option<&str>) -> Vec<(Tone, String)> {
    let mut notes = values
        .unreachable
        .iter()
        .map(|node| (Tone::Bad, format!("{}: {}", node.node, node.problem)))
        .collect::<Vec<_>>();
    for (kind, name) in values.keys() {
        if key.is_some_and(|wanted| wanted != name) {
            continue;
        }
        if values.disagreeing(kind, &name) {
            notes.push((
                Tone::Warn,
                format!(
                    "the nodes stage different revisions of {name}; set it again or discard it"
                ),
            ));
        }
    }
    let unset = rows(values, key)
        .into_iter()
        .filter(|(_, _, value)| value.required && value.unset_after_stage())
        .map(|(name, node, _)| format!("{name} on {node}"))
        .collect::<Vec<_>>();
    if !unset.is_empty() {
        notes.push((
            Tone::Bad,
            format!(
                "required but unset: {}; a deployment is refused until it is set",
                unset.join(", ")
            ),
        ));
    }
    notes
}

#[must_use]
pub fn remote_values_card(values: &RemoteValues, key: Option<&str>) -> Card {
    let mut card = Card::new(format!("Secrets and variables of {}", values.environment));
    if let Some(node) = values.nodes.first().filter(|node| !node.commit.is_empty()) {
        card = card.badge(format!("deployed {}", short(&node.commit)));
    }
    let table = rows(values, key)
        .into_iter()
        .map(|(name, node, value)| {
            vec![
                Value::new(name),
                Value::dim(kind_label(value)),
                Value::new(node),
                if value.active.is_some() || value.origin.is_some() {
                    Value::new(live(value))
                } else {
                    Value::tone(
                        "unset",
                        if value.required {
                            Tone::Bad
                        } else {
                            Tone::Idle
                        },
                    )
                },
                staged(value).map_or_else(|| Value::dim("-"), |text| Value::tone(text, Tone::Warn)),
                Value::dim(value.services.join(", ")),
            ]
        })
        .collect();
    card.table(
        &["KEY", "KIND", "NODE", "LIVE", "STAGED", "SERVICES"],
        table,
    );
    card.notes(notes(values, key));
    if values
        .nodes
        .iter()
        .flat_map(|node| &node.values)
        .any(|value| value.staged.is_some())
    {
        card.command(
            "Make the staged changes live",
            format!("mm -e {} deploy redeploy", values.environment),
        );
    }
    card
}

pub(crate) fn write_remote_values(
    values: &RemoteValues,
    key: Option<&str>,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(values);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Secrets and variables of {} on {}",
        terminal_safe(&values.environment),
        terminal_safe(&values.target)
    )
    .map_err(output_error)?;
    for (name, node, value) in rows(values, key) {
        writeln!(
            writer,
            "  {:<28} {:<8} {:<12} live: {:<28} staged: {}",
            terminal_safe(&name),
            kind_label(value),
            terminal_safe(&node),
            terminal_safe(&live(value)),
            terminal_safe(&staged(value).unwrap_or_else(|| "-".to_owned())),
        )
        .map_err(output_error)?;
    }
    for (_, note) in notes(values, key) {
        writeln!(writer, "  {}", terminal_safe(&note)).map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_stage_report(
    report: &crate::app::StageReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for change in &report.changes {
        let line = match &change.action {
            StagedAction::Set { revision } => {
                format!("Staged {} revision {}", change.key, short(revision))
            }
            StagedAction::Unset { .. } => format!("Staged the removal of {}", change.key),
            StagedAction::Discard => format!("Discarded what was staged for {}", change.key),
        };
        writeln!(
            writer,
            "{} on {}",
            terminal_safe(&line),
            terminal_safe(&report.nodes.join(", "))
        )
        .map_err(output_error)?;
    }
    let pending = report
        .changes
        .iter()
        .any(|change| !matches!(change.action, StagedAction::Discard));
    if pending {
        writeln!(
            writer,
            "Running services keep their values until the next deployment: `mm -e {} deploy plan` lists it, `mm -e {} deploy redeploy` makes it live",
            terminal_safe(&report.environment),
            terminal_safe(&report.environment)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

/// One line on what a stage did, for the terminal UI.
#[must_use]
pub fn stage_summary(report: &crate::app::StageReport) -> String {
    let keys = report
        .changes
        .iter()
        .map(|change| match &change.action {
            StagedAction::Set { .. } => format!("set {}", change.key),
            StagedAction::Unset { .. } => format!("removal of {}", change.key),
            StagedAction::Discard => format!("discarded {}", change.key),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Staged {keys} on {}; a deployment makes it live",
        report.nodes.join(", ")
    )
}

/// What `mm secret migrate` moved, and what to change in the repo next.
pub(crate) fn write_store_migrated(
    moved: &crate::domain::secret_store::StoreMigrated,
    environment: &str,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(moved);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for secret in &moved.secrets {
        let line = match (&secret.revision, secret.written) {
            (Some(revision), true) => format!(
                "{}: written to {} at {} as revision {}",
                secret.key,
                moved.provider,
                secret.location,
                short(revision)
            ),
            (Some(revision), false) => format!(
                "{}: {} holds revision {} already",
                secret.key,
                moved.provider,
                short(revision)
            ),
            (None, _) => format!(
                "{}: no value in the age store; set it in the store later",
                secret.key
            ),
        };
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    writeln!(
        writer,
        "Next: in .config/mm.yaml give these secrets `storage: {{provider: {}, location: LOCATION}}` with the location above, then `mm -e {} deploy plan`; the containers that use them keep running",
        terminal_safe(&moved.provider),
        terminal_safe(environment)
    )
    .map_err(output_error)
}
