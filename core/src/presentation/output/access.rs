use super::*;

pub(crate) fn write_gate_plan(
    plan: &crate::domain::gate::GateMutationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Gate {:?} plan {}",
        plan.action,
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(writer, "  policy: {}", terminal_safe(&plan.policy)).map_err(output_error)?;
    writeln!(writer, "  subject: {}", terminal_safe(&plan.subject)).map_err(output_error)?;
    for effect in &plan.effects {
        writeln!(writer, "  effect: {}", terminal_safe(effect)).map_err(output_error)?;
    }
    if plan.requires_hidden_input {
        writeln!(
            writer,
            "  secret input: hidden prompt or --stdin at apply only"
        )
        .map_err(output_error)?;
    }
    writeln!(
        writer,
        "Review complete. Apply with --plan {} --yes.",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

pub(crate) fn write_gate_applied(
    plan: &crate::domain::gate::GateMutationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(
            &serde_json::json!({ "plan_hash": plan.plan_hash, "action": plan.action, "applied": true }),
        );
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Applied gate plan {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)
}

pub(crate) fn write_gate_policies(
    values: &[crate::domain::gate::GatePolicy],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&values);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for value in values {
        writeln!(
            writer,
            "{}  mode={:?} drivers={} session={}d revision={} secrets={}",
            terminal_safe(value.name.as_str()),
            value.mode,
            value
                .drivers
                .iter()
                .map(|driver| driver.as_str())
                .collect::<Vec<_>>()
                .join(","),
            value.session_ttl_seconds / 86_400,
            value.revision,
            terminal_safe(&value.secret_provider)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_gate_users(
    values: &[crate::domain::gate::GateUserMetadata],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&values);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for value in values {
        writeln!(
            writer,
            "{}  enabled={} password-version={}",
            terminal_safe(&value.display_username),
            value.enabled,
            value.password_version
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_gate_sessions(
    values: &[crate::domain::gate::GateSessionMetadata],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&values);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for value in values {
        writeln!(
            writer,
            "{}  user={} expires={} revoked={}",
            value.id,
            terminal_safe(&value.username),
            value.expires_at_unix_ms,
            value.revoked_at_unix_ms.is_some()
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_gate_attachments(
    values: &[crate::domain::gate::GateRouteAttachment],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&values);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    for value in values {
        writeln!(
            writer,
            "{}{}  router={} middleware={}",
            terminal_safe(&value.host),
            terminal_safe(&value.path_prefix),
            terminal_safe(&value.router_name),
            terminal_safe(&value.middleware_name)
        )
        .map_err(output_error)?;
    }
    Ok(())
}
