//! Which services of a stack's Compose files see processes beyond their own
//! containers, and the AppArmor profile each names.

use std::collections::{BTreeMap, BTreeSet};

use serde_yaml_ng::Value;

/// A service whose `pid` is the host's, another container's, or that of a
/// service that is one of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedPidService {
    pub pid: String,
    /// The profile its `security_opt` names with `apparmor=` or `apparmor:`.
    pub apparmor: Option<String>,
}

/// The pid-sharing services of `files`, Compose files in the order Compose
/// applies them, each parsed with its YAML merge keys applied.
#[must_use]
pub fn shared_pid_services(files: Vec<Value>) -> BTreeMap<String, SharedPidService> {
    let mut services = BTreeMap::<String, (Option<String>, Option<String>)>::new();
    for mut model in files {
        if let Some(compose) = model.as_mapping_mut() {
            crate::domain::compose_extends::resolve_extends(compose);
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
                entry.0 = Some(pid.to_owned());
            }
            if let Some(profile) = apparmor_option(service) {
                entry.1 = Some(profile);
            }
        }
    }
    let mut sharing = BTreeSet::new();
    loop {
        let before = sharing.len();
        for (name, (pid, _)) in &services {
            let Some(pid) = pid.as_deref() else {
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
    services
        .into_iter()
        .filter(|(name, _)| sharing.contains(name))
        .map(|(name, (pid, apparmor))| {
            (
                name,
                SharedPidService {
                    pid: pid.unwrap_or_default(),
                    apparmor,
                },
            )
        })
        .collect()
}

/// Refuses rendered Compose files that would start a pid-sharing service
/// under Podman's default where that default is mm-containers: a service
/// that names no profile, or names mm-containers.
pub fn refuse_unconfined_shared_pid(files: Vec<Value>) -> Result<(), String> {
    for (name, service) in shared_pid_services(files) {
        if service
            .apparmor
            .as_deref()
            .is_none_or(|profile| profile == super::MM_CONTAINER_PROFILE)
        {
            return Err(format!(
                "service '{name}' sees processes beyond its own containers (`pid: {}`), and the files mm rendered for it give it no AppArmor profile, so on this host it would run under {}. Deploy the environment again (`mm deploy plan`): it renders the service with {}",
                service.pid,
                super::MM_CONTAINER_PROFILE,
                super::MM_SHARED_PID_PROFILE
            ));
        }
    }
    Ok(())
}

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
