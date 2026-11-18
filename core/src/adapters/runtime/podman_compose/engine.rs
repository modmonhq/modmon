// Reads from rootful Podman's Docker-compatible API, and what a rootful
// engine needs from the process that drives it.
//
// On a remote host Podman's Compose provider is Docker Compose, which talks
// to the engine over that socket (`/run/podman/podman.sock`, the
// `podman.socket` unit), so the socket is there wherever `mmd` drives
// Podman. A status's containers and the engine's version are read from it
// exactly as from Docker Engine's (`engine_containers::project_containers`),
// and so are the containers whose logs a read follows. What the compatible
// API does not carry, or carries less well, stays on the CLI: a network's
// lease range and whether its DNS is on (`podman network inspect`), the
// event stream the name resolver follows (`podman events`), and each
// container's log lines, which Podman 5.4's API stamps to the second only
// (`podman logs --timestamps` gives nanoseconds, by which the lines of
// several nodes are merged: `engine_logs::LineSource::Command`).

impl PodmanComposeRuntime {
    /// A rootful engine is driven by root only: rootless Podman as a user
    /// on a workstation, rootful Podman by `mmd` on a remote host.
    fn rootful_engine_problem(&self, rootless: Option<bool>) -> Option<Diagnostic> {
        (rootless == Some(false) && !(self.root)()).then(|| {
            Diagnostic::error(
                "MM_PODMAN_ROOTFUL",
                "Podman answers as root, but mm does not run as root",
            )
            .with_help(
                "run mm as your own user against rootless Podman (unset CONTAINER_HOST); rootful Podman is for `mmd` on a remote host",
            )
        })
    }

    /// What a status needs to know about the engine: whether it answers,
    /// asked with one request on its socket.
    async fn status_through_api(&self) -> RuntimeProbe {
        let Some(engine) = &self.engine else {
            return self.probe().await;
        };
        match engine.json("/version").await {
            Ok(version) => RuntimeProbe {
                kind: self.kind(),
                available: true,
                engine_version: Some(format!(
                    "podman version {}",
                    version["Version"].as_str().unwrap_or("unknown")
                )),
                compose_provider: None,
                rootless: Some(false),
                socket_path: Some(engine.socket().to_path_buf()),
                capabilities: runtime_descriptor(self.kind()).advertised_capabilities(),
                diagnostics: Vec::new(),
            },
            Err(error) => RuntimeProbe::unavailable(
                self.kind(),
                Diagnostic::error("MM_PODMAN_UNAVAILABLE", error.to_string()).with_help(
                    "check that Podman's API socket is running: `systemctl status podman.socket`",
                ),
            ),
        }
    }

    /// The project's containers from the engine's API, found by the
    /// project's label; `None` without a socket.
    async fn observe_through_api(
        &self,
        target: &RuntimeTarget,
        expected_services: &[String],
    ) -> Option<Result<ObservedProject, MmError>> {
        let engine = self.engine.as_ref()?;
        Some(
            crate::adapters::runtime::engine_containers::project_containers(
                engine,
                &target.project_name,
                None,
            )
            .await
            .map(|containers| {
                let containers = containers
                    .into_iter()
                    .map(|container| container.observation)
                    .collect::<Vec<_>>();
                ObservedProject {
                    stack: target.stack_name.clone(),
                    services: normalize_observation(target, expected_services, &containers),
                    containers,
                }
            }),
        )
    }
}
