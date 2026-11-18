use std::collections::BTreeMap;

use serde_yaml_ng::Value;

use super::*;
use crate::domain::config::ServiceRole;
use crate::domain::repo_target::MeshConfig;
use crate::domain::shared_storage::{VolumeName, needs_mesh, volume_directory};

const MAX_COMPOSE_FILE_BYTES: usize = 16 * 1024 * 1024;

/// On a cluster target a stack's `shared-volumes` are directories on the
/// storage node that every node mounts. Each one has to be a top-level
/// volume of the stack that mm may place (no `external`, no `name`), and
/// no database or queue may keep its data on one unless its service says
/// so. Elsewhere a shared volume is an ordinary volume and nothing is
/// checked.
pub(super) fn validate_shared_volumes(
    context: &ValidationContext<'_>,
    report: &mut ValidationReport,
) {
    let document = context.document;
    let environment = context.environment;
    let Some((target_name, target)) = crate::app::cluster_target(document, environment) else {
        return;
    };
    let mut refused_interface = false;
    for (stack_name, stack) in &document.config.stacks {
        if stack.shared_volumes.is_empty() {
            continue;
        }
        if let MeshConfig::Interface(interface) = &target.cluster.mesh {
            if !refused_interface {
                report.push(Diagnostic::error(
                    "MM_SHARED_VOLUME",
                    format!(
                        "stacks.{stack_name}.shared-volumes: {}",
                        needs_mesh(target_name, interface)
                    ),
                ));
            }
            refused_interface = true;
            continue;
        }
        // Missing and malformed files are reported by the stack checks.
        let Some(models) = compose_models(document, stack) else {
            continue;
        };
        let mut declared = BTreeMap::<&str, &Value>::new();
        for model in &models {
            if let Some(volumes) = model.get("volumes").and_then(Value::as_mapping) {
                for (name, volume) in volumes {
                    if let Some(name) = name.as_str() {
                        declared.insert(name, volume);
                    }
                }
            }
        }
        let users = crate::app::stacks::volume_users(&models);
        for volume in &stack.shared_volumes {
            let subject = SharedVolume {
                stack: stack_name,
                volume,
                target: target_name,
            };
            for problem in volume_problems(context, &subject, declared.get(volume.as_str())) {
                report.push(Diagnostic::error("MM_SHARED_VOLUME", problem));
            }
            for service in users.get(volume).into_iter().flatten() {
                if let Some(problem) = data_service_problem(context, &subject, service) {
                    report.push(Diagnostic::error("MM_SHARED_VOLUME", problem));
                }
            }
        }
    }
}

/// The stack's Compose files, or `None` when one cannot be read.
pub(super) fn compose_models(
    document: &ConfigDocument,
    stack: &crate::domain::config::StackConfig,
) -> Option<Vec<Value>> {
    let mut models = Vec::with_capacity(stack.files.len());
    for file in &stack.files {
        let Ok(Some(bytes)) = read_regular_file(&document.root.join(file), MAX_COMPOSE_FILE_BYTES)
        else {
            return None;
        };
        let Ok(mut model) = serde_yaml_ng::from_slice::<Value>(&bytes) else {
            return None;
        };
        if model.apply_merge().is_err() {
            return None;
        }
        models.push(model);
    }
    Some(models)
}

struct SharedVolume<'a> {
    stack: &'a str,
    volume: &'a str,
    target: &'a str,
}

/// What keeps mm from placing the volume: it is not declared, it cannot
/// name a directory, or the Compose file says where it is stored.
fn volume_problems(
    context: &ValidationContext<'_>,
    subject: &SharedVolume<'_>,
    declared: Option<&&Value>,
) -> Vec<String> {
    let SharedVolume { stack, volume, .. } = subject;
    let mut problems = Vec::new();
    if let Err(problem) = volume_directory(
        context.document.config.project.id,
        context.environment,
        &VolumeName::new(stack, volume),
    ) {
        problems.push(format!("stacks.{stack}.shared-volumes: {problem}"));
    }
    let Some(declared) = declared else {
        problems.push(format!(
            "stacks.{stack}.shared-volumes lists '{volume}', and no Compose file of stack '{stack}' declares a top-level volume of that name. Add `volumes: {{{volume}: {{}}}}` to the stack's Compose file and mount it in a service as `{volume}:/path`, or take '{volume}' out of shared-volumes"
        ));
        return problems;
    };
    for key in ["external", "name"] {
        if declared.get(key).is_some() {
            problems.push(format!(
                "shared volume '{volume}' of stack '{stack}' sets `{key}` in its Compose file. mm decides where a shared volume is stored (a directory on the cluster's storage node); remove `{key}`, or take '{volume}' out of stacks.{stack}.shared-volumes to keep the volume as it is"
            ));
        }
    }
    problems
}

/// Refuses a database or a queue that keeps its data on a shared volume,
/// unless its service allows it.
fn data_service_problem(
    context: &ValidationContext<'_>,
    subject: &SharedVolume<'_>,
    service: &str,
) -> Option<String> {
    let SharedVolume {
        stack,
        volume,
        target,
    } = subject;
    let (module, config) =
        context
            .document
            .config
            .modules
            .iter()
            .find_map(|(module, config)| {
                config
                    .services
                    .get(service)
                    .filter(|config| config.stack == *stack)
                    .map(|config| (module.as_str(), config))
            })?;
    if !config.is_in_environment(context.environment) || config.allow_shared_volumes == Some(true) {
        return None;
    }
    let role = match config.role? {
        ServiceRole::Database => "database",
        ServiceRole::Queue => "queue",
        _ => return None,
    };
    Some(format!(
        "service '{service}' has role {role} and mounts shared volume '{volume}' of stack '{stack}'. A shared volume is a network mount from the storage node of target '{target}': slower than local disk, without the locking guarantees a {role} needs, and unavailable while the storage node is down. Keep its data on an ordinary volume and pin the service to one node: take '{volume}' out of stacks.{stack}.shared-volumes and set `placement: <node>` under modules.{module}.services.{service}. To do it anyway, set `allow-shared-volumes: true` there"
    ))
}
