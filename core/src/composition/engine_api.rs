//! Which engine API this process reads from.

use crate::adapters::engine_api::EngineApi;

/// Set to `off` to ask the engine through its command line only.
pub const ENGINE_API_ENVIRONMENT: &str = "MM_ENGINE_API";

/// Where rootful Podman serves its Docker-compatible API (`podman.socket`).
const ROOTFUL_PODMAN_SOCKET: &str = "/run/podman/podman.sock";

fn reads_through_api() -> bool {
    std::env::var_os(ENGINE_API_ENVIRONMENT).is_none_or(|value| value != "off")
}

/// The local Docker engine's API socket, unless reads were told to use the
/// command line, or the Docker configuration names another engine.
pub(crate) fn local() -> Option<EngineApi> {
    if !reads_through_api() {
        return None;
    }
    EngineApi::local_docker()
}

/// Rootful Podman's API socket, for a process that runs as root on a host
/// where the socket exists: `mmd` on a remote host, where Podman's Compose
/// provider needs it anyway. Rootless Podman on a workstation keeps its
/// command line.
pub(crate) fn local_podman() -> Option<EngineApi> {
    let socket = std::path::Path::new(ROOTFUL_PODMAN_SOCKET);
    (reads_through_api() && rustix::process::geteuid().is_root() && socket.exists())
        .then(|| EngineApi::at(socket))
}

/// The API of the engine setup installed on a remote host, for what `mmd`
/// can only ask through an API (the autoscaler's look at a service's
/// containers): Docker's socket, or rootful Podman's. `MM_ENGINE_API`
/// does not apply: there is no command line to fall back to.
pub(crate) fn host_api(engine: crate::domain::runtime::RuntimeKind) -> Option<EngineApi> {
    match engine {
        crate::domain::runtime::RuntimeKind::DockerCompose => EngineApi::local_docker(),
        // Asked for when needed: `podman.socket` may open after `mmd`.
        crate::domain::runtime::RuntimeKind::PodmanCompose => {
            Some(EngineApi::at(std::path::Path::new(ROOTFUL_PODMAN_SOCKET)))
        }
    }
}
