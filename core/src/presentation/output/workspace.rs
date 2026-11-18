use super::*;

pub(crate) fn write_validation(
    snapshot: &ValidationSnapshot,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(snapshot);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{} [{}]",
        terminal_safe(&snapshot.project_name),
        terminal_safe(&snapshot.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  config: {}",
        terminal_safe(&snapshot.config_path.display().to_string())
    )
    .map_err(output_error)?;
    write_validation_report(&mut writer, &snapshot.validation)
}

pub(crate) fn write_doctor(
    report: &DoctorReport,
    format: OutputFormat,
    context: &RecoveryContext,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        let mut value = serde_json::to_value(report).map_err(|error| MmError::State {
            message: format!("could not serialize doctor report: {error}"),
        })?;
        value["recovery_commands"] = serde_json::json!(
            report
                .readiness
                .iter()
                .flat_map(|report| &report.checks)
                .filter_map(|check| check.issue.as_ref())
                .map(|issue| context.command(issue))
                .collect::<Vec<_>>()
        );
        return write_json(&value);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "mm doctor").map_err(output_error)?;
    if let Some(project_name) = &report.project_name {
        writeln!(writer, "  project:     {}", terminal_safe(project_name)).map_err(output_error)?;
    }
    if let Some(root) = &report.root {
        writeln!(
            writer,
            "  root:        {}",
            terminal_safe(&root.display().to_string())
        )
        .map_err(output_error)?;
    }
    if let Some(target) = &report.target {
        writeln!(
            writer,
            "  target:      {}{}",
            terminal_safe(target),
            report
                .target_source
                .as_deref()
                .map_or_else(String::new, |source| { format!(" (from {source})") })
        )
        .map_err(output_error)?;
    }
    if let Some(environment) = &report.environment {
        writeln!(writer, "  environment: {}", terminal_safe(environment)).map_err(output_error)?;
    }
    if let Some(delivery) = &report.environment_delivery {
        writeln!(
            writer,
            "  delivery:    {} (from {})",
            delivery.mode,
            delivery.provenance.as_str()
        )
        .map_err(output_error)?;
    }
    if !report.project_selected {
        writeln!(
            writer,
            "  selection:   host-only; no project runtime was selected"
        )
        .map_err(output_error)?;
    } else if let (Some(kind), Some(source)) =
        (report.runtime_kind, report.runtime_source.as_deref())
    {
        writeln!(writer, "  selected:    {kind:?} (from {source})").map_err(output_error)?;
    }
    if let Some(readiness) = &report.readiness {
        write_readiness(&mut writer, readiness)?;
    }
    if report.runtime_kind.is_some() || report.readiness.is_none() {
        write_runtime(&mut writer, &report.runtime)?;
    }
    write_ingress_statuses_to(&mut writer, &report.ingress)?;
    if let Some(update) = &report.update {
        writeln!(
            writer,
            "  update:      {}; {}",
            match update.status {
                crate::domain::release::UpdateStatus::UpToDate => "up to date",
                crate::domain::release::UpdateStatus::UpdateAvailable => "update available",
                crate::domain::release::UpdateStatus::InstalledVersionNewer =>
                    "installed version newer",
                crate::domain::release::UpdateStatus::UnsupportedPlatform => "unsupported platform",
                crate::domain::release::UpdateStatus::Unavailable => "unavailable",
            },
            terminal_safe(&update.message)
        )
        .map_err(output_error)?;
    }
    write_findings_and_steps(&mut writer, report, context)
}

/// The findings, then what to do about setup. A step says the same as its
/// diagnostic in plainer words, so that diagnostic is left out.
fn write_findings_and_steps(
    writer: &mut impl Write,
    report: &DoctorReport,
    context: &RecoveryContext,
) -> Result<(), MmError> {
    let steps = report.setup_steps();
    let mut validation = report.validation.clone();
    validation.diagnostics.retain(|diagnostic| {
        !steps.iter().any(|step| {
            step.issue
                .as_ref()
                .is_some_and(|issue| issue.diagnostic_code() == diagnostic.code)
        })
    });
    write_validation_report(writer, &validation)?;
    write_diagnostics(writer, &report.runtime.diagnostics)?;
    write_setup_steps(writer, report, &steps, context)
}

/// Ends the report with what to do: each step once, in the order to do it.
fn write_setup_steps(
    writer: &mut impl Write,
    report: &DoctorReport,
    steps: &[crate::domain::readiness::SetupStep],
    context: &RecoveryContext,
) -> Result<(), MmError> {
    let subject = report
        .environment
        .as_deref()
        .map_or_else(|| "This machine".to_owned(), terminal_safe);
    if steps.is_empty() {
        if report
            .readiness
            .as_ref()
            .is_some_and(|readiness| readiness.is_ready())
        {
            writeln!(writer, "\n{subject} is ready.").map_err(output_error)?;
        }
        return Ok(());
    }
    writeln!(
        writer,
        "\n{subject} needs {} before it can start:",
        if steps.len() == 1 {
            "1 step".to_owned()
        } else {
            format!("{} steps", steps.len())
        }
    )
    .map_err(output_error)?;
    for (index, step) in steps.iter().enumerate() {
        writeln!(
            writer,
            "\n  {}. {}\n     {}",
            index + 1,
            terminal_safe(&step.title),
            terminal_safe(&step.detail)
        )
        .map_err(output_error)?;
        if let Some(issue) = &step.issue {
            writeln!(
                writer,
                "     run: {}",
                terminal_safe(&context.display_command(issue))
            )
            .map_err(output_error)?;
        }
    }
    if steps
        .iter()
        .any(|step| step.issue.as_ref().is_some_and(|issue| issue.is_guided()))
    {
        writeln!(
            writer,
            "\nRun `mm` in a terminal to do these one at a time."
        )
        .map_err(output_error)?;
    }
    Ok(())
}

fn write_readiness(
    writer: &mut impl Write,
    readiness: &crate::domain::readiness::ReadinessReport,
) -> Result<(), MmError> {
    // Passed checks are a count; failed ones come back as the steps that
    // end the report, and waiting ones clear with what they wait for.
    use crate::domain::readiness::CheckStatus;
    let count = |status: CheckStatus| {
        readiness
            .checks
            .iter()
            .filter(|check| check.status == status)
            .count()
    };
    let mut summary = vec![format!("{} passed", count(CheckStatus::Passed))];
    if count(CheckStatus::Failed) > 0 {
        summary.push(format!("{} failed", count(CheckStatus::Failed)));
    }
    let waiting = readiness
        .checks
        .iter()
        .filter(|check| check.status == CheckStatus::Blocked)
        .map(|check| terminal_safe(&check.name))
        .collect::<Vec<_>>();
    if !waiting.is_empty() {
        summary.push(format!(
            "{} waiting ({})",
            waiting.len(),
            waiting.join(", ")
        ));
    }
    writeln!(writer, "  checks:      {}", summary.join(", ")).map_err(output_error)?;
    for candidate in &readiness.candidates {
        writeln!(
            writer,
            "  candidate:   {:?} ({})",
            candidate.runtime,
            if candidate.probe.available {
                "available"
            } else {
                "unavailable"
            }
        )
        .map_err(output_error)?;
        if !candidate.probe.available {
            write_diagnostics(writer, &candidate.probe.diagnostics)?;
        }
    }
    Ok(())
}
