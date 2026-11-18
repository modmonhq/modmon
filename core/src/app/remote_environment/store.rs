//! The managed secret store of a remote environment: a snapshot from one of
//! its members, and a snapshot put back on every node.

use super::{RemoteEnvironment, in_node};
use crate::app::ports::{ClusterStoreKeeper, StoreScope};
use crate::domain::error::MmError;
use crate::domain::secret_store::{StoreMember, StoreRestoreStep, StoreRestored, StoreSnapshot};

fn serialized<T: serde::Serialize>(value: &T) -> Result<zeroize::Zeroizing<Vec<u8>>, MmError> {
    serde_json::to_vec(value)
        .map(zeroize::Zeroizing::new)
        .map_err(|error| MmError::State {
            message: format!("serializing the snapshot: {error}"),
        })
}

impl RemoteEnvironment {
    /// A snapshot sealed to `recipient` from the first node whose member
    /// answers, with that node's name.
    pub async fn store_snapshot(&self, recipient: &str) -> Result<(String, String), MmError> {
        let mut first_error = None;
        for node in &self.nodes {
            let answer = match node.connect() {
                Ok(transport) => {
                    transport
                        .store_snapshot(node.scope.clone(), recipient.to_owned())
                        .await
                }
                Err(error) => Err(error),
            };
            match answer {
                Ok(sealed) => return Ok((self.name_of(node).to_owned(), sealed)),
                Err(error) => {
                    first_error.get_or_insert(in_node(node.name.as_deref(), error));
                }
            }
        }
        Err(first_error.unwrap_or_else(|| MmError::Target {
            message: format!("target '{}' lists no node", self.target),
        }))
    }

    /// Puts `snapshot` back on the first of `members`, then its keys on every node.
    pub async fn store_restore(
        &self,
        snapshot: &StoreSnapshot,
        members: &[StoreMember],
        keeper: &dyn ClusterStoreKeeper,
    ) -> Result<Vec<StoreRestored>, MmError> {
        let restored = snapshot;
        let first = members
            .first()
            .and_then(|member| {
                self.nodes
                    .iter()
                    .find(|node| node.name.as_deref() == Some(member.name.as_str()))
            })
            .ok_or_else(|| MmError::Target {
                message: "the store has no member to put the snapshot back on".to_owned(),
            })?;
        let mut answers = Vec::with_capacity(self.nodes.len() + 1);
        let recipient = self.recipient_of(first).await?;
        let sealed = keeper.seal(&recipient, &serialized(restored)?)?;
        answers.push(
            self.restore_step(first, StoreRestoreStep::Snapshot { sealed })
                .await?,
        );
        let keys = serialized(&restored.recovery)?;
        for node in &self.nodes {
            let recipient = self.recipient_of(node).await?;
            let sealed = keeper.seal(&recipient, &keys)?;
            answers.push(
                self.restore_step(node, StoreRestoreStep::Keys { sealed })
                    .await?,
            );
        }
        let scope = StoreScope {
            project: snapshot.project_id,
            environment: &snapshot.environment,
            provider: &snapshot.provider,
        };
        let operator = keeper.operator_recipient(snapshot.project_id)?;
        keeper.keep_recovery(scope, &keeper.seal(&operator, &keys)?)?;
        Ok(answers)
    }

    async fn recipient_of(&self, node: &super::RemoteNode) -> Result<String, MmError> {
        node.connect()?
            .secret_recipient(node.scope.clone())
            .await
            .map_err(|error| in_node(node.name.as_deref(), error))
    }

    async fn restore_step(
        &self,
        node: &super::RemoteNode,
        step: StoreRestoreStep,
    ) -> Result<StoreRestored, MmError> {
        node.connect()?
            .store_restore(node.scope.clone(), step)
            .await
            .map_err(|error| in_node(node.name.as_deref(), error))
    }
}

impl RemoteEnvironment {
    /// Moves the age store's secrets into `provider` through the first node that answers.
    pub async fn store_migrate(
        &self,
        provider: &str,
    ) -> Result<crate::domain::secret_store::StoreMigrated, MmError> {
        let mut first_error = None;
        for node in &self.nodes {
            let answer = match node.connect() {
                Ok(transport) => {
                    transport
                        .store_migrate(node.scope.clone(), provider.to_owned())
                        .await
                }
                Err(error) => Err(error),
            };
            match answer {
                Ok(moved) => return Ok(moved),
                Err(error) => {
                    first_error.get_or_insert(in_node(node.name.as_deref(), error));
                }
            }
        }
        Err(first_error.unwrap_or_else(|| MmError::Target {
            message: format!("target '{}' lists no node", self.target),
        }))
    }
}
