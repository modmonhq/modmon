//! Reads which AppArmor profile rootful Podman gives this host's containers.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::domain::remote::ContainerAppArmor;

const ENABLED: &str = "/sys/module/apparmor/parameters/enabled";
const LOADED: &str = "/sys/kernel/security/apparmor/profiles";
const CONFIG_ROOTS: [&str; 2] = ["/usr/share/containers", "/etc/containers"];
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_DROP_INS: usize = 256;

/// The profile Podman's containers get, or `None` when the kernel does not
/// run AppArmor or the files cannot be read.
#[must_use]
pub(crate) fn container_apparmor() -> Option<ContainerAppArmor> {
    if !read(Path::new(ENABLED))?.trim_start().starts_with('Y') {
        return None;
    }
    let files = config_files()
        .iter()
        .filter_map(|path| read(path))
        .collect::<Vec<_>>();
    let profile = ContainerAppArmor::configured_profile(files.iter().map(String::as_str));
    let loaded = !profile.is_empty()
        && read(Path::new(LOADED))
            .is_some_and(|profiles| ContainerAppArmor::is_loaded(&profiles, &profile));
    Some(ContainerAppArmor { profile, loaded })
}

fn config_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in CONFIG_ROOTS.map(Path::new) {
        files.push(root.join("containers.conf"));
        for directory in ["containers.conf.d", "containers.rootful.conf.d"] {
            files.extend(drop_ins(&root.join(directory)));
        }
    }
    files
}

fn drop_ins(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "conf")
        })
        .take(MAX_DROP_INS)
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn read(path: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}
