//! Reads which AppArmor profile rootful Podman gives this host's containers.

use std::io::Read;
use std::path::Path;

use crate::domain::error::MmError;
use crate::domain::remote::{
    ContainerAppArmor, MM_CONTAINER_PROFILE, MM_SHARED_PID_PROFILE, SharedPidAppArmor,
    refuse_unconfined_shared_pid,
};
use crate::domain::runtime::RuntimeTarget;

const ENABLED: &str = "/sys/module/apparmor/parameters/enabled";
const LOADED: &str = "/sys/kernel/security/apparmor/profiles";
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_DROP_INS: usize = 256;

/// The profile the containers of Podman `release` get, or `None` when the
/// kernel does not run AppArmor.
#[must_use]
pub(crate) fn container_apparmor(release: Option<(u32, u32)>) -> Option<ContainerAppArmor> {
    if !read(Path::new(ENABLED))?.trim_start().starts_with('Y') {
        return None;
    }
    let files = ContainerAppArmor::podman_config_files(
        release,
        &|path| Path::new(path).is_file(),
        &drop_ins,
    )
    .iter()
    .filter_map(|path| read(Path::new(path)))
    .collect::<Vec<_>>();
    let profile = ContainerAppArmor::configured_profile(files.iter().map(String::as_str));
    let profiles = read(Path::new(LOADED)).unwrap_or_default();
    let loaded = !profile.is_empty() && ContainerAppArmor::is_loaded(&profiles, &profile);
    let shared_pid_loaded = (profile == MM_CONTAINER_PROFILE)
        .then(|| ContainerAppArmor::is_loaded(&profiles, MM_SHARED_PID_PROFILE));
    Some(ContainerAppArmor {
        profile,
        loaded,
        shared_pid_loaded,
    })
}

/// How this host confines a container that shares a process namespace
/// beyond its own: with the shared-pid profile where the kernel holds it,
/// and not at all when Podman's default is not mm-containers under either
/// loader.
#[must_use]
pub(crate) fn shared_pid_apparmor() -> SharedPidAppArmor {
    if !read(Path::new(ENABLED)).is_some_and(|enabled| enabled.trim_start().starts_with('Y')) {
        return SharedPidAppArmor::NotConfined;
    }
    let profiles = read(Path::new(LOADED)).unwrap_or_default();
    if ContainerAppArmor::is_loaded(&profiles, MM_SHARED_PID_PROFILE) {
        return SharedPidAppArmor::Pin;
    }
    if gives_mm_profile() {
        SharedPidAppArmor::Missing
    } else {
        SharedPidAppArmor::NotConfined
    }
}

/// Refuses to create containers from `target`'s rendered files when rootful
/// Podman here gives containers mm-containers by default and the files
/// leave a pid-sharing service to that default: files rendered before setup
/// installed the profile, which a job, a rollback or a scale-up would reuse.
pub(crate) fn require_shared_pid_confined(target: &RuntimeTarget) -> Result<(), MmError> {
    if !rustix::process::geteuid().is_root() || !gives_mm_profile() {
        return Ok(());
    }
    let mut models = Vec::new();
    for path in &target.compose_files {
        let Some(text) = read(path) else {
            continue;
        };
        let mut model = match serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) {
            Ok(model) => model,
            Err(_) => continue,
        };
        if model.apply_merge().is_err() {
            continue;
        }
        models.push(model);
    }
    refuse_unconfined_shared_pid(models).map_err(|message| MmError::Remote {
        code: "MM_PODMAN_APPARMOR",
        message,
    })
}

fn gives_mm_profile() -> bool {
    [None, Some((6, 0))].into_iter().any(|release| {
        container_apparmor(release).is_some_and(|apparmor| apparmor.profile == MM_CONTAINER_PROFILE)
    })
}

fn drop_ins(directory: &str) -> Vec<String> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut names = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_dir()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .filter(|name| name.ends_with(".conf"))
        .collect::<Vec<_>>();
    names.sort();
    names.truncate(MAX_DROP_INS);
    names
}

fn read(path: &Path) -> Option<String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return None,
    };
    let mut text = String::new();
    match file.take(MAX_FILE_BYTES).read_to_string(&mut text) {
        Ok(_) => Some(text),
        Err(_) => None,
    }
}
