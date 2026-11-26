//! Services on several nodes that set no `update.order` start first when
//! nothing of the host stands in the way (`domain::rollout::auto_order`).

use serde_yaml_ng::Value;

use crate::domain::config::{ConfigDocument, UpdateFailure, UpdateOrder};
use crate::domain::error::MmError;
use crate::domain::rollout::{HostTies, RolloutPolicy, ServiceUpdate, starts_first_by_default};

/// Makes each service of `policy` that the defaults let start first a
/// `start-first` one.
pub(super) fn start_replicated_first(
    document: &ConfigDocument,
    environment: &str,
    policy: &mut RolloutPolicy,
) -> Result<(), MmError> {
    for module in document.config.modules.values() {
        for (name, service) in &module.services {
            if !service.is_active(environment)
                || service.is_job_only()
                || service
                    .update
                    .as_ref()
                    .is_some_and(|update| update.order.is_some())
                || service.replicas.is_some()
                || service.autoscale.is_some()
                || service.host_network.is_some()
            {
                continue;
            }
            let nodes = service.placed_on.as_ref().map_or(0, Vec::len);
            if nodes < 2 || !starts_first_by_default(nodes, &ties(document, &service.stack, name)?)
            {
                continue;
            }
            match (policy.services.iter_mut())
                .find(|update| update.stack == service.stack && update.service == *name)
            {
                Some(update) => update.order = UpdateOrder::StartFirst,
                None => policy.services.push(ServiceUpdate {
                    stack: service.stack.clone(),
                    service: name.clone(),
                    order: UpdateOrder::StartFirst,
                    parallelism: 1,
                    failure: UpdateFailure::default(),
                }),
            }
        }
    }
    policy.services.sort();
    Ok(())
}

/// What `service` takes of its host in any of the stack's Compose files.
fn ties(document: &ConfigDocument, stack: &str, service: &str) -> Result<HostTies, MmError> {
    let mut ties = HostTies::default();
    let Some(config) = document.config.stacks.get(stack) else {
        return Ok(ties);
    };
    for file in &config.files {
        let path = document.root.join(file);
        let text = std::fs::read_to_string(&path).map_err(|source| MmError::Io {
            context: format!("reading {}", path.display()),
            source,
        })?;
        let model: Value =
            serde_yaml_ng::from_str(&text).map_err(|error| MmError::ConfigParse {
                path: path.clone(),
                message: error.to_string(),
            })?;
        let Some(definition) = model
            .get("services")
            .and_then(|services| services.get(service))
        else {
            continue;
        };
        ties = ties.with(HostTies::of(definition));
    }
    Ok(ties)
}
