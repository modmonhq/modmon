use super::*;
use crate::app::ports::CommandOutput;
use crate::domain::build::LocalImageEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Builder {
    Buildx,
    Engine(LocalImageEngine),
}

/// What the failed build asked the builder for.
pub(super) struct Asked<'a> {
    pub(super) platforms: &'a [String],
    pub(super) publish: bool,
}

const MISSING_BUILDX: [&str; 3] = [
    "buildx component is missing",
    "'buildx' is not a docker command",
    "unknown command: docker buildx",
];

const DOCKER_UNREACHABLE: [&str; 3] = [
    "cannot connect to the docker daemon",
    "is the docker daemon running",
    "error during connect",
];

const PODMAN_UNREACHABLE: [&str; 2] = ["cannot connect to podman", "unable to connect to podman"];

fn own(message: impl Into<String>) -> MmError {
    MmError::Build {
        message: message.into(),
    }
    .in_own_words()
}

/// A builder that could not start because it is not installed.
pub(super) fn not_started(error: MmError, builder: Builder) -> MmError {
    match &error {
        MmError::ProcessSpawn { source, .. } if source.kind() == std::io::ErrorKind::NotFound => {
            own(match builder {
                Builder::Buildx | Builder::Engine(LocalImageEngine::Docker) => {
                    "Docker is not installed, or `docker` is not on PATH; install Docker, then check it with `docker version`"
                }
                Builder::Engine(LocalImageEngine::Podman) => {
                    "Podman is not installed, or `podman` is not on PATH; install Podman, then check it with `podman version`"
                }
            })
        }
        _ => error,
    }
}

/// Why a build could not run, in mm's words when the builder's answer is
/// one mm knows; otherwise the builder's own error.
pub(super) async fn explain(
    process: &dyn ProcessRunner,
    builder: Builder,
    output: &CommandOutput,
    asked: Option<Asked<'_>>,
) -> MmError {
    let stderr = output.stderr.to_ascii_lowercase();
    let docker = builder != Builder::Engine(LocalImageEngine::Podman);
    if docker
        && DOCKER_UNREACHABLE
            .iter()
            .any(|known| stderr.contains(known))
    {
        return own(
            "Docker is not running, or this user cannot reach it; start Docker, then check it with `docker info`",
        );
    }
    if !docker
        && PODMAN_UNREACHABLE
            .iter()
            .any(|known| stderr.contains(known))
    {
        return own(
            "Podman cannot be reached; start it (on macOS and Windows with `podman machine start`), then check it with `podman info`",
        );
    }
    if docker && MISSING_BUILDX.iter().any(|known| stderr.contains(known)) {
        return own(
            "Docker Buildx is not installed; install the docker-buildx-plugin package (Docker Desktop includes it), then check it with `docker buildx version`",
        );
    }
    if docker && let Some(missing) = missing_builder(&output.stderr) {
        let missing = missing.map_or_else(
            || "the buildx builder Docker has selected".to_owned(),
            |name| format!("the buildx builder '{name}' that Docker has selected"),
        );
        return own(match builder {
            Builder::Buildx => format!(
                "{missing} no longer exists; select another with `docker buildx use NAME` (`docker buildx ls` lists them) or create one with `docker buildx create --use`, then run the build again"
            ),
            Builder::Engine(_) => format!(
                "{missing} no longer exists; select the engine's own builder with `docker buildx use default` (`docker buildx ls` lists the others), then run the build again"
            ),
        });
    }
    if docker && stderr.contains("not supported for the docker driver") {
        let builder = match builder_name(process).await {
            Some(name) => format!("the buildx builder '{name}'"),
            None => "the current buildx builder".to_owned(),
        };
        return own(format!(
            "{builder} uses the docker driver, which can't {}; create a builder that can with `docker buildx create --use`, then run the build again",
            lacking(&stderr, asked.as_ref())
        ));
    }
    MmError::ProcessFailed {
        program: match builder {
            Builder::Engine(LocalImageEngine::Podman) => "podman",
            Builder::Buildx | Builder::Engine(LocalImageEngine::Docker) => "docker",
        }
        .to_owned(),
        code: output.code,
        stderr: output.stderr.trim().to_owned(),
    }
}

fn lacking(stderr: &str, asked: Option<&Asked<'_>>) -> String {
    let several = || {
        asked
            .map(|asked| asked.platforms.join(", "))
            .filter(|platforms| platforms.contains(','))
            .map_or_else(
                || "build several platforms at once".to_owned(),
                |platforms| format!("build several platforms at once ({platforms})"),
            )
    };
    if stderr.contains("multi-platform") {
        several()
    } else if stderr.contains("attestation") {
        "attach the provenance and SBOM attestations a published build carries".to_owned()
    } else if stderr.contains("oci exporter") {
        "write the OCI image archive a build without --publish makes".to_owned()
    } else if stderr.contains("cache export") {
        "export a build cache".to_owned()
    } else if asked.is_some_and(|asked| asked.platforms.len() > 1) {
        several()
    } else if asked.is_some_and(|asked| asked.publish) {
        "attach the provenance and SBOM attestations a published build carries".to_owned()
    } else {
        "run this build".to_owned()
    }
}

/// `Some` when Buildx answered that the builder it was to use does not
/// exist, with the builder's name when it said it.
fn missing_builder(stderr: &str) -> Option<Option<String>> {
    let lower = stderr.to_ascii_lowercase();
    if let Some(start) = lower.find("no builder ") {
        let rest = &stderr[start + "no builder ".len()..];
        let found = rest.lines().next().unwrap_or_default();
        if found.trim_end().to_ascii_lowercase().ends_with(" found") {
            let name = found.split_whitespace().next().unwrap_or_default();
            let name = name.trim_matches(['"', '\'']);
            return Some(plain_name(name).then(|| name.to_owned()));
        }
    }
    let start = lower.find("buildx/instances/")? + "buildx/instances/".len();
    let (name, after) = stderr[start..].split_once(':')?;
    after
        .to_ascii_lowercase()
        .contains("no such file or directory")
        .then(|| plain_name(name).then(|| name.to_owned()))
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
}

async fn builder_name(process: &dyn ProcessRunner) -> Option<String> {
    let output = process
        .run(
            docker_command("docker")
                .args(["buildx", "inspect"])
                .timeout(Duration::from_secs(30)),
        )
        .await
        .ok()?;
    if !output.success() {
        return None;
    }
    output
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("Name:"))
        .map(str::trim)
        .filter(|name| plain_name(name))
        .map(str::to_owned)
}
