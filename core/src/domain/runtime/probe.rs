//! A probe that stands for the engine of a remote target.

use super::{RuntimeKind, RuntimeProbe};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;

/// The code of a probe finding for an engine command that ran out of time.
pub const PROBE_TIMEOUT_CODE: &str = "MM_PROCESS_TIMEOUT";

/// The code of a probe finding for `error`: its own when the engine did not
/// answer in time, `otherwise` for every other failure.
#[must_use]
pub fn probe_failure_code(error: &MmError, otherwise: &'static str) -> &'static str {
    if matches!(error, MmError::ProcessTimeout { .. }) {
        PROBE_TIMEOUT_CODE
    } else {
        otherwise
    }
}

impl RuntimeProbe {
    /// Whether the engine maps its containers' user ids to others of the
    /// host (Docker's `userns-remap`): `None` when the probe did not read
    /// the engine's security options, which `rootless` comes from.
    #[must_use]
    pub fn remaps_user_ids(&self) -> Option<bool> {
        (self.available && self.rootless.is_some()).then(|| {
            self.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DOCKER_USERNS_REMAP)
        })
    }

    /// The code of the note that stands in for a probe when the engine
    /// runs on a remote target's hosts.
    pub const NOT_INSPECTED_HERE: &'static str = "MM_REMOTE_RUNTIME_NOT_INSPECTED";

    /// The engine of a remote target: its hosts answer for it, so this
    /// machine's engine was not asked and says nothing about it.
    #[must_use]
    pub fn on_remote_hosts(kind: RuntimeKind) -> Self {
        Self::unavailable(
            kind,
            Diagnostic::info(
                Self::NOT_INSPECTED_HERE,
                "remote runtime inspection is performed by the target; local engines were not probed",
            ),
        )
    }

    /// Whether this is what this machine's engine answered. False for a
    /// remote target, where `available` is false because nothing was asked.
    #[must_use]
    pub fn inspected_here(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == Self::NOT_INSPECTED_HERE)
    }

    /// When the engine did not answer in time, the timeout an operation
    /// that needs it fails with: what did not answer, not a capability it
    /// lacks. `owner` is whose engine it is, such as `node 'app-1'`.
    #[must_use]
    pub fn late_engine(&self, owner: Option<&str>) -> Option<MmError> {
        let late = self
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == PROBE_TIMEOUT_CODE)
            .filter(|_| !self.available)?;
        let (engine, check) = match self.kind {
            RuntimeKind::DockerCompose => ("Docker", "systemctl status docker"),
            RuntimeKind::PodmanCompose => ("Podman", "podman info"),
        };
        Some(MmError::Timeout {
            message: format!(
                "the {engine} engine{} did not answer in time, and nothing was changed: {}. Check the engine on the host (`{check}`) and run the command again",
                owner
                    .map(|owner| format!(" of {owner}"))
                    .unwrap_or_default(),
                late.message
            ),
        })
    }

    #[must_use]
    pub fn unusable_engine(&self, target: Option<&str>) -> Option<MmError> {
        if self.available {
            return None;
        }
        let found = self
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.severity == crate::domain::diagnostic::Severity::Error)?;
        let (engine, code) = match self.kind {
            RuntimeKind::DockerCompose => ("Docker", "MM_DOCKER_UNAVAILABLE"),
            RuntimeKind::PodmanCompose => ("Podman", "MM_PODMAN_UNAVAILABLE"),
        };
        Some(MmError::Remote {
            code,
            message: format!(
                "the {engine} engine{} cannot be used, and nothing was changed: {} ({}).{}",
                target
                    .map(|target| format!(" of target '{target}'"))
                    .unwrap_or_default(),
                found.message,
                found.code,
                found
                    .help
                    .as_deref()
                    .map(|help| format!(" {help}"))
                    .unwrap_or_default()
            ),
        })
    }
}

/// The note a Docker probe carries when the daemon runs with
/// `userns-remap`, whose containers do not have the host's user ids.
pub const DOCKER_USERNS_REMAP: &str = "MM_DOCKER_USERNS_REMAP";
