//! Whether a host's engine can run a container under a named OCI runtime,
//! for `mm doctor`: the runtime has to be known to the engine (Docker
//! Engine's runtime list, or the runtime Podman resolves the name to) and
//! has to start a container that does nothing.

use std::time::Duration;

use crate::adapters::runtime::project_services::ContainerEngine;
use crate::domain::error::MmError;

/// The image the check runs: a few kilobytes that print a greeting and
/// exit. The engine pulls it the first time.
const IMAGE: &str = "hello-world";
const TIMEOUT: Duration = Duration::from_secs(30);
/// A first run pulls the image, and a sandboxed runtime starts slowly.
const RUN_TIMEOUT: Duration = Duration::from_secs(120);
/// How much of what the engine said a finding carries.
const SHOWN: usize = 400;

fn said(error: &MmError) -> String {
    let text = match error {
        MmError::ProcessFailed { stderr, .. } => stderr.trim().to_owned(),
        other => other.to_string(),
    };
    let line = text.lines().last().unwrap_or_default().trim();
    crate::domain::error::printed(&line.chars().take(SHOWN).collect::<String>())
}

/// Runs a trivial container under `runtime`; `Err` says why it did not
/// work and what to do.
pub(crate) async fn check_oci_runtime(
    engine: &dyn ContainerEngine,
    runtime: &str,
) -> Result<(), String> {
    let name = engine.engine_name();
    if name == "Docker" {
        docker_knows(engine, runtime).await?;
    } else {
        podman_knows(engine, runtime).await?;
    }
    let arguments = [
        "run",
        "--rm",
        "--runtime",
        runtime,
        "--network",
        "none",
        IMAGE,
    ]
    .map(str::to_owned)
    .to_vec();
    engine.engine(arguments, RUN_TIMEOUT).await.map(drop).map_err(|error| {
        format!(
            "a container did not run under '{runtime}': {}. `{} run --rm --runtime {runtime} {IMAGE}` on the host shows the same; fix the runtime or take it out of `runtimes:` in /etc/mm/host.yaml",
            said(&error),
            name.to_ascii_lowercase()
        )
    })
}

/// Docker Engine lists the runtimes registered with it.
async fn docker_knows(engine: &dyn ContainerEngine, runtime: &str) -> Result<(), String> {
    let name = engine.engine_name();
    let arguments = ["info", "--format", "{{json .Runtimes}}"]
        .map(str::to_owned)
        .to_vec();
    let listed = engine
        .engine(arguments, TIMEOUT)
        .await
        .map_err(|error| format!("{name} did not list its runtimes: {}", said(&error)))?;
    let known =
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(listed.stdout.trim())
            .map_err(|_| format!("{name} printed a runtime list mmd does not understand"))?;
    if !known.contains_key(runtime) {
        return Err(format!(
            "{name} on this host has no runtime named '{runtime}' (it has {}). Install it and register it with the engine (for gVisor: `runsc install`, then `systemctl reload docker`), or take it out of `runtimes:` in /etc/mm/host.yaml",
            known.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

/// Podman finds a runtime by name through `[engine.runtimes]` of its
/// `containers.conf` (crun, runc, runsc, kata, ... at their usual paths)
/// and says which binary that is, or that it has none: `podman --runtime
/// NAME info` reads exactly that, without a container.
async fn podman_knows(engine: &dyn ContainerEngine, runtime: &str) -> Result<(), String> {
    let name = engine.engine_name();
    let arguments = [
        "--runtime",
        runtime,
        "info",
        "--format",
        "{{json .Host.OCIRuntime}}",
    ]
    .map(str::to_owned)
    .to_vec();
    let found = engine.engine(arguments, TIMEOUT).await.map_err(|error| {
        format!(
            "{name} on this host has no runtime named '{runtime}': {}. Install it where {name} looks for it (`[engine.runtimes]` in /etc/containers/containers.conf names the paths), or take it out of `runtimes:` in /etc/mm/host.yaml",
            said(&error)
        )
    })?;
    let understood = match serde_json::from_str::<serde_json::Value>(found.stdout.trim()) {
        Ok(value) => value.get("path").is_some_and(serde_json::Value::is_string),
        Err(_) => false,
    };
    if !understood {
        return Err(format!(
            "{name} printed a runtime mmd does not understand for '{runtime}'"
        ));
    }
    Ok(())
}
