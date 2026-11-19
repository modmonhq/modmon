//! A remote host's container engine as its `mmd` finds it, and what
//! `mm doctor` says about it.
//!
//! On a remote host `mmd` runs the engine as root. Docker Engine brings
//! its own Compose plugin. Podman needs two things mm cannot do without:
//! version 5 or later, whose network stack (netavark) creates the bridge
//! networks that span cluster nodes, and Docker Compose as the Compose
//! provider of `podman compose`, talking to Podman's API socket. mm runs
//! Compose with `--project-directory` and reads the labels Docker Compose
//! writes; podman-compose 1.3, the one Debian 13 ships, has no such option.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::NodeReport;
use crate::domain::diagnostic::Diagnostic;
use crate::domain::runtime::{RuntimeKind, RuntimeProbe};

/// The oldest Podman a remote host may run.
pub const PODMAN_MINIMUM: (u32, u32) = (5, 0);

/// A host's engine as its `mmd` probed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineFacts {
    pub runtime: RuntimeKind,
    /// As the engine names itself: `podman version 5.4.2`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The Compose provider: `docker-compose`, `podman-compose`, or
    /// `compose-provider` for one mm does not know.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose: Option<String>,
    /// The provider's version line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose_version: Option<String>,
    /// Why the engine cannot be used, as the probe said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

impl EngineFacts {
    #[must_use]
    pub fn of(probe: &RuntimeProbe) -> Self {
        let problem = (!probe.available).then(|| {
            let said = probe
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.severity == crate::domain::diagnostic::Severity::Error
                })
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>();
            if said.is_empty() {
                "the engine does not answer".to_owned()
            } else {
                said.join("; ")
            }
        });
        Self {
            runtime: probe.kind,
            version: probe.engine_version.clone(),
            compose: probe
                .compose_provider
                .as_ref()
                .map(|provider| provider.name.clone()),
            compose_version: probe
                .compose_provider
                .as_ref()
                .and_then(|provider| provider.version.clone()),
            problem,
        }
    }

    /// The engine in a few words, for a list of nodes: `Podman 5.4`, or
    /// that it does not answer.
    #[must_use]
    pub fn summary(&self) -> String {
        let name = match self.runtime {
            RuntimeKind::DockerCompose => "Docker",
            RuntimeKind::PodmanCompose => "Podman",
        };
        match (&self.problem, self.release()) {
            (Some(_), _) => format!("{name} not answering"),
            (None, Some((major, minor))) => format!("{name} {major}.{minor}"),
            (None, None) => name.to_owned(),
        }
    }

    /// The release in `version`, as major and minor: the first word that
    /// starts with a digit.
    #[must_use]
    pub fn release(&self) -> Option<(u32, u32)> {
        let word = self
            .version
            .as_deref()?
            .split(|character: char| character.is_whitespace() || character == ',')
            .find(|word| word.starts_with(|character: char| character.is_ascii_digit()))?;
        let mut parts = word.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts
            .next()
            .and_then(|minor| {
                minor
                    .trim_end_matches(|character: char| !character.is_ascii_digit())
                    .parse()
                    .ok()
            })
            .unwrap_or(0);
        Some((major, minor))
    }
}

/// What `mm doctor` says about each answering host's engine: one that does
/// not answer, Podman older than [`PODMAN_MINIMUM`], and a Compose provider
/// other than Docker Compose behind `podman compose`.
#[must_use]
pub fn engine_diagnostics(reports: &[NodeReport]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for report in reports {
        let Some(engine) = &report.engine else {
            continue;
        };
        let node = &report.name;
        if let Some(problem) = &engine.problem {
            let help = match engine.runtime {
                RuntimeKind::PodmanCompose => {
                    "on the node, `systemctl status podman.socket` and `podman info` say why; `mm target setup plan` and its apply install and start what is missing"
                }
                RuntimeKind::DockerCompose => {
                    "on the node, `systemctl status docker` says why; `mm target setup plan` and its apply install what is missing"
                }
            };
            diagnostics.push(
                Diagnostic::error(
                    "MM_NODE_ENGINE",
                    format!(
                        "the {} engine of '{node}' cannot be used: {problem}",
                        engine.runtime.name()
                    ),
                )
                .with_help(help),
            );
            continue;
        }
        if engine.runtime != RuntimeKind::PodmanCompose {
            continue;
        }
        if let Some((major, minor)) = engine.release()
            && (major, minor) < PODMAN_MINIMUM
        {
            diagnostics.push(
                Diagnostic::error(
                    "MM_PODMAN_VERSION",
                    format!(
                        "'{node}' runs Podman {major}.{minor}; mm needs Podman {}.{} or later on a remote host, whose network stack (netavark) makes the networks that span cluster nodes",
                        PODMAN_MINIMUM.0, PODMAN_MINIMUM.1
                    ),
                )
                .with_help(
                    "install a release that ships Podman 5 (Debian 13, Ubuntu 25.04 or later) on the node, then run `mm target setup plan` and its apply",
                ),
            );
        }
        if engine.compose.as_deref() != Some("docker-compose") {
            let found = match (&engine.compose, &engine.compose_version) {
                (Some(_), Some(version)) => format!("runs {version}"),
                (Some(name), None) => format!("runs {name}"),
                (None, _) => "has no Compose provider".to_owned(),
            };
            diagnostics.push(
                Diagnostic::error(
                    "MM_PODMAN_COMPOSE",
                    format!(
                        "`podman compose` on '{node}' {found}; mm drives Podman through Docker Compose, which talks to Podman's API socket"
                    ),
                )
                .with_help(
                    "on the node, `apt-get install docker-compose` and `systemctl enable --now podman.socket`; `mm target setup plan` and its apply do both",
                ),
            );
        }
    }
    diagnostics
}
