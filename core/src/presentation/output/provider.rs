use super::*;

pub(crate) fn write_age_initialized(
    initialized: &InitializedAgeStore,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(initialized);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Initialized Age secret storage").map_err(output_error)?;
    writeln!(
        writer,
        "  identity:  {}",
        terminal_safe(&initialized.identity_path.display().to_string())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  store:     {}",
        terminal_safe(&initialized.store_path.display().to_string())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  recipient: {}",
        terminal_safe(&initialized.recipient)
    )
    .map_err(output_error)
}

pub(crate) fn write_provider_status(
    status: &crate::domain::secret_backend::RemoteSecretBackendStatus,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(status);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{} provider instance",
        terminal_safe(&status.driver_display_name)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  name:          {}",
        terminal_safe(status.instance.as_str())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  environment:   {}",
        terminal_safe(&status.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  server:        {}",
        terminal_safe(&status.health.version)
    )
    .map_err(output_error)?;
    writeln!(writer, "  healthy:       {}", status.health.healthy).map_err(output_error)?;
    writeln!(writer, "  sealed:        {}", status.health.sealed).map_err(output_error)?;
    writeln!(writer, "  authenticated: {}", status.authenticated).map_err(output_error)?;
    writeln!(writer, "  mode:          {}", status.mode.as_str()).map_err(output_error)?;
    writeln!(writer, "  scope:         {:?}", status.scope).map_err(output_error)?;
    writeln!(writer, "  capabilities:  {:?}", status.capabilities).map_err(output_error)?;
    writeln!(
        writer,
        "  owned path:    {}",
        terminal_safe(&status.isolated_scope)
    )
    .map_err(output_error)
}

pub(crate) fn write_remote_secret_backend_setup_plan(
    plan: &crate::domain::secret_backend::RemoteSecretBackendSetupPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "External provider '{}' {} plan {}",
        plan.request.instance,
        plan.request.operation.as_str(),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  instance:    {}",
        terminal_safe(plan.request.instance.as_str())
    )
    .map_err(output_error)?;
    writeln!(writer, "  operation:   {}", plan.request.operation.as_str()).map_err(output_error)?;
    writeln!(
        writer,
        "  address:     {}",
        terminal_safe(&plan.request.address)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  environment: {}",
        terminal_safe(&plan.request.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  locations:   {} exact declaration(s)",
        plan.request.locations.len()
    )
    .map_err(output_error)?;
    for consequence in &plan.consequences {
        writeln!(writer, "  review:      {}", terminal_safe(consequence)).map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_managed_provider_plan(
    plan: &crate::domain::secret_backend::ManagedSecretBackendPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Managed provider '{}' {} plan {}",
        plan.instance,
        plan.action.as_str(),
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  image:       {}",
        terminal_safe(&plan.immutable_artifact)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  ingress:     {}",
        terminal_safe(&plan.ingress_url)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  management:  {}",
        terminal_safe(&plan.management_access)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  data volume: {}",
        terminal_safe(&plan.data_resource)
    )
    .map_err(output_error)?;
    writeln!(writer, "  purge data:  {}", plan.purge_data).map_err(output_error)
}

pub(crate) fn write_managed_provider_status(
    status: &crate::domain::secret_backend::ManagedSecretBackendStatus,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(status);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{} provider '{}' installed: {}",
        terminal_safe(&status.driver_display_name),
        status.instance,
        status.installed
    )
    .map_err(output_error)?;
    writeln!(writer, "  mode:       {}", status.mode.as_str()).map_err(output_error)?;
    writeln!(writer, "  scope:      {:?}", status.scope).map_err(output_error)?;
    writeln!(writer, "  capabilities: {:?}", status.capabilities).map_err(output_error)?;
    writeln!(
        writer,
        "  ingress:    {}",
        terminal_safe(&status.ingress_url)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  access:     {}",
        terminal_safe(&status.management_access)
    )
    .map_err(output_error)?;
    if let Some(health) = &status.health {
        writeln!(writer, "  version:    {}", terminal_safe(&health.version))
            .map_err(output_error)?;
        writeln!(writer, "  healthy:    {}", health.healthy).map_err(output_error)?;
        writeln!(writer, "  sealed:     {}", health.sealed).map_err(output_error)?;
    }
    writeln!(
        writer,
        "  recovery:  {}",
        if status.recovery_available_off_instance {
            "encrypted off-instance material available"
        } else {
            "missing"
        }
    )
    .map_err(output_error)?;
    for diagnostic in &status.diagnostics {
        writeln!(writer, "  diagnostic: {}", terminal_safe(diagnostic)).map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_managed_provider_result(
    result: &crate::domain::secret_backend::ManagedSecretBackendResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Managed provider '{}' {} completed for plan {}",
        result.instance,
        result.action.as_str(),
        terminal_safe(&result.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(writer, "  rolled back: {}", result.rolled_back).map_err(output_error)?;
    writeln!(writer, "  data purged: {}", result.data_purged).map_err(output_error)
}

pub(crate) fn write_managed_secret_data_result(
    result: &crate::domain::secret_backend::ManagedSecretDataResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Provider '{}' {} completed",
        result.instance,
        result.action.as_str()
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  snapshot digest: {}",
        terminal_safe(&result.snapshot_digest)
    )
    .map_err(output_error)?;
    writeln!(writer, "  encrypted:       {}", result.encrypted).map_err(output_error)?;
    writeln!(writer, "  integrity:       {}", result.integrity_checked).map_err(output_error)
}

pub(crate) fn write_provider_catalog(
    drivers: &[crate::domain::secret_backend::ProviderDescriptor],
    instances: &[crate::domain::secret_backend::ProviderInstanceDescriptor],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({ "drivers": drivers, "instances": instances }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Registered provider drivers:").map_err(output_error)?;
    for driver in drivers {
        writeln!(
            writer,
            "  {} ({})",
            terminal_safe(&driver.display_name),
            driver.driver
        )
        .map_err(output_error)?;
        for mode in &driver.modes {
            writeln!(writer, "    {}: {:?}", mode.mode.as_str(), mode.operations)
                .map_err(output_error)?;
        }
    }
    writeln!(writer, "Configured provider instances:").map_err(output_error)?;
    for instance in instances {
        writeln!(
            writer,
            "  {}: {} / {} / {:?}",
            instance.name,
            terminal_safe(&instance.driver_display_name),
            instance.mode.as_str(),
            instance.scope
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_provider_instance(
    instance: &crate::domain::secret_backend::ProviderInstanceDescriptor,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(instance);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Provider instance: {}", instance.name).map_err(output_error)?;
    writeln!(
        writer,
        "  driver:       {} ({})",
        terminal_safe(&instance.driver_display_name),
        instance.driver
    )
    .map_err(output_error)?;
    writeln!(writer, "  mode:         {}", instance.mode.as_str()).map_err(output_error)?;
    writeln!(writer, "  scope:        {:?}", instance.scope).map_err(output_error)?;
    writeln!(writer, "  capabilities: {:?}", instance.operations).map_err(output_error)?;
    writeln!(
        writer,
        "  configuration: {}",
        terminal_safe(&instance.configuration.to_string())
    )
    .map_err(output_error)
}
