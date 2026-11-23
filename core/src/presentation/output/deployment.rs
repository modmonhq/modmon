use super::*;

pub(crate) fn write_deployment_plan(
    plan: &DeploymentPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    deployment_plan_text(&mut stdout.lock(), plan)
}

/// The text `mm deploy plan` prints for one engine's plan.
pub(crate) fn deployment_plan_text(
    writer: &mut impl Write,
    plan: &DeploymentPlan,
) -> Result<(), MmError> {
    writeln!(writer, "Deployment plan {}", terminal_safe(&plan.plan_hash)).map_err(output_error)?;
    writeln!(
        writer,
        "  {} [{}] -> {}",
        terminal_safe(&plan.project_name),
        terminal_safe(&plan.environment),
        terminal_safe(&plan.target_name)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  environment delivery: {} (from {})",
        plan.environment_review.delivery.mode,
        plan.environment_review.delivery.provenance.as_str()
    )
    .map_err(output_error)?;
    write_receipt_origin_line(writer, plan.build_receipt.as_ref())?;
    if let Some(build) = &plan.local_build {
        writeln!(
            writer,
            "  builds {}",
            terminal_safe(&local_build_line(build))
        )
        .map_err(output_error)?;
    }
    for line in super::release::release_lines(plan.release.as_ref()) {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    // A cluster plan says this once, for all its nodes.
    if plan.node.is_none() {
        for line in super::rollout::policy_lines(&plan.rollout) {
            writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
        }
    }
    write_stack_lines(writer, plan)?;
    for change in &plan.runtime_changes {
        writeln!(writer, "  security: {}", terminal_safe(&change.line())).map_err(output_error)?;
    }
    for line in super::config_files::config_file_lines(&plan.config_files) {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    for stack in &plan.removed_stacks {
        writeln!(
            writer,
            "  remove stack: {} ({})",
            terminal_safe(&stack.stack),
            terminal_safe(&stack.compose_project_name)
        )
        .map_err(output_error)?;
    }
    write_excluded_services_to(writer, &plan.excluded_services, &plan.environment)?;
    if let Some(line) = plan.adopted_routes_line() {
        writeln!(writer, "  routes: {}", terminal_safe(&line)).map_err(output_error)?;
    }
    for file in &plan.secret_file_owners {
        writeln!(writer, "  note: {}", terminal_safe(&file.line())).map_err(output_error)?;
    }
    for check in &plan.healthcheck_arguments {
        writeln!(writer, "  warning: {}", terminal_safe(&check.line())).map_err(output_error)?;
        writeln!(writer, "    {}", terminal_safe(&check.help())).map_err(output_error)?;
    }
    // A cluster node's routes are the cluster plan's ingress section.
    if plan.node.is_some() {
        return Ok(());
    }
    write_route_diff(writer, &plan.route_diff)?;
    write_ingress_statuses_to(writer, &plan.ingress)?;
    writeln!(
        writer,
        "  ingress: {}",
        if plan.removed_ingress.is_some() {
            "remove managed services"
        } else if plan.static_ingress.is_some() {
            "managed"
        } else {
            "no desired routes"
        }
    )
    .map_err(output_error)
}

/// Each stack with its images and, for a service that is hardened or runs
/// under a named runtime, what it runs with and what it leaves out.
fn write_stack_lines(writer: &mut impl Write, plan: &DeploymentPlan) -> Result<(), MmError> {
    for stack in &plan.stacks {
        writeln!(
            writer,
            "  stack:  {} ({})",
            terminal_safe(&stack.stack),
            terminal_safe(&stack.artifact_hash)
        )
        .map_err(output_error)?;
        for service in &stack.artifact.manifest.services {
            if let Some(image) = &service.image {
                writeln!(
                    writer,
                    "    image: {}/{} -> {}",
                    terminal_safe(&service.module),
                    terminal_safe(&service.service),
                    terminal_safe(image)
                )
                .map_err(output_error)?;
            }
            if service.is_job_only() {
                writeln!(
                    writer,
                    "    job only: {}/{}, no container until one of its jobs runs",
                    terminal_safe(&service.module),
                    terminal_safe(&service.service)
                )
                .map_err(output_error)?;
            }
        }
        // What each hardened service runs with, and what it leaves out.
        for line in stack
            .artifact
            .manifest
            .security
            .iter()
            .flat_map(crate::domain::hardening::ServiceSecurity::describe)
        {
            writeln!(writer, "    security: {}", terminal_safe(&line)).map_err(output_error)?;
        }
        let low_ports = &stack.artifact.manifest.low_ports;
        if !low_ports.is_empty() {
            writeln!(
                writer,
                "    ports: {} may listen below 1024 as any user, as on Docker Engine (net.ipv4.ip_unprivileged_port_start=0)",
                terminal_safe(&low_ports.join(", "))
            )
            .map_err(output_error)?;
        }
        for service in &stack.artifact.manifest.host_network {
            writeln!(writer, "    network: {}", terminal_safe(&service.line()))
                .map_err(output_error)?;
        }
        for path in &stack.artifact.manifest.masked_paths {
            writeln!(writer, "    {}", terminal_safe(&path.line())).map_err(output_error)?;
        }
    }
    Ok(())
}

fn write_route_diff(
    writer: &mut impl Write,
    diff: &crate::domain::deployment::DeploymentRouteDiff,
) -> Result<(), MmError> {
    for route in &diff.added {
        write_deployment_route(writer, "+", route)?;
    }
    for route in &diff.changed {
        writeln!(
            writer,
            "  ~ {}{}: {} -> {}",
            terminal_safe(&route.host),
            terminal_safe(&route.path_prefix),
            route_destination(&route.before),
            route_destination(&route.after)
        )
        .map_err(output_error)?;
    }
    for route in &diff.removed {
        write_deployment_route(writer, "-", route)?;
    }
    if diff.added.is_empty() && diff.changed.is_empty() && diff.removed.is_empty() {
        writeln!(writer, "  routes: unchanged").map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_remote_deployment_plan(
    plan: &crate::domain::remote::RemoteDeploymentPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    remote_deployment_plan_text(&mut stdout.lock(), plan)
}

/// The text `mm deploy plan` prints for a target that names one host.
pub(crate) fn remote_deployment_plan_text(
    writer: &mut impl Write,
    plan: &crate::domain::remote::RemoteDeploymentPlan,
) -> Result<(), MmError> {
    writeln!(
        writer,
        "Remote deployment plan {}",
        terminal_safe(&plan.binding.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  target:   {} ({})",
        terminal_safe(&plan.binding.target),
        terminal_safe(&plan.binding.target_fingerprint)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  daemon:   {} protocol={} version={}",
        terminal_safe(&plan.binding.daemon_fingerprint),
        plan.binding.protocol_version,
        terminal_safe(&plan.binding.daemon_version)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  scope:    {} / {}",
        plan.binding.project_id.0,
        terminal_safe(&plan.binding.environment)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  commit:   {}",
        terminal_safe(&plan.binding.commit)
    )
    .map_err(output_error)?;
    if let Some(bundle_hash) = &plan.binding.bundle_hash {
        writeln!(
            writer,
            "  bundle:   {} (pushed by mm; the host fetched nothing)",
            terminal_safe(bundle_hash)
        )
        .map_err(output_error)?;
    }
    writeln!(
        writer,
        "  expires:  {}",
        crate::domain::rollout::utc_time(plan.binding.expires_at_unix_ms)
    )
    .map_err(output_error)?;
    write_receipt_lines(writer, plan.receipt.as_ref())?;
    write_access_change_lines(writer, &plan.access)?;
    let jobs = plan
        .plan
        .iter()
        .flat_map(|deployment| &deployment.rollout.jobs)
        .map(|job| (None, job))
        .collect::<Vec<_>>();
    for line in super::rollout::job_lines(&jobs, &plan.once) {
        writeln!(writer, "{}", terminal_safe(&line)).map_err(output_error)?;
    }
    write_staged_lines(writer, &plan.staged)?;
    match &plan.plan {
        Some(deployment) => deployment_plan_text(writer, deployment),
        None => Ok(()),
    }
}

/// The staged secrets and variables a node's plan makes live.
pub(super) fn write_staged_lines(
    writer: &mut impl Write,
    staged: &[crate::domain::staged::StagedPlanItem],
) -> Result<(), MmError> {
    if staged.is_empty() {
        return Ok(());
    }
    writeln!(writer, "  staged, goes live with this deployment:").map_err(output_error)?;
    for item in staged {
        writeln!(writer, "    {}", terminal_safe(&item.line())).map_err(output_error)?;
    }
    Ok(())
}

/// The users-file change a plan applies before anything else.
pub(super) fn write_access_change_lines(
    writer: &mut impl Write,
    change: &crate::domain::users::AccessChange,
) -> Result<(), MmError> {
    let describe = |access: &Option<crate::domain::host_policy::EnvironmentAccess>| {
        access.as_ref().map_or_else(
            || "none".to_owned(),
            |access| {
                let mut text = access.level.as_str().to_owned();
                if !access.exec.is_none() {
                    text.push_str(" +exec");
                }
                if !access.forward.is_none() {
                    text.push_str(" +forward");
                }
                if access.secrets {
                    text.push_str(" +secrets");
                }
                text
            },
        )
    };
    if change.entries.is_empty() && change.deploy_from_before == change.deploy_from_after {
        writeln!(
            writer,
            "  access:   unchanged ({} user(s))",
            change.record.users.len()
        )
        .map_err(output_error)?;
        return Ok(());
    }
    writeln!(
        writer,
        "  access changes in {}:",
        terminal_safe(&change.environment)
    )
    .map_err(output_error)?;
    for entry in &change.entries {
        writeln!(
            writer,
            "    {}: {} -> {}{}{}",
            terminal_safe(&entry.user),
            describe(&entry.before),
            describe(&entry.after),
            if entry.keys_added.is_empty() {
                String::new()
            } else {
                format!(", +{} key(s)", entry.keys_added.len())
            },
            if entry.keys_removed.is_empty() {
                String::new()
            } else {
                format!(", -{} key(s)", entry.keys_removed.len())
            },
        )
        .map_err(output_error)?;
    }
    if change.deploy_from_before != change.deploy_from_after {
        let show = |value: &Option<Vec<String>>| {
            value
                .as_ref()
                .map_or_else(|| "any branch".to_owned(), |branches| branches.join(", "))
        };
        writeln!(
            writer,
            "    deploy-from: {} -> {}",
            terminal_safe(&show(&change.deploy_from_before)),
            terminal_safe(&show(&change.deploy_from_after))
        )
        .map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_access_plan(
    plan: &crate::domain::remote::RemoteAccessPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Team sync plan {}",
        terminal_safe(&plan.binding.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  target:   {}",
        terminal_safe(&plan.binding.target)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  commit:   {}",
        terminal_safe(&plan.binding.commit)
    )
    .map_err(output_error)?;
    write_access_change_lines(&mut writer, &plan.access)?;
    writeln!(
        writer,
        "Apply within ten minutes with `mm --environment {} team sync --plan {} --yes`.",
        terminal_safe(&plan.binding.environment),
        terminal_safe(&plan.binding.plan_hash)
    )
    .map_err(output_error)
}

pub(crate) fn write_access_change(
    change: &crate::domain::users::AccessChange,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(change);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Applied the users file of {} to {}",
        terminal_safe(&change.record.commit),
        terminal_safe(&change.environment)
    )
    .map_err(output_error)?;
    write_access_change_lines(&mut writer, change)
}

pub(crate) fn write_remote_identity(
    identity: &crate::domain::remote::RemoteIdentity,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(identity);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "{} in {}{}",
        terminal_safe(&identity.principal),
        terminal_safe(&identity.environment),
        if identity.break_glass {
            " (break-glass)"
        } else {
            ""
        }
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  key:      {}",
        terminal_safe(&identity.key_fingerprint)
    )
    .map_err(output_error)?;
    let access = identity.access.as_ref().map_or_else(
        || "none".to_owned(),
        |access| serde_json::to_string(access).unwrap_or_else(|_| access.level.as_str().to_owned()),
    );
    writeln!(writer, "  access:   {}", terminal_safe(&access)).map_err(output_error)?;
    writeln!(
        writer,
        "  users file from: {}",
        terminal_safe(
            identity
                .access_commit
                .as_deref()
                .unwrap_or("never reconciled")
        )
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  deployed:        {}",
        terminal_safe(identity.deployed_commit.as_deref().unwrap_or("nothing yet"))
    )
    .map_err(output_error)
}

pub(crate) fn write_team(
    document: &crate::domain::config::ConfigDocument,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&document.config.users);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if document.config.users.is_empty() {
        writeln!(
            writer,
            "No users yet. `mm team join` adds you with this device's key."
        )
        .map_err(output_error)?;
        return Ok(());
    }
    for (name, user) in &document.config.users {
        writeln!(
            writer,
            "{}{}",
            terminal_safe(name),
            if user.machine { " (machine)" } else { "" }
        )
        .map_err(output_error)?;
        for key in &user.keys {
            let fingerprint = crate::domain::host_policy::parse_public_key(key.line()).map_or_else(
                |message| format!("invalid: {message}"),
                |key| key.fingerprint,
            );
            let agent = if key.is_agent() { " (agent)" } else { "" };
            writeln!(writer, "  key {}{agent}", terminal_safe(&fingerprint))
                .map_err(output_error)?;
        }
        for (environment, access) in &user.access {
            let access = access.access();
            writeln!(
                writer,
                "  {}: {}",
                terminal_safe(environment),
                access.level.as_str()
            )
            .map_err(output_error)?;
        }
    }
    Ok(())
}

pub(crate) fn write_development_mount_plan(
    plan: &crate::domain::development_mount::DevelopmentMountPlan,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(plan);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Development mount plan {}",
        terminal_safe(&plan.plan_hash)
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  module: {} [{}] -> {} ({:?})",
        terminal_safe(&plan.proposal.module),
        terminal_safe(&plan.environment),
        terminal_safe(&plan.target),
        plan.runtime
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  confidence: {:?}; configured: {}",
        plan.proposal.confidence, plan.proposal.configured
    )
    .map_err(output_error)?;
    if let Some(intent) = &plan.proposal.intent {
        for (source, target) in intent.sources() {
            writeln!(
                writer,
                "  bind: {} -> {} ({:?})",
                terminal_safe(&source.display().to_string()),
                terminal_safe(target),
                intent.mode
            )
            .map_err(output_error)?;
        }
        writeln!(
            writer,
            "  services: {}",
            terminal_safe(&intent.services.join(", "))
        )
        .map_err(output_error)?;
        for path in &intent.preserve {
            writeln!(writer, "  preserve: {}", terminal_safe(&path.path)).map_err(output_error)?;
        }
    } else {
        writeln!(
            writer,
            "  bind: unavailable; provide an explicit --mount-target"
        )
        .map_err(output_error)?;
    }
    for evidence in &plan.proposal.evidence {
        writeln!(
            writer,
            "  evidence: {:?} {}{} -> {}",
            evidence.source,
            terminal_safe(&evidence.location),
            evidence
                .service
                .as_deref()
                .map(|service| format!(" ({})", terminal_safe(service)))
                .unwrap_or_default(),
            terminal_safe(&evidence.target)
        )
        .map_err(output_error)?;
    }
    for reason in &plan.proposal.confirmations {
        writeln!(writer, "  confirm: {}", terminal_safe(reason)).map_err(output_error)?;
    }
    for warning in &plan.proposal.warnings {
        writeln!(writer, "  note: {}", terminal_safe(warning)).map_err(output_error)?;
    }
    Ok(())
}

pub(crate) fn write_deployment_result(
    result: &DeploymentResult,
    format: OutputFormat,
) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(result);
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    writeln!(
        writer,
        "Applied deployment {} to {} [{}]",
        terminal_safe(&result.plan_hash),
        terminal_safe(&result.target_name),
        terminal_safe(&result.environment)
    )
    .map_err(output_error)?;
    if !result.removed_stacks.is_empty() {
        writeln!(
            writer,
            "  removed stacks: {}",
            terminal_safe(&result.removed_stacks.join(", "))
        )
        .map_err(output_error)?;
    }
    writeln!(
        writer,
        "  stacks: {}",
        terminal_safe(&result.applied_stacks.join(", "))
    )
    .map_err(output_error)?;
    writeln!(
        writer,
        "  routes: {} (generation {})",
        result.route_count, result.route_generation
    )
    .map_err(output_error)
}

pub(crate) fn write_stopped_stacks(stacks: &[String], format: OutputFormat) -> Result<(), MmError> {
    if format == OutputFormat::Json {
        return write_json(&serde_json::json!({ "stopped_stacks": stacks }));
    }
    let stdout = crate::presentation::record::stdout();
    let mut writer = stdout.lock();
    if stacks.is_empty() {
        return writeln!(writer, "No stacks are configured; nothing to stop.")
            .map_err(output_error);
    }
    writeln!(
        writer,
        "Stopped stacks: {}\nVolumes, routes, and overrides were kept; run `mm dev up` to start again.",
        terminal_safe(&stacks.join(", "))
    )
    .map_err(output_error)
}

pub(crate) fn write_deployment_route(
    writer: &mut impl Write,
    prefix: &str,
    route: &DeploymentRouteSnapshot,
) -> Result<(), MmError> {
    let scheme = if route.behavior.tls { "https" } else { "http" };
    writeln!(
        writer,
        "  {prefix} {scheme}://{}{} -> {}",
        terminal_safe(&route.host),
        terminal_safe(&route.path_prefix),
        route_destination(&route.behavior)
    )
    .map_err(output_error)
}

pub(crate) fn route_destination(route: &DeploymentRouteBehavior) -> String {
    terminal_safe(&format!("{}/{}:{}", route.stack, route.service, route.port))
}
