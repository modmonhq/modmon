//! mmd unseals the members of managed stores on its node: at start, and
//! whenever a member restarted sealed, with no operator step.

use std::sync::Arc;
use std::time::Duration;

use super::{StoreLayout, member_connection, seal_status, unseal};
use crate::adapters::secrets::{OpenBaoConnectionStore, OpenBaoHttpTransport};
use crate::domain::error::MmError;
use crate::domain::secret_store::STORE_API_PORT;

const EVERY: Duration = Duration::from_secs(10);
/// One look at every member, the requests included.
const ROUND: Duration = Duration::from_secs(60);

pub struct StoreUnsealer {
    layout: StoreLayout,
    connections: Arc<OpenBaoConnectionStore>,
    transport: Arc<dyn OpenBaoHttpTransport>,
    /// Reloads a member whose audit log was rotated.
    process: Option<Arc<dyn crate::app::ports::ProcessRunner>>,
}

impl std::fmt::Debug for StoreUnsealer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoreUnsealer")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

impl StoreUnsealer {
    #[must_use]
    pub fn new(
        layout: StoreLayout,
        connections: Arc<OpenBaoConnectionStore>,
        transport: Arc<dyn OpenBaoHttpTransport>,
    ) -> Self {
        Self {
            layout,
            connections,
            transport,
            process: None,
        }
    }

    /// Also rotates each member's audit log, as mmd rotates its own.
    #[must_use]
    pub fn rotating(mut self, process: Arc<dyn crate::app::ports::ProcessRunner>) -> Self {
        self.process = Some(process);
        self
    }

    async fn rotate(&self, id: &str) -> Result<(), MmError> {
        let Some(process) = &self.process else {
            return Ok(());
        };
        if !super::audit::rotate(&self.layout.audit_log(id))? {
            return Ok(());
        }
        let unit = crate::domain::secret_store::store_unit(id);
        let output = process
            .run(
                crate::app::ports::CommandSpec::new("systemctl")
                    .args(["reload", unit.as_str()])
                    .timeout(Duration::from_secs(30)),
            )
            .await?;
        if !output.success() {
            tracing::warn!(%unit, "the secret store's member did not reopen its rotated audit log");
        }
        Ok(())
    }

    /// Looks at every member of this node every ten seconds, for as long
    /// as the daemon runs.
    pub async fn run(self) {
        loop {
            match tokio::time::timeout(ROUND, self.round()).await {
                Ok(Ok(unsealed)) => {
                    for id in unsealed {
                        tracing::info!(store = %id, "unsealed the secret store's member on this node");
                    }
                }
                Ok(Err(error)) => {
                    tracing::warn!(%error, "the secret store's members could not be looked at");
                }
                Err(_) => tracing::warn!("looking at the secret store's members took too long"),
            }
            tokio::time::sleep(EVERY).await;
        }
    }

    /// Unseals each sealed member this node holds the keys of and that
    /// joined its store; returns the stores it unsealed.
    pub async fn round(&self) -> Result<Vec<String>, MmError> {
        let mut unsealed = Vec::new();
        for (id, record) in self.layout.members()? {
            self.rotate(&id).await?;
            if !record.initialized {
                continue;
            }
            let Some(recovery) = self.connections.cluster_store(&id)? else {
                continue;
            };
            let connection = member_connection(
                record.project_id,
                &id,
                format!("https://127.0.0.1:{STORE_API_PORT}"),
                recovery.ca_pem.clone(),
            );
            let Ok(status) = seal_status(self.transport.as_ref(), &connection).await else {
                continue;
            };
            if status.initialized
                && status.sealed
                && unseal(self.transport.as_ref(), &connection, &recovery.unseal_keys).await?
            {
                unsealed.push(id);
            }
        }
        Ok(unsealed)
    }
}
