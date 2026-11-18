use super::*;

pub(super) fn configured_service_names(
    document: &ConfigDocument,
    stack_name: &str,
    environment: &str,
) -> Vec<String> {
    document
        .config
        .modules
        .values()
        .flat_map(|module| {
            module
                .services
                .iter()
                .filter(|(_, config)| config.stack == stack_name && config.is_active(environment))
                .map(|(service, _)| service.clone())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// The `role: job` services of `stack_name` that `environment` has here.
pub(super) fn job_only_service_names(
    document: &ConfigDocument,
    stack_name: &str,
    environment: &str,
) -> BTreeSet<String> {
    document
        .config
        .modules
        .values()
        .flat_map(|module| &module.services)
        .filter(|(_, config)| {
            config.stack == stack_name && config.is_job_only() && config.is_active(environment)
        })
        .map(|(service, _)| service.clone())
        .collect()
}

pub(super) fn service_snapshots(
    document: &ConfigDocument,
    rendered: &RenderedProject,
    observed: ObservedProject,
    job_only: &BTreeSet<String>,
) -> Vec<ServiceSnapshot> {
    let modules = document
        .config
        .modules
        .iter()
        .flat_map(|(module_name, module)| {
            module
                .services
                .iter()
                .filter(|(_, config)| config.stack == rendered.stack.as_str())
                .map(move |(service, _)| (service.clone(), module_name.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let normalized = observed
        .services
        .into_iter()
        .map(|service| (service.identity.service.clone(), service))
        .collect::<BTreeMap<_, _>>();
    let mut containers = BTreeMap::<String, Vec<ContainerObservation>>::new();
    for container in observed.containers {
        containers
            .entry(container.service.clone())
            .or_default()
            .push(container);
    }
    let service_names = rendered
        .service_names
        .iter()
        .cloned()
        .chain(containers.keys().cloned())
        .chain(job_only.iter().cloned())
        .collect::<BTreeSet<_>>();

    service_names
        .into_iter()
        .map(|name| {
            let mut service_containers = containers.remove(&name).unwrap_or_default();
            mark_history(&mut service_containers);
            let observation = normalized.get(&name);
            let desired_count = observation.map_or(1, |service| service.desired_count);
            let running_count = observation.map_or(0, |service| service.running_count);
            let healthy_count = observation.map_or(0, |service| service.healthy_count);
            let mut service = ServiceSnapshot {
                stack: rendered.stack.clone(),
                module: modules.get(&name).cloned(),
                nodes: Vec::new(),
                state: observation.map_or_else(
                    || classify_service(&service_containers),
                    |service| service.state,
                ),
                starting: false,
                desired_count,
                running_count,
                healthy_count,
                image: service_containers
                    .iter()
                    .find(|container| !container.history)
                    .or(service_containers.first())
                    .map(|container| container.image.clone()),
                revision: observation
                    .and_then(|service| service.revision.clone())
                    .or_else(|| {
                        service_containers
                            .iter()
                            .find_map(|container| container.revision.clone())
                    }),
                restart_count: service_containers
                    .iter()
                    .filter(|container| !container.history)
                    .filter_map(|container| container.restart_count)
                    .max(),
                exit_code: observation.and_then(|service| service.exit_code),
                created_at: observation.and_then(|service| service.created_at.clone()),
                started_at: observation.and_then(|service| service.started_at.clone()),
                finished_at: observation.and_then(|service| service.finished_at.clone()),
                diagnostic_code: observation.and_then(|service| service.diagnostic_code.clone()),
                error: service_containers
                    .iter()
                    .filter(|container| !container.history)
                    .find_map(|container| container.error.clone()),
                containers: service_containers,
                name,
                scale: None,
                restarts: None,
            };
            if job_only.contains(&service.name) {
                service.state = ServiceState::JobOnly;
                service.desired_count = 0;
            }
            service.note_starting();
            service
        })
        .collect()
}

/// Marks the containers that say nothing about the service as it is now:
/// one that a newer container replaced, one that ran once for a job.
fn mark_history(containers: &mut [ContainerObservation]) {
    let current = {
        let all = containers.iter().collect::<Vec<_>>();
        crate::domain::runtime::current_containers(&all)
            .into_iter()
            .map(|container| container.id.clone())
            .collect::<BTreeSet<_>>()
    };
    for container in containers {
        container.history = !current.contains(&container.id);
    }
}

pub(super) fn classify_service(containers: &[ContainerObservation]) -> ServiceState {
    crate::domain::runtime::tally_service(&containers.iter().collect::<Vec<_>>()).state
}

pub(super) fn user_runtime_target(
    target: &crate::domain::runtime::RuntimeTarget,
) -> Result<crate::domain::runtime::RuntimeTarget, MmError> {
    if target.user_compose_file_count == 0
        || target.user_compose_file_count > target.compose_files.len()
    {
        return Err(MmError::RuntimeResponse {
            message: format!(
                "stack '{}' has an invalid user Compose file boundary",
                target.stack_name
            ),
        });
    }
    let mut user_target = target.clone();
    user_target
        .compose_files
        .truncate(target.user_compose_file_count);
    Ok(user_target)
}

pub(super) fn require_capabilities(
    runtime: &mut RuntimeProbe,
    required: &[RuntimeCapability],
) -> bool {
    let missing = required
        .iter()
        .filter(|capability| !runtime.capabilities.contains(capability))
        .copied()
        .collect::<Vec<_>>();
    for capability in &missing {
        runtime.diagnostics.push(Diagnostic::error(
            "MM_RUNTIME_CAPABILITY_MISSING",
            format!("runtime does not provide required capability {capability:?}"),
        ));
    }
    missing.is_empty()
}

pub(super) fn target_blocks_runtime(validation: &ValidationReport) -> bool {
    validation.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.code.as_str(),
            "MM_TARGET_NAME_INVALID"
                | "MM_TARGET_UNAVAILABLE"
                | "MM_RUNTIME_TARGET_TRANSPORT_UNSUPPORTED"
        )
    })
}

pub(super) fn skipped_target_runtime(kind: crate::domain::runtime::RuntimeKind) -> RuntimeProbe {
    RuntimeProbe::unavailable(
        kind,
        Diagnostic::error(
            "MM_RUNTIME_PROBE_SKIPPED",
            "runtime probe was skipped because the selected target cannot be inspected locally",
        ),
    )
}

#[cfg(test)]
mod tests;
