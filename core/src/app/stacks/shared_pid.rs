//! Which services share a process namespace beyond their own containers,
//! and the AppArmor profile mm gives them.

use std::collections::{BTreeMap, BTreeSet};

use serde_yaml_ng::Value;

use crate::domain::error::MmError;
use crate::domain::remote::{MM_CONTAINER_PROFILE, MM_SHARED_PID_PROFILE, SharedPidAppArmor};
use crate::domain::stack::CompiledComposeSource;

/// The `security_opt` entry that gives a service the shared-pid profile.
pub(super) const SHARED_PID_APPARMOR: &str = "apparmor=mm-containers-shared-pid";

#[derive(Default)]
struct Sharing {
    pid: Option<String>,
    apparmor: Option<String>,
}

/// The services mm gives `mm-containers-shared-pid` on a host that
/// confines them as `host` says: those whose `pid` is the host's, another
/// container's, or that of a service that is one of them, and that choose
/// no AppArmor profile themselves. Refused when such a service chooses
/// mm-containers, or when the host would give it mm-containers.
pub(super) fn services_sharing_pid(
    sources: &[CompiledComposeSource],
    host: SharedPidAppArmor,
) -> Result<BTreeSet<String>, MmError> {
    let services = compose_services(sources)?;
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
    let mut pinned = BTreeSet::new();
    for name in sharing {
        let service = &services[&name];
        let pid = service.pid.as_deref().unwrap_or_default();
        match service.apparmor.as_deref() {
            Some(MM_CONTAINER_PROFILE) => {
                return Err(refusal(format!(
                    "service '{name}' sees processes beyond its own containers (`pid: {pid}`) and sets `security_opt: apparmor={MM_CONTAINER_PROFILE}`, whose rules let it signal and read every process it sees. Remove the option: on Podman mm gives such a service {MM_SHARED_PID_PROFILE}, which keeps Podman's rules"
                )));
            }
            Some(_) => {}
            None => match host {
                SharedPidAppArmor::Pin => {
                    pinned.insert(name);
                }
                SharedPidAppArmor::Missing => {
                    return Err(refusal(format!(
                        "service '{name}' sees processes beyond its own containers (`pid: {pid}`), and this host gives containers {MM_CONTAINER_PROFILE} but has not loaded {MM_SHARED_PID_PROFILE}, which keeps Podman's rules for such a service. Run `mm target setup plan` and its apply to install it"
                    )));
                }
                SharedPidAppArmor::NotConfined => {}
            },
        }
    }
    Ok(pinned)
}

fn refusal(message: String) -> MmError {
    MmError::Remote {
        code: "MM_PODMAN_APPARMOR",
        message,
    }
}

fn compose_services(
    sources: &[CompiledComposeSource],
) -> Result<BTreeMap<String, Sharing>, MmError> {
    let mut services = BTreeMap::<String, Sharing>::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        if let Some(compose) = model.as_mapping_mut() {
            super::guardrails::resolve_extends(compose);
        }
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
            if let Some(profile) = apparmor_option(service) {
                entry.apparmor = Some(profile);
            }
        }
    }
    Ok(services)
}

/// The profile a service's `security_opt` names with `apparmor=` or
/// `apparmor:`.
fn apparmor_option(service: &Value) -> Option<String> {
    service
        .get("security_opt")
        .and_then(Value::as_sequence)?
        .iter()
        .rev()
        .filter_map(Value::as_str)
        .find_map(|option| {
            let (key, value) = option.trim().split_once(['=', ':'])?;
            key.trim()
                .eq_ignore_ascii_case("apparmor")
                .then(|| value.trim().to_owned())
        })
}
