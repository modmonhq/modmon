//! Reads which AppArmor profile rootful Podman gives this host's containers.

use std::io::Read;
use std::path::Path;

use crate::domain::remote::{ContainerAppArmor, MM_SHARED_PID_PROFILE};

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
    let loaded = !profile.is_empty()
        && read(Path::new(LOADED))
            .is_some_and(|profiles| ContainerAppArmor::is_loaded(&profiles, &profile));
    Some(ContainerAppArmor { profile, loaded })
}

/// Whether the kernel runs AppArmor and holds the profile setup installs
/// for containers that share a process namespace beyond their own.
#[must_use]
pub(crate) fn holds_shared_pid_profile() -> bool {
    read(Path::new(ENABLED)).is_some_and(|enabled| enabled.trim_start().starts_with('Y'))
        && read(Path::new(LOADED))
            .is_some_and(|profiles| ContainerAppArmor::is_loaded(&profiles, MM_SHARED_PID_PROFILE))
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
