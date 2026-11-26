//! The jobs a deployment runs, and what has to run before them.
//!
//! A `before-deploy` job runs once the services its own service depends on
//! (its `depends_on` in the stack's Compose files, and theirs) are healthy,
//! and before its service is started or replaced. Those services start
//! first, everywhere they run; the rest of the stack follows the job.

use std::collections::{BTreeMap, BTreeSet};

use serde_yaml_ng::Value;

use crate::domain::config::{ConfigDocument, JobWhen};
use crate::domain::error::MmError;
use crate::domain::placement::PlacedService;
use crate::domain::rollout::RolloutPolicy;

/// The rollout policy of `environment` as this checkout (or this node's
/// view of it) declares it, with its deployment jobs.
pub(super) fn rollout_policy(
    document: &ConfigDocument,
    environment: &str,
) -> Result<RolloutPolicy, MmError> {
    let mut policy = RolloutPolicy::from_config(&document.config, environment)
        .map_err(|message| MmError::Environment { message })?;
    let mut ahead = BTreeSet::new();
    for job in document.config.jobs.values() {
        if job.run != Some(JobWhen::BeforeDeploy) || !job.runs_in(environment) {
            continue;
        }
        let Some(service) = document
            .config
            .modules
            .get(&job.module)
            .and_then(|module| module.services.get(&job.service))
            .filter(|service| service.is_in_environment(environment))
        else {
            continue;
        };
        for dependency in dependencies(document, &service.stack, &job.service)? {
            let here = document.config.modules.values().any(|module| {
                module.services.get(&dependency).is_some_and(|candidate| {
                    candidate.stack == service.stack && candidate.is_active(environment)
                })
            });
            if here {
                ahead.insert(PlacedService {
                    stack: service.stack.clone(),
                    service: dependency,
                });
            }
        }
    }
    policy.ahead = ahead.into_iter().collect();
    super::auto_order::start_replicated_first(document, environment, &mut policy)?;
    Ok(policy)
}

/// The stacks each stack follows through `after:`.
pub(super) fn stack_after(document: &ConfigDocument) -> BTreeMap<String, BTreeSet<String>> {
    (document.config.stacks.iter())
        .filter(|(_, stack)| !stack.after.is_empty())
        .map(|(name, stack)| (name.clone(), stack.after.iter().cloned().collect()))
        .collect()
}

/// What `service` depends on in `stack`, directly or through the services
/// it depends on, by the stack's Compose files.
fn dependencies(
    document: &ConfigDocument,
    stack: &str,
    service: &str,
) -> Result<BTreeSet<String>, MmError> {
    let Some(config) = document.config.stacks.get(stack) else {
        return Ok(BTreeSet::new());
    };
    let mut direct = BTreeMap::<String, BTreeSet<String>>::new();
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
        let Some(services) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, definition) in services {
            let (Some(name), Some(depends)) = (name.as_str(), definition.get("depends_on")) else {
                continue;
            };
            let names: Vec<&str> = match depends {
                Value::Mapping(map) => map.keys().filter_map(Value::as_str).collect(),
                Value::Sequence(items) => items.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            direct
                .entry(name.to_owned())
                .or_default()
                .extend(names.into_iter().map(str::to_owned));
        }
    }
    let mut found = BTreeSet::new();
    let mut next = vec![service.to_owned()];
    while let Some(current) = next.pop() {
        for dependency in direct.get(&current).into_iter().flatten() {
            if dependency != service && found.insert(dependency.clone()) {
                next.push(dependency.clone());
            }
        }
    }
    Ok(found)
}
