//! Which release of `mm` and `mmd` takes part: the environment's pin
//! (`deploy.mm-version`), checked before every deployment plan, and what
//! `mm doctor` says when this machine and the hosts run different
//! releases.

use futures_util::future::join_all;

use super::{RemoteEnvironment, RemoteNode};
use crate::domain::diagnostic::Diagnostic;
use crate::domain::error::MmError;
use crate::domain::remote::{NodeReport, NodeState};
use crate::domain::version_pin::{VersionPin, release_diagnostics};

/// The release of this `mm`.
const THIS_RELEASE: &str = env!("CARGO_PKG_VERSION");

impl RemoteEnvironment {
    /// The environment's `deploy.mm-version`, from the configuration this
    /// client read.
    #[must_use]
    pub fn with_version_pin(mut self, pin: Option<VersionPin>) -> Self {
        self.version_pin = pin;
        self
    }

    #[must_use]
    pub fn version_pin(&self) -> Option<&VersionPin> {
        self.version_pin.as_ref()
    }

    /// Refuses to plan with an `mm` the pin leaves out. Nothing is asked of
    /// the hosts.
    pub(super) fn check_pinned_client(&self) -> Result<(), MmError> {
        match &self.version_pin {
            Some(pin) => pin
                .check_client(THIS_RELEASE)
                .map_err(|message| MmError::Environment { message }),
            None => Ok(()),
        }
    }

    /// Refuses to plan for a host whose `mmd` the pin leaves out. The hosts
    /// are asked only when a pin is set. A host that does not answer is
    /// left to the plan, which says so in its own words.
    pub(super) async fn check_pinned_hosts(&self, selected: &[&RemoteNode]) -> Result<(), MmError> {
        let Some(pin) = &self.version_pin else {
            return Ok(());
        };
        let reports = join_all(selected.iter().map(|node| self.node_report(node))).await;
        let problems = reports
            .iter()
            .filter(|report| report.state == NodeState::Ready)
            .filter_map(|report| {
                pin.check_host(&self.host_label(report), report.mmd_version.as_deref())
                    .err()
            })
            .collect::<Vec<_>>();
        if problems.is_empty() {
            Ok(())
        } else {
            Err(MmError::Environment {
                message: problems.join("; "),
            })
        }
    }

    /// `node 'app-2'`, or `the host of target 'production'`.
    fn host_label(&self, report: &NodeReport) -> String {
        if self.is_cluster() {
            format!("node '{}'", report.name)
        } else {
            format!("the host of target '{}'", self.target)
        }
    }

    /// What `mm doctor` reports about releases, from the hosts that
    /// answered (see `domain::version_pin::release_diagnostics`).
    #[must_use]
    pub fn version_diagnostics(&self, reports: &[NodeReport]) -> Vec<Diagnostic> {
        let hosts = reports
            .iter()
            .filter(|report| report.state == NodeState::Ready)
            .map(|report| (self.host_label(report), report.mmd_version.clone()))
            .collect::<Vec<_>>();
        release_diagnostics(self.version_pin.as_ref(), THIS_RELEASE, &hosts)
    }
}
