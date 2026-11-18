//! Snapshots of the store's Raft data, and putting one back on fresh
//! members.

use base64::Engine as _;
use reqwest::Method;
use zeroize::Zeroizing;

use super::{NodeStore, failed, login};
use crate::domain::error::MmError;
use crate::domain::secret_store::{
    MAX_STORE_SNAPSHOT_BYTES, StoreRestoreStep, StoreRestored, StoreSnapshot,
};

const MAX_SEALED_SNAPSHOT_BYTES: usize = MAX_STORE_SNAPSHOT_BYTES * 2;

impl NodeStore {
    pub(super) fn held(&self) -> Result<crate::domain::secret_store::StoreRecovery, MmError> {
        self.connections
            .cluster_store(&self.id())?
            .ok_or_else(|| MmError::Target {
                message: format!(
                    "this node holds no keys of the secret store of '{}'; `mm target setup apply` gives them",
                    self.environment
                ),
            })
    }

    pub(super) fn member_ca(&self) -> Result<String, MmError> {
        super::member::read_text(&self.layout.ca(&self.id()))?.ok_or_else(|| MmError::Target {
            message: "this node runs no member of the secret store".to_owned(),
        })
    }

    /// A snapshot of the store's Raft data with the recovery document it
    /// opens with, sealed to `recipient`.
    pub async fn snapshot(&self, recipient: &str, now_unix_ms: i64) -> Result<String, MmError> {
        let recovery = self.held()?;
        let record = self
            .layout
            .record(&self.id())?
            .ok_or_else(|| MmError::Target {
                message: "this node runs no member of the secret store".to_owned(),
            })?;
        let connection = self.loopback(self.member_ca()?);
        let token = login(self.transport.as_ref(), &connection, &recovery.operations).await?;
        let response = self
            .transport
            .request(
                &connection,
                Method::GET,
                "sys/storage/raft/snapshot",
                Some(token.as_str()),
                None,
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(failed(&response, "a snapshot"));
        }
        if response.body.len() > MAX_STORE_SNAPSHOT_BYTES {
            return Err(MmError::Target {
                message: format!(
                    "the store's snapshot is larger than the {} MiB mm moves",
                    MAX_STORE_SNAPSHOT_BYTES / (1024 * 1024)
                ),
            });
        }
        let snapshot = StoreSnapshot {
            project_id: self.project,
            environment: self.environment.clone(),
            provider: recovery.provider.clone(),
            taken_at_unix_ms: now_unix_ms,
            node: record.node,
            raft: Zeroizing::new(
                base64::engine::general_purpose::STANDARD.encode(response.body.as_slice()),
            ),
            recovery,
        };
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(&snapshot).map_err(|error| MmError::State {
                    message: format!("serializing the snapshot: {error}"),
                })?,
            );
        crate::adapters::secrets::seal_to_recipient(recipient, &plaintext)
    }

    /// One step of putting a snapshot back: the Raft data on this member, or
    /// the keys it opens with on this node.
    pub async fn restore(&self, step: StoreRestoreStep) -> Result<StoreRestored, MmError> {
        let node = self
            .layout
            .record(&self.id())?
            .map(|record| record.node)
            .unwrap_or_default();
        match step {
            StoreRestoreStep::Snapshot { sealed } => {
                let opened = self
                    .host_store
                    .open_sealed(sealed.as_bytes(), MAX_SEALED_SNAPSHOT_BYTES)?;
                let snapshot: StoreSnapshot =
                    serde_json::from_slice(&opened).map_err(|_| MmError::Secret {
                        message: "the snapshot is not valid".to_owned(),
                    })?;
                if snapshot.project_id != self.project || snapshot.environment != self.environment {
                    return Err(MmError::Security {
                        message: format!(
                            "the snapshot is of another project or environment than {}",
                            self.environment
                        ),
                    });
                }
                let raft = Zeroizing::new(
                    base64::engine::general_purpose::STANDARD
                        .decode(snapshot.raft.as_bytes())
                        .map_err(|_| MmError::Secret {
                            message: "the snapshot's Raft data is not valid".to_owned(),
                        })?,
                );
                let connection = self.loopback(self.member_ca()?);
                let token = login(
                    self.transport.as_ref(),
                    &connection,
                    &self.held()?.operations,
                )
                .await?;
                let response = self
                    .transport
                    .request_bytes(
                        &connection,
                        Method::POST,
                        "sys/storage/raft/snapshot-force",
                        &token,
                        Some(raft.as_slice()),
                    )
                    .await?;
                if !(200..300).contains(&response.status) {
                    return Err(failed(&response, "the snapshot"));
                }
                Ok(StoreRestored { node, sealed: true })
            }
            StoreRestoreStep::Keys { sealed } => {
                let joined = self.join(sealed.as_bytes()).await?;
                Ok(StoreRestored {
                    node,
                    sealed: joined.sealed,
                })
            }
        }
    }
}
