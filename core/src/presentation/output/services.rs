use super::*;

pub(crate) fn write_service_stack_plan(
    plan: &crate::domain::service_stack::ServiceStackPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Service {} plan {}",
        plan.action.as_str(),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  stack version:  {}",
        terminal_safe(&plan.stack_version)
    )
    .map_err(output_error)?;
    writeln!(writer, "  runtime:        {:?}", plan.runtime).map_err(output_error)?;
    writeln!(writer, "  Mailpit UI:     https://mail.mm.test").map_err(output_error)?;
    writeln!(writer, "  internal SMTP:  mailpit:1025").map_err(output_error)?;
    writeln!(
        writer,
        "  host SMTP:      {}",
        plan.host_smtp_port.map_or_else(
            || "not published".to_owned(),
            |port| format!("127.0.0.1:{port}")
        )
    )
    .map_err(output_error)?;
    writeln!(writer, "  purge data:     {}", plan.purge_data).map_err(output_error)?;
    writeln!(writer, "  components:").map_err(output_error)?;
    for component in &plan.components {
        writeln!(
            writer,
            "    {:?} {} {}",
            component.kind,
            terminal_safe(&component.version),
            component
                .image
                .as_deref()
                .map_or_else(|| "host process".to_owned(), terminal_safe)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_service_stack_status(
    status: &crate::domain::service_stack::ServiceStackStatus,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(status);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Installation services: {}",
        if status.installed {
            "installed"
        } else {
            "not installed"
        }
    )
    .map_err(output_error)?;
    writeln!(writer, "  UI:             {}", terminal_safe(&status.route)).map_err(output_error)?;
    writeln!(
        writer,
        "  internal SMTP:  {}",
        terminal_safe(&status.internal_smtp)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  host SMTP:      {}",
        status.host_smtp_port.map_or_else(
            || "not published".to_owned(),
            |port| format!("127.0.0.1:{port}")
        )
    )
    .map_err(output_error)?;
    for component in &status.components {
        writeln!(
            writer,
            "  {:?}: {} ({})",
            component.kind,
            if component.healthy {
                "healthy"
            } else if component.running {
                "degraded"
            } else {
                "stopped"
            },
            terminal_safe(&component.version)
        )
        .map_err(output_error)?;
    }
    write_diagnostics(&mut writer, &status.diagnostics)
}

pub(crate) fn write_service_stack_result(
    result: &crate::domain::service_stack::ServiceStackResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Service {} completed: {:?}",
        result.action.as_str(),
        result.state
    )
    .map_err(output_error)?;
    if result.rolled_back {
        writeln!(writer, "  interrupted transaction recovered by rollback")
            .map_err(output_error)?;
    }
    if result.data_purged {
        writeln!(writer, "  persistent service data purged").map_err(output_error)?;
    }
    Ok(())
}
