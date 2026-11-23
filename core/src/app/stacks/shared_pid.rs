//! Which services share a process namespace beyond their own containers.

use std::collections::{BTreeMap, BTreeSet};

use serde_yaml_ng::Value;

use crate::domain::error::MmError;
use crate::domain::stack::CompiledComposeSource;

/// The `security_opt` entry that gives a service the shared-pid profile.
pub(super) const SHARED_PID_APPARMOR: &str = "apparmor=mm-containers-shared-pid";

#[derive(Default)]
struct Sharing {
    pid: Option<String>,
    own_apparmor: bool,
}

/// The services of the stack's Compose files whose `pid` is the host's,
/// another container's, or that of a service that is one of them, and
/// that choose no AppArmor profile themselves.
pub(super) fn services_sharing_pid(
    sources: &[CompiledComposeSource],
) -> Result<BTreeSet<String>, MmError> {
    let mut services = BTreeMap::<String, Sharing>::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        let Some(listed) = model.get("services").and_then(Value::as_mapping) else {
            continue;
        };
        for (name, service) in listed {
            let Some(name) = name.as_str() else {
                continue;
            };
            let entry = services.entry(name.to_owned()).or_default();
            if let Some(pid) = service.get("pid").and_then(Value::as_str) {
                entry.pid = Some(pid.to_owned());
            }
            entry.own_apparmor |= service
                .get("security_opt")
                .and_then(Value::as_sequence)
                .is_some_and(|options| {
                    options.iter().filter_map(Value::as_str).any(|option| {
                        option
                            .trim_start()
                            .to_ascii_lowercase()
                            .starts_with("apparmor")
                    })
                });
        }
    }
    let mut sharing = BTreeSet::new();
    loop {
        let before = sharing.len();
        for (name, service) in &services {
            let Some(pid) = service.pid.as_deref() else {
                continue;
            };
            if pid == "host"
                || pid.starts_with("container:")
                || pid
                    .strip_prefix("service:")
                    .is_some_and(|other| sharing.contains(other))
            {
                sharing.insert(name.clone());
            }
        }
        if sharing.len() == before {
            break;
        }
    }
    Ok(sharing
        .into_iter()
        .filter(|name| {
            services
                .get(name)
                .is_some_and(|service| !service.own_apparmor)
        })
        .collect())
}
