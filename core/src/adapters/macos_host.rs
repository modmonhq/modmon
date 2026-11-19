//! Resolves the facts a macOS ingress artifact pins for launchd: launchd jobs get
//! no login shell, so executables and the Docker endpoint must be explicit.
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::domain::target_ingress::{IngressHostProfile, MacosHostFacts};

const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

/// Probes this Mac once. Failures become `Unavailable` so commands that never
/// plan local ingress keep working; planning the local target reports the reason.
#[must_use]
pub fn probe_local_ingress_host() -> IngressHostProfile {
    match resolve() {
        Ok(host) => IngressHostProfile::MacosLaunchd(host),
        Err(reason) => IngressHostProfile::Unavailable { reason },
    }
}

fn resolve() -> Result<MacosHostFacts, String> {
    let home = directories::BaseDirs::new()
        .map(|directories| directories.home_dir().to_path_buf())
        .ok_or_else(|| "could not resolve the current user's home directory".to_owned())?;
    let home = utf8_absolute(&home, "home directory")?;
    let haproxy = homebrew_haproxy().ok_or_else(|| {
        "HAProxy is not installed; run `brew install haproxy`, then retry".to_owned()
    })?;
    let docker = search_path("docker").ok_or_else(|| {
        "the Docker CLI is not on PATH; install and start Docker Desktop, then retry".to_owned()
    })?;
    let docker_executable = utf8_absolute(&docker, "Docker CLI path")?;
    let docker_host = active_docker_endpoint(&docker)?;
    require_docker_desktop(&docker_host, &home)?;
    Ok(MacosHostFacts {
        haproxy_executable: utf8_absolute(&haproxy, "HAProxy path")?,
        docker_executable,
        docker_host,
        home,
    })
}

/// The Homebrew CLI on PATH, else in a standard prefix.
#[must_use]
pub fn homebrew_executable() -> Option<PathBuf> {
    search_path("brew").or_else(|| {
        HOMEBREW_PREFIXES
            .iter()
            .map(|prefix| Path::new(prefix).join("bin/brew"))
            .find(|path| path.is_file())
    })
}

/// Homebrew's HAProxy, if installed.
#[must_use]
pub fn homebrew_haproxy() -> Option<PathBuf> {
    haproxy_executable(&homebrew_prefixes())
}

/// `brew --prefix` covers custom prefixes; the two standard prefixes are the fallback.
fn homebrew_prefixes() -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    if let Some(brew) = homebrew_executable()
        && let Ok(output) = Command::new(brew)
            .arg("--prefix")
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .envs(std::env::var_os("HOME").map(|home| ("HOME", home)))
            .output()
        && output.status.success()
    {
        let prefix = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if prefix.starts_with('/') {
            prefixes.push(PathBuf::from(prefix));
        }
    }
    for prefix in HOMEBREW_PREFIXES {
        let prefix = PathBuf::from(prefix);
        if !prefixes.contains(&prefix) {
            prefixes.push(prefix);
        }
    }
    prefixes
}

/// Homebrew's `opt` link survives `brew upgrade`, unlike the versioned Cellar path.
pub(crate) fn haproxy_executable(prefixes: &[PathBuf]) -> Option<PathBuf> {
    prefixes
        .iter()
        .map(|prefix| prefix.join("opt/haproxy/bin/haproxy"))
        .find(|path| path.is_file())
}

/// Keeps the PATH entry rather than its canonical target, which for Homebrew is
/// a versioned Cellar path that an upgrade removes.
fn search_path(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn active_docker_endpoint(docker: &Path) -> Result<String, String> {
    if let Some(host) = std::env::var_os("DOCKER_HOST").filter(|host| !host.is_empty()) {
        return host
            .into_string()
            .map_err(|_| "DOCKER_HOST is not UTF-8".to_owned());
    }
    let mut command = Command::new(docker);
    command
        .args([
            "context",
            "inspect",
            "--format",
            "{{.Endpoints.docker.Host}}",
        ])
        .env_clear();
    for name in ["HOME", "PATH", "DOCKER_CONFIG", "DOCKER_CONTEXT"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let output = command
        .output()
        .map_err(|error| format!("could not inspect the active Docker context: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "could not inspect the active Docker context: {}",
            crate::domain::error::printed(String::from_utf8_lossy(&output.stderr).trim())
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The networking profile (gateway peer, host-gateway reachability) was verified
/// on Docker Desktop only; other macOS engines forward ports differently.
pub(crate) fn require_docker_desktop(endpoint: &str, home: &str) -> Result<(), String> {
    let desktop = format!("{home}/.docker/run/docker.sock");
    let socket = endpoint.strip_prefix("unix://").unwrap_or_default();
    let resolves_to_desktop = socket == desktop
        || std::fs::canonicalize(socket)
            .ok()
            .zip(std::fs::canonicalize(&desktop).ok())
            .is_some_and(|(socket, desktop)| socket == desktop);
    if resolves_to_desktop {
        Ok(())
    } else {
        Err(format!(
            "local HTTPS ingress on macOS requires Docker Desktop, but the active Docker endpoint is {endpoint}; run `docker context use desktop-linux` (or unset DOCKER_HOST), then retry"
        ))
    }
}

fn utf8_absolute(path: &Path, kind: &str) -> Result<String, String> {
    if !path.is_absolute() {
        return Err(format!("{kind} must be absolute: {}", path.display()));
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{kind} must be UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_desktop_endpoint_is_required() {
        assert!(
            require_docker_desktop("unix:///srv/dev/.docker/run/docker.sock", "/srv/dev").is_ok()
        );
        let colima =
            require_docker_desktop("unix:///srv/dev/.colima/default/docker.sock", "/srv/dev");
        assert!(colima.is_err_and(|reason| reason.contains("docker context use desktop-linux")));
        assert!(require_docker_desktop("tcp://127.0.0.1:2375", "/srv/dev").is_err());
    }

    #[test]
    fn haproxy_resolves_through_the_upgrade_stable_opt_link()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = crate::test_support::tempdir()?;
        let custom = temp.path().join("custom brew");
        assert_eq!(haproxy_executable(std::slice::from_ref(&custom)), None);
        let link = custom.join("opt/haproxy/bin");
        std::fs::create_dir_all(&link)?;
        std::fs::write(link.join("haproxy"), "")?;
        assert_eq!(
            haproxy_executable(&[temp.path().join("missing"), custom.clone()]),
            Some(link.join("haproxy"))
        );
        Ok(())
    }
}
