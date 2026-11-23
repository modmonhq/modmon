use super::*;

pub(crate) fn write_ingress_statuses(
    statuses: &[crate::domain::ingress::IngressServiceStatus],
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({ "services": statuses }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    write_ingress_statuses_to(&mut writer, statuses)
}

pub(crate) fn write_ingress_statuses_to(
    writer: &mut impl Write,
    statuses: &[crate::domain::ingress::IngressServiceStatus],
) -> Result<(), MmError> {
    if statuses.is_empty() {
        return Ok(());
    }
    writeln!(writer, "  ingress:").map_err(output_error)?;
    for status in statuses {
        if let Some(url) = &status.url {
            writeln!(
                writer,
                "    {}/{}: {} -> port {} ({}){}",
                terminal_safe(&status.stack),
                terminal_safe(&status.service),
                terminal_safe(url),
                status.selected_port.unwrap_or_default(),
                status
                    .provenance
                    .map_or_else(|| "unknown".to_owned(), |value| value.to_string()),
                status
                    .warning
                    .as_deref()
                    .map_or_else(String::new, |warning| format!(
                        " WARNING: {}",
                        terminal_safe(warning)
                    ))
            )
            .map_err(output_error)?;
        } else {
            writeln!(
                writer,
                "    {}/{}: not routed - {}",
                terminal_safe(&status.stack),
                terminal_safe(&status.service),
                terminal_safe(status.reason.as_deref().unwrap_or("no reason recorded"))
            )
            .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn write_target_catalog(
    snapshot: &TargetCatalogSnapshot,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(snapshot);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "  {:<20} {:<38} {:<64} TRANSPORT",
        "TARGET", "ID", "FINGERPRINT"
    )
    .map_err(output_error)?;
    for target in &snapshot.targets {
        writeln!(
            writer,
            "  {:<20} {:<38} {:<64} {}",
            terminal_safe(&target.name),
            target.definition.id.0,
            terminal_safe(&target.fingerprint),
            target_transport(&target.definition.transport)
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_target(target: &NamedTarget, format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(target);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Added target {}", terminal_safe(&target.name)).map_err(output_error)?;
    writeln!(writer, "  id:          {}", target.definition.id.0).map_err(output_error)?;
    writeln!(
        writer,
        "  fingerprint: {}",
        terminal_safe(&target.fingerprint)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  transport:   {}",
        target_transport(&target.definition.transport)
    )
    .map_err(output_error)?;
    if let Some(TargetIngress::Traefik {
        image, acme_email, ..
    }) = &target.definition.ingress
    {
        writeln!(writer, "  ingress:     traefik").map_err(output_error)?;
        writeln!(writer, "  image:       {}", terminal_safe(image)).map_err(output_error)?;
        if let Some(acme_email) = acme_email {
            writeln!(writer, "  ACME email:  {}", terminal_safe(acme_email))
                .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn write_target_details(
    target: &NamedTarget,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(target);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Target {}", terminal_safe(&target.name)).map_err(output_error)?;
    writeln!(writer, "  id:          {}", target.definition.id.0).map_err(output_error)?;
    writeln!(
        writer,
        "  fingerprint: {}",
        terminal_safe(&target.fingerprint)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  transport:   {}",
        target_transport(&target.definition.transport)
    )
    .map_err(output_error)
}

pub(crate) fn write_target_removed(
    target: &NamedTarget,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({ "removed": target }));
    }
    writeln!(
        crate::presentation::record::stdout().lock(),
        "Removed target {}",
        terminal_safe(&target.name)
    )
    .map_err(output_error)
}

pub(crate) fn write_target_updated(
    target: &NamedTarget,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(target);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(writer, "Updated target {}", terminal_safe(&target.name)).map_err(output_error)?;
    writeln!(writer, "  id:          {}", target.definition.id.0).map_err(output_error)?;
    writeln!(
        writer,
        "  fingerprint: {}",
        terminal_safe(&target.fingerprint)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  transport:   {}",
        target_transport(&target.definition.transport)
    )
    .map_err(output_error)?;
    if let Some(TargetIngress::Traefik {
        image, acme_email, ..
    }) = &target.definition.ingress
    {
        writeln!(writer, "  ingress:     traefik").map_err(output_error)?;
        writeln!(writer, "  image:       {}", terminal_safe(image)).map_err(output_error)?;
        if let Some(acme_email) = acme_email {
            writeln!(writer, "  ACME email:  {}", terminal_safe(acme_email))
                .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn write_target_bootstrap_plan(
    plan: &TargetBootstrapPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Bootstrap plan {} for {} [{}]",
        terminal_safe(&plan.plan_hash),
        terminal_safe(&plan.project_name),
        terminal_safe(&plan.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  target: {} ({})",
        terminal_safe(&plan.target.name),
        plan.target.definition.id.0
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  transport: {}",
        target_transport(&plan.target.definition.transport)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  runtime:   {:?} ({})",
        plan.selected_runtime,
        if plan.rootless { "rootless" } else { "rootful" }
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  target fingerprint: {}",
        terminal_safe(&plan.target.fingerprint)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  config hash:        {}",
        terminal_safe(&plan.config_hash)
    )
    .map_err(output_error)?;
    for file in &plan.input_files {
        writeln!(
            writer,
            "  input:  {} ({})",
            terminal_safe(&file.path.display().to_string()),
            terminal_safe(&file.digest)
        )
        .map_err(output_error)?;
    }
    for capability in &plan.required_capabilities {
        writeln!(writer, "  requires: {}", capability.label()).map_err(output_error)?;
    }
    for step in &plan.steps {
        writeln!(writer, "  step:    {}", step.label()).map_err(output_error)?;
    }
    if let Some(access) = &plan.access {
        write_bootstrap_access(&mut writer, access)?;
        if plan.selected_runtime == crate::domain::runtime::RuntimeKind::PodmanCompose
            && !plan.rootless
        {
            writeln!(
                writer,
                "    apparmor:        /etc/apparmor.d/mm-containers for Podman's containers, where the kernel runs AppArmor"
            )
            .map_err(output_error)?;
        }
    }
    writeln!(
        writer,
        "  apply with `{}`",
        terminal_safe(&plan.apply_command())
    )
    .map_err(output_error)
}

fn write_bootstrap_access(
    writer: &mut impl Write,
    access: &crate::domain::target::BootstrapAccess,
) -> Result<(), MmError> {
    writeln!(writer, "  remote access:").map_err(output_error)?;
    // No origin: the host is enrolled to take the files mm sends.
    if access.origin.is_empty() {
        writeln!(
            writer,
            "    delivery:        {}",
            crate::domain::delivery::PUSH_SETUP_LINE
        )
        .map_err(output_error)?;
    } else {
        writeln!(
            writer,
            "    origin:          {}",
            terminal_safe(&access.origin)
        )
        .map_err(output_error)?;
    }
    if let Some(key) = &access.origin_host_key {
        writeln!(writer, "    origin host key: {}", terminal_safe(key)).map_err(output_error)?;
    }
    for entry in &access.break_glass {
        writeln!(
            writer,
            "    break-glass:     {} {}",
            terminal_safe(&entry.name),
            terminal_safe(&entry.key)
        )
        .map_err(output_error)?;
    }
    for set in &access.binaries {
        writeln!(
            writer,
            "    host binaries:   {} from {}",
            terminal_safe(&set.architecture),
            terminal_safe(&set.source)
        )
        .map_err(output_error)?;
        for (name, digest) in [("mm", &set.mm_digest), ("mmd", &set.mmd_digest)] {
            writeln!(
                writer,
                "    install {name:<4}     {} ({})",
                terminal_safe(&set.binary(name).display().to_string()),
                terminal_safe(digest)
            )
            .map_err(output_error)?;
        }
    }
    if access.binaries.len() > 1 {
        writeln!(
            writer,
            "    host cpu:        not read yet; the host takes the pair for its own CPU"
        )
        .map_err(output_error)?;
    }
    for notice in &access.notices {
        writeln!(writer, "    release:         {}", terminal_safe(notice)).map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_target_bootstrap_result(
    result: &TargetBootstrapResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Bootstrapped target {} ({})",
        terminal_safe(&result.target),
        result.target_id.0
    )
    .map_err(output_error)?;
    writeln!(writer, "  plan: {}", terminal_safe(&result.plan_hash)).map_err(output_error)?;
    for capability in &result.verified_capabilities {
        writeln!(writer, "  verified: {}", capability.label()).map_err(output_error)?;
    }
    if let Some(access) = &result.access {
        writeln!(writer, "  mm and mmd: {}", terminal_safe(&access.version))
            .map_err(output_error)?;
        writeln!(writer, "  host key:   {}", terminal_safe(&access.host_key))
            .map_err(output_error)?;
        writeln!(
            writer,
            "  daemon:     {}",
            terminal_safe(&access.daemon_fingerprint)
        )
        .map_err(output_error)?;
        // A host that takes the project's files from mm has no deploy key.
        let pushed = access.deploy_key.is_empty();
        if pushed {
            writeln!(
                writer,
                "The host fetches nothing: mm sends it each commit's files. Commit this target, with the host name developers use, and a users file:"
            )
            .map_err(output_error)?;
        } else {
            writeln!(
                writer,
                "  deploy key: {}",
                terminal_safe(&access.deploy_key)
            )
            .map_err(output_error)?;
            writeln!(
                writer,
                "Add the deploy key to the git host as a read-only key. Commit this target, with the host name developers use, and a users file:"
            )
            .map_err(output_error)?;
        }
        writeln!(
            writer,
            "  targets:\n    {}:\n      host: HOST\n      host-key: {}\n      daemon-fingerprint: {}{}",
            terminal_safe(&result.target),
            terminal_safe(&access.host_key),
            terminal_safe(&access.daemon_fingerprint),
            if pushed { "\n      delivery: push" } else { "" }
        )
        .map_err(output_error)?;
        writeln!(
            writer,
            "Then apply the users file once with a break-glass key (MM_SSH_KEY)."
        )
        .map_err(output_error)?;
    }
    writeln!(writer, "  next: {}", terminal_safe(&result.next_command)).map_err(output_error)
}

pub(crate) fn write_target_ingress_plan(
    plan: &TargetIngressPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Ingress plan {} for target {} ({})",
        terminal_safe(&plan.plan_hash),
        terminal_safe(&plan.target_name),
        plan.target_id.0
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  compose:  {}",
        terminal_safe(&plan.artifact.compose_path.display().to_string())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  manifest: {}",
        terminal_safe(&plan.artifact.manifest_path.display().to_string())
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  network:  {}",
        terminal_safe(&plan.artifact.manifest.network_name)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  ACME:     {}",
        terminal_safe(&plan.artifact.manifest.acme_volume_name)
    )
    .map_err(output_error)?;
    for (file, path) in plan
        .artifact
        .manifest
        .support_files
        .iter()
        .zip(&plan.artifact.support_paths)
    {
        writeln!(
            writer,
            "  support:  {} -> {} mode {} ({})",
            terminal_safe(&path.display().to_string()),
            terminal_safe(&file.destination),
            terminal_safe(&file.mode),
            terminal_safe(&file.digest)
        )
        .map_err(output_error)?;
    }
    for step in &plan.artifact.manifest.steps {
        writeln!(writer, "  step:     {}", step.label()).map_err(output_error)?;
    }
    writeln!(
        writer,
        "  this command only plans; apply the reviewed ingress through `mm deploy`"
    )
    .map_err(output_error)
}

pub(crate) fn target_transport(transport: &TargetTransport) -> String {
    match transport {
        TargetTransport::Local => "local".to_owned(),
        TargetTransport::Ssh {
            host, port, user, ..
        } => format!(
            "ssh {}@{}:{}",
            terminal_safe(user),
            terminal_safe(host),
            port
        ),
    }
}

pub(crate) fn write_target_trust(
    review: &crate::composition::TargetTrustReview,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(review);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    let show = |label: &str, pin: Option<&crate::domain::repo_target::TargetPin>| {
        pin.map_or_else(
            || format!("  {label}: nothing pinned yet"),
            |pin| {
                let mut text = format!("  {label}:");
                if let (Some(host_key), Some(daemon)) = (&pin.host_key, &pin.daemon_fingerprint) {
                    text.push_str(&format!(
                        "\n    host key: {}\n    daemon:   {}",
                        terminal_safe(host_key),
                        terminal_safe(daemon)
                    ));
                }
                for node in &pin.nodes {
                    text.push_str(&format!(
                        "\n    {}:\n      host key: {}\n      daemon:   {}",
                        terminal_safe(&node.name),
                        terminal_safe(&node.host_key),
                        terminal_safe(&node.daemon_fingerprint)
                    ));
                }
                if pin.host_key.is_none() && pin.nodes.is_empty() {
                    text.push_str(" no node has recorded keys yet");
                }
                text
            },
        )
    };
    writeln!(writer, "Target {}", terminal_safe(&review.target)).map_err(output_error)?;
    writeln!(writer, "{}", show("pinned", review.pinned.as_ref())).map_err(output_error)?;
    writeln!(writer, "{}", show("repo", Some(&review.repo))).map_err(output_error)?;
    if review.trusted {
        writeln!(writer, "Pinned the repo's keys for this target.").map_err(output_error)
    } else if review.pinned.as_ref() == Some(&review.repo) {
        writeln!(writer, "The repo matches what you pinned.").map_err(output_error)
    } else if review
        .pinned
        .as_ref()
        .is_some_and(|pinned| pinned.covers(&review.repo))
    {
        writeln!(
            writer,
            "The repo only drops or reorders keys you pinned; nothing to review."
        )
        .map_err(output_error)
    } else {
        writeln!(
            writer,
            "Verify the new keys with the host's admin, then run `mm target trust {} --yes`.",
            terminal_safe(&review.target)
        )
        .map_err(output_error)
    }
}
