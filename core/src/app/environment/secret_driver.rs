//! How secret files reach a service's containers.

use crate::app::WorkspaceSelection;
use crate::domain::config::MmConfig;
use crate::domain::runtime::RuntimeKind;

/// A file per revision where mm knows the host ids of a container's users:
/// Docker Engine, and rootful Podman on hosts setup prepared, where `mmd`
/// gives each service ids of its own (it says so with `user_ids`; a
/// workstation names what its hosts use). Podman's own store elsewhere,
/// as rootless Podman on this machine.
pub(super) fn runtime_secret_driver(
    runtime: RuntimeKind,
    selection: &WorkspaceSelection,
    config: &MmConfig,
    environment: &str,
) -> &'static str {
    let known_ids = selection.user_ids.is_some()
        || crate::domain::user_namespace::on_prepared_hosts(
            config,
            environment,
            selection.target.as_deref(),
        );
    if runtime == RuntimeKind::PodmanCompose && !known_ids {
        "podman-native-secret"
    } else {
        "compose-file-secret"
    }
}
