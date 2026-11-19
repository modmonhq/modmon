use super::*;

pub(crate) fn write_secret_providers(
    snapshot: &SecretCapabilitySnapshot,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(snapshot);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Storage backends").map_err(output_error)?;
    writeln!(
        writer,
        "  {:<16} {:<14} {:<12} ROTATION",
        "BACKEND", "REVISION", "SCOPING"
    )
    .map_err(output_error)?;
    for backend in &snapshot.storage {
        writeln!(
            writer,
            "  {:<16} {:<14} {:<12} {:?}",
            terminal_safe(backend.id.as_str()),
            format!("{:?}", backend.revision_semantics).to_ascii_lowercase(),
            format!("{:?}", backend.environment_scoping).to_ascii_lowercase(),
            backend.rotation
        )
        .map_err(output_error)?;
        if !backend.supports(StorageOperation::Create) {
            writeln!(
                writer,
                "    read-only; operations: {:?}",
                backend.operations
            )
            .map_err(output_error)?;
        }
    }
    writeln!(writer, "Delivery drivers").map_err(output_error)?;
    writeln!(writer, "  {:<24} {:<34} PLAINTEXT", "DRIVER", "OPERATION").map_err(output_error)?;
    for delivery in &snapshot.delivery {
        writeln!(
            writer,
            "  {:<24} {:<34} {:?}",
            terminal_safe(delivery.id.as_str()),
            format!("{:?}", delivery.operation).to_ascii_lowercase(),
            delivery.plaintext_persistence
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_direct_secret_rotation_plan(
    plan: &crate::domain::secret::DirectSecretRotationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Secret '{}' rotation plan {}",
        terminal_safe(&plan.name),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(writer, "  provider:    {}", terminal_safe(&plan.provider)).map_err(output_error)?;
    writeln!(
        writer,
        "  environment: {}",
        terminal_safe(&plan.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  hidden input required: {}",
        plan.requires_hidden_input
    )
    .map_err(output_error)
}

pub(crate) fn write_secret_status(
    snapshot: &SecretStatusSnapshot,
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
        "  {:<28} {:<12} {:<12} LOCATION",
        "KEY", "PROVIDER", "STATUS"
    )
    .map_err(output_error)?;
    for secret in &snapshot.secrets {
        writeln!(
            writer,
            "  {:<28} {:<12} {:<12} {}",
            terminal_safe(&secret.key),
            terminal_safe(&secret.provider),
            secret_status(secret.status),
            terminal_safe(secret.location.as_deref().unwrap_or("-"))
        )
        .map_err(output_error)?;
    }
    if !snapshot.runtime_delivery.is_empty() {
        writeln!(writer, "  Runtime delivery").map_err(output_error)?;
        writeln!(
            writer,
            "  {:<28} {:<22} {:<22} {:<14} {:<10} CONSUMERS",
            "KEY", "BACKEND", "DELIVERY", "REVISION", "AGE"
        )
        .map_err(output_error)?;
        for secret in &snapshot.runtime_delivery {
            writeln!(
                writer,
                "  {:<28} {:<22} {:<22} {:<14} {:<10} {}",
                terminal_safe(&secret.logical_name),
                terminal_safe(&secret.backend),
                terminal_safe(&secret.delivery),
                terminal_safe(secret.revision.as_deref().unwrap_or("unknown")),
                secret
                    .age_seconds
                    .map_or_else(|| "unknown".to_owned(), |age| format!("{age}s")),
                terminal_safe(&secret.consumers.join(", "))
            )
            .map_err(output_error)?;
        }
    }
    write_diagnostics(&mut writer, &snapshot.diagnostics)
}

pub(crate) fn write_secret_mutation(
    key: &str,
    provider: &str,
    revision: Option<&str>,
    removed: Option<bool>,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({
            "key": key,
            "provider": provider,
            "revision": revision,
            "removed": removed,
            "set": revision.is_some(),
        }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if let Some(removed) = removed {
        writeln!(
            writer,
            "{} secret {} ({})",
            if removed { "Removed" } else { "No stored" },
            terminal_safe(key),
            terminal_safe(provider)
        )
        .map_err(output_error)
    } else {
        writeln!(
            writer,
            "Set secret {} ({})",
            terminal_safe(key),
            terminal_safe(provider)
        )
        .map_err(output_error)
    }
}

pub(crate) fn write_secret_generation(
    report: &SecretGenerationReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Generated secrets for {} in {}",
        terminal_safe(&report.project_name),
        terminal_safe(&report.environment)
    )
    .map_err(output_error)?;
    if report.secrets.is_empty() {
        return writeln!(writer, "  No secret to generate").map_err(output_error);
    }
    writeln!(writer, "  {:<28} {:<16} STATUS", "NAME", "PROVIDER").map_err(output_error)?;
    for secret in &report.secrets {
        writeln!(
            writer,
            "  {:<28} {:<16} {}",
            terminal_safe(&secret.name),
            terminal_safe(&secret.provider),
            match secret.status {
                SecretGenerationStatus::Generated => "generated",
                SecretGenerationStatus::AlreadySet => "already set",
                SecretGenerationStatus::Skipped => "skipped",
            }
        )
        .map_err(output_error)?;
    }
    for secret in &report.secrets {
        if let Some(reason) = &secret.reason {
            writeln!(
                writer,
                "Skipped {}: {}",
                terminal_safe(&secret.name),
                terminal_safe(reason)
            )
            .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn write_secret_rotated(
    result: &SecretRotationResultItem,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Rotated secret {} ({})",
        terminal_safe(&result.name),
        terminal_safe(&result.provider)
    )
    .map_err(output_error)
}

pub(crate) fn write_secret_rotation_plan(
    plan: &SecretRotationPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Rotation plan {} for {}",
        terminal_safe(&plan.plan_hash),
        terminal_safe(&plan.project_name)
    )
    .map_err(output_error)?;
    writeln!(writer, "  {:<28} {:<22} STATUS", "NAME", "STRATEGY").map_err(output_error)?;
    for secret in &plan.secrets {
        writeln!(
            writer,
            "  {:<28} {:<22} {}",
            terminal_safe(&secret.name),
            terminal_safe(&secret.strategy),
            rotation_plan_status(secret.status)
        )
        .map_err(output_error)?;
    }
    if plan
        .secrets
        .iter()
        .any(|secret| secret.status == SecretRotationPlanStatus::Ready)
        && !plan.secrets.iter().any(|secret| {
            matches!(
                secret.status,
                SecretRotationPlanStatus::InputRequired
                    | SecretRotationPlanStatus::UnsupportedStrategy
            )
        })
    {
        writeln!(
            writer,
            "Apply with: mm secret rotation apply --plan {}",
            terminal_safe(&plan.plan_hash)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_secret_rotation_apply(
    report: &SecretRotationApplyReport,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(report);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Applied rotation plan {} for {}",
        terminal_safe(&report.plan_hash),
        terminal_safe(&report.project_name)
    )
    .map_err(output_error)?;
    for secret in &report.rotated {
        writeln!(
            writer,
            "  Rotated {} ({})",
            terminal_safe(&secret.name),
            terminal_safe(&secret.provider)
        )
        .map_err(output_error)?;
    }
    Ok(())
}
