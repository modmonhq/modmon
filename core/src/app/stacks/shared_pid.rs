//! The AppArmor profile mm gives services that share a process namespace
//! beyond their own containers.

use std::collections::BTreeSet;

use serde_yaml_ng::Value;

use crate::domain::error::MmError;
use crate::domain::remote::{
    MM_CONTAINER_PROFILE, MM_SHARED_PID_PROFILE, SharedPidAppArmor, shared_pid_services,
};
use crate::domain::stack::CompiledComposeSource;

/// The `security_opt` entry that gives a service the shared-pid profile.
pub(super) const SHARED_PID_APPARMOR: &str = "apparmor=mm-containers-shared-pid";

/// The services mm gives `mm-containers-shared-pid` on a host that
/// confines them as `host` says: the pid-sharing services that choose no
/// AppArmor profile themselves. Refused when such a service chooses
/// mm-containers, or when the host would give it mm-containers.
pub(super) fn services_sharing_pid(
    sources: &[CompiledComposeSource],
    host: SharedPidAppArmor,
) -> Result<BTreeSet<String>, MmError> {
    let mut models = Vec::new();
    for source in sources {
        let invalid = |cause: serde_yaml_ng::Error| MmError::Environment {
            message: format!("Compose file '{}': {cause}", source.manifest.path.display()),
        };
        let mut model: Value = serde_yaml_ng::from_slice(&source.bytes).map_err(invalid)?;
        model.apply_merge().map_err(invalid)?;
        models.push(model);
    }
    let mut pinned = BTreeSet::new();
    for (name, service) in shared_pid_services(models) {
        let pid = &service.pid;
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
