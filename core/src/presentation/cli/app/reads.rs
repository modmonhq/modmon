//! What one session of the terminal UI reads its environment through.
//!
//! The gateway is built once and kept for the session: status, logs, and
//! commands all go through it, a remote environment keeps each node's last
//! answer between refreshes (so a refresh shows each node as it answers),
//! and every request rides on the same kept connections.

use super::*;

pub(super) struct Reads {
    invocation: super::super::dispatch::Invocation,
    gateway: std::sync::Mutex<Option<Arc<crate::app::ServiceGateway>>>,
    /// This checkout's root and configuration file. A remote host's status
    /// names its own copy of the project; the views edit this one.
    checkout: (std::path::PathBuf, std::path::PathBuf),
}

impl Reads {
    pub(super) fn new(
        invocation: &super::super::dispatch::Invocation,
        document: &crate::domain::config::ConfigDocument,
    ) -> Arc<Self> {
        Arc::new(Self {
            invocation: invocation.clone(),
            gateway: std::sync::Mutex::new(None),
            checkout: (document.root.clone(), document.path.clone()),
        })
    }

    /// The environment's remote hosts; `None` when it runs on this machine.
    pub(super) fn remote_environment(
        &self,
    ) -> Result<Option<crate::app::RemoteEnvironment>, MmError> {
        let invocation = &self.invocation;
        composition::remote_environment(
            &invocation.selection,
            invocation.home(),
            invocation.runtime_kind,
        )
    }

    /// The session's gateway, built on first use. One that cannot be built
    /// is tried again next time: the target may be registered meanwhile.
    pub(super) fn gateway(&self) -> Result<Arc<crate::app::ServiceGateway>, MmError> {
        let mut kept = match self.gateway.lock() {
            Ok(kept) => kept,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(gateway) = kept.as_ref() {
            return Ok(Arc::clone(gateway));
        }
        let invocation = &self.invocation;
        let gateway = Arc::new(composition::service_gateway(
            &invocation.selection,
            invocation.home(),
            invocation.runtime_kind,
            &invocation.services,
        )?);
        *kept = Some(Arc::clone(&gateway));
        Ok(gateway)
    }

    /// Whether the environment runs on other hosts.
    pub(super) fn remote(&self) -> bool {
        self.gateway()
            .is_ok_and(|gateway| gateway.remote_target().is_some())
    }

    /// The status, telling `so_far` what is known each time a node answers.
    pub(super) async fn status(
        &self,
        so_far: Option<crate::app::StatusSoFar<'_>>,
    ) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
        let gateway = self.gateway()?;
        let here = |snapshot| self.here(snapshot);
        let reported = so_far.map(|so_far| {
            move |snapshot: crate::domain::workspace::WorkspaceSnapshot| so_far(here(snapshot))
        });
        let snapshot = gateway
            .status_reporting(
                &approval("tui", false),
                reported
                    .as_ref()
                    .map(|report| report as crate::app::StatusSoFar<'_>),
            )
            .await?;
        Ok(self.here(snapshot))
    }

    /// A remote status with this checkout as its workspace, so editing,
    /// secrets, and the Overview's root name the files on this machine.
    fn here(
        &self,
        mut snapshot: crate::domain::workspace::WorkspaceSnapshot,
    ) -> crate::domain::workspace::WorkspaceSnapshot {
        let (root, path) = &self.checkout;
        snapshot.workspace.root.clone_from(root);
        snapshot.workspace.config_path.clone_from(path);
        snapshot
    }
}
