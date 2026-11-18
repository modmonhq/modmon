use super::*;

pub(crate) fn write_environment_status(
    snapshot: &EnvironmentStatusSnapshot,
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
        "  delivery: {} (from {})",
        snapshot.delivery.mode,
        snapshot.delivery.provenance.as_str()
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  {:<14} {:<14} {:<16} {:<22} {:<18} {:<12} VALUE",
        "STACK", "MODULE", "SERVICE", "VARIABLE", "SOURCE", "STATUS"
    )
    .map_err(output_error)?;
    for module in &snapshot.modules {
        for service in &module.services {
            for variable in &service.variables {
                let value = if variable.sensitive {
                    "[secret]"
                } else {
                    variable.value.as_deref().unwrap_or("-")
                };
                writeln!(
                    writer,
                    "  {:<14} {:<14} {:<16} {:<22} {:<18} {:<12} {}",
                    terminal_safe(&service.stack),
                    terminal_safe(&module.module),
                    terminal_safe(&service.service),
                    terminal_safe(&variable.name),
                    value_source(variable.source),
                    resolution_status(variable.status),
                    terminal_safe(value)
                )
                .map_err(output_error)?;
            }
        }
    }
    write_diagnostics(&mut writer, &snapshot.diagnostics)
}

pub(crate) fn write_environment_override(
    value: &EnvironmentValueOverride,
    removed: Option<bool>,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({
            "key": value.key,
            "value": value.value,
            "revision": value.revision,
            "updated_at_unix_ms": value.updated_at_unix_ms,
            "removed": removed,
        }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if removed == Some(true) {
        writeln!(writer, "Removed override for {}", terminal_safe(&value.key)).map_err(output_error)
    } else {
        writeln!(
            writer,
            "Set {}={} (revision {})",
            terminal_safe(&value.key),
            terminal_safe(&value.value),
            value.revision
        )
        .map_err(output_error)
    }
}

pub(crate) fn write_removed(key: &str, removed: bool, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({ "key": key, "removed": removed }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{} {}",
        if removed {
            "Removed"
        } else {
            "No override for"
        },
        terminal_safe(key)
    )
    .map_err(output_error)
}

pub(crate) fn write_environment_delivery(
    environment: &str,
    delivery: &ResolvedEnvironmentDelivery,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({
            "environment": environment,
            "mode": delivery.mode,
            "provenance": delivery.provenance,
        }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{}: {} (from {})",
        terminal_safe(environment),
        delivery.mode,
        delivery.provenance.as_str()
    )
    .map_err(output_error)
}

pub(crate) fn write_environment_review(
    review: &EnvironmentReview,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(review);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "delivery: {} (from {})",
        review.delivery.mode,
        review.delivery.provenance.as_str()
    )
    .map_err(output_error)?;
    writeln!(writer, "input: {}", terminal_safe(&review.input_hash)).map_err(output_error)?;
    for service in &review.services {
        writeln!(
            writer,
            "  {}/{}/{}: {} variables, {} secrets",
            terminal_safe(&service.stack),
            terminal_safe(&service.module),
            terminal_safe(&service.service),
            service.variable_count,
            service.secret_count
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_default_environment(
    environment: Option<&str>,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({ "default_environment": environment }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Default environment: {}",
        terminal_safe(environment.unwrap_or("not set"))
    )
    .map_err(output_error)
}

pub(crate) fn write_compiled_manifest(
    manifest: &CompiledEnvironmentManifest,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(manifest);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Environment check passed for {} [{}]",
        terminal_safe(&manifest.project_name),
        terminal_safe(&manifest.environment)
    )
    .map_err(output_error)?;
    for service in &manifest.services {
        writeln!(
            writer,
            "  {}/{}/{}: {} variables, {} secrets",
            terminal_safe(&service.stack),
            terminal_safe(&service.module),
            terminal_safe(&service.service),
            service.variable_count,
            service.secret_count
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_stack_compilation(
    report: &StackCompilationReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Compiled {} [{}] for target {}",
        terminal_safe(&report.project_name),
        terminal_safe(&report.environment),
        terminal_safe(&report.target)
    )
    .map_err(output_error)?;
    for stack in &report.stacks {
        writeln!(
            writer,
            "  {} -> {} ({})",
            terminal_safe(&stack.manifest.stack),
            terminal_safe(&stack.manifest.compose_project_name),
            terminal_safe(&stack.manifest.artifact_hash)
        )
        .map_err(output_error)?;
        writeln!(
            writer,
            "    overlay:  {}",
            terminal_safe(&stack.generated_overlay_path.display().to_string())
        )
        .map_err(output_error)?;
        writeln!(
            writer,
            "    manifest: {}",
            terminal_safe(&stack.manifest_path.display().to_string())
        )
        .map_err(output_error)?;
    }
    write_excluded_services_to(&mut writer, &report.excluded_services, &report.environment)?;
    write_ingress_statuses_to(&mut writer, &report.ingress)?;
    Ok(())
}

pub(super) fn write_excluded_services_to(
    writer: &mut impl Write,
    services: &[crate::domain::stack::ExcludedService],
    environment: &str,
) -> Result<(), MmError> {
    for service in services {
        let name = format!("{}/{}", service.module, service.service);
        let line = match &service.off_locally {
            Some(off) => match off.enable_command() {
                Some(command) => format!("  off locally (optional): {name}; turn it on: {command}"),
                None => format!(
                    "  off locally: {name} (development: off through {})",
                    off.unit
                ),
            },
            None => format!(
                "  excluded: {name} (stack {}, environment {environment})",
                service.stack
            ),
        };
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    Ok(())
}

/// One line per optional unit the plan leaves off, with what turns it on.
#[must_use]
pub fn off_locally_notes(services: &[crate::domain::stack::ExcludedService]) -> Vec<String> {
    let mut units = std::collections::BTreeMap::<&str, Vec<String>>::new();
    for service in services {
        if let Some(off) = service.off_locally.as_ref().filter(|off| off.is_optional()) {
            units
                .entry(off.unit.as_str())
                .or_default()
                .push(format!("{}/{}", service.module, service.service));
        }
    }
    units
        .into_iter()
        .map(|(unit, services)| {
            format!(
                "{unit} is off locally (optional): {}. Turn it on with `mm dev enable {unit}`",
                services.join(", ")
            )
        })
        .collect()
}

pub(crate) fn write_materialized(
    materialized: &MaterializedEnvironment,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(materialized);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Materialized {}/{}/{} to {} ({} variables, {} secrets)",
        terminal_safe(&materialized.stack),
        terminal_safe(&materialized.module),
        terminal_safe(&materialized.service),
        terminal_safe(&materialized.path.display().to_string()),
        materialized.variable_count,
        materialized.secret_count
    )
    .map_err(output_error)
}

pub(crate) fn write_environment_diff(
    difference: &EnvironmentDiff,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(difference);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{}/{}/{}: {} ({})",
        terminal_safe(&difference.stack),
        terminal_safe(&difference.module),
        terminal_safe(&difference.service),
        if difference.changed {
            "changed"
        } else {
            "unchanged"
        },
        terminal_safe(&difference.path.display().to_string())
    )
    .map_err(output_error)
}
