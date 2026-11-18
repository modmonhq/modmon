//! The store's first initialisation, on its first member.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::Deserialize;
use zeroize::Zeroizing;

use super::{NodeStore, failed, malformed};
use crate::adapters::secrets::{ManagedRecoveryMaterial, OpenBaoConnection};
use crate::domain::error::MmError;
use crate::domain::secret_store::{
    STORE_AUTH_MOUNT, STORE_MOUNT, StoreCredential, StoreInitAnswer, StoreInitRequest,
    StoreRecovery,
};
use crate::domain::workspace::ProjectId;

const DEPLOY: &str = "mm-deploy";
const OPERATIONS: &str = "mm-operations";

/// What mmd may do with the environment's values: read and write versions
/// under the environment's path, and nothing else.
#[must_use]
pub fn deploy_policy(project: ProjectId, environment: &str) -> String {
    let prefix = format!("mm/{}/{environment}", project.0);
    format!(
        "path \"{STORE_MOUNT}/data/{prefix}/*\" {{ capabilities = [\"create\", \"read\", \"update\", \"delete\"] }}\npath \"{STORE_MOUNT}/metadata/{prefix}/*\" {{ capabilities = [\"read\", \"list\", \"delete\"] }}\npath \"{STORE_MOUNT}/delete/{prefix}/*\" {{ capabilities = [\"update\"] }}\n"
    )
}

/// What mmd may ask of the store itself: its members, and snapshots.
#[must_use]
pub fn operations_policy() -> String {
    [
        ("sys/storage/raft/configuration", "\"read\""),
        ("sys/storage/raft/autopilot/state", "\"read\""),
        ("sys/storage/raft/snapshot", "\"read\""),
        ("sys/storage/raft/remove-peer", "\"update\""),
        ("sys/storage/raft/snapshot-force", "\"update\", \"sudo\""),
    ]
    .iter()
    .map(|(path, capabilities)| format!("path \"{path}\" {{ capabilities = [{capabilities}] }}\n"))
    .collect()
}

#[derive(Deserialize)]
struct Initialized {
    keys_base64: Vec<Zeroizing<String>>,
    root_token: Zeroizing<String>,
}

impl NodeStore {
    async fn call(
        &self,
        connection: &OpenBaoConnection,
        (method, path): (Method, &str),
        token: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, MmError> {
        let response = self
            .transport
            .request(connection, method, path, Some(token), body)
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(failed(&response, path));
        }
        if response.body.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&response.body).map_err(|_| malformed(path))
    }

    /// Whether a listing (`sys/mounts`, `sys/auth`) has `key`.
    async fn listed(
        &self,
        connection: &OpenBaoConnection,
        path: &str,
        token: &str,
        key: &str,
    ) -> Result<bool, MmError> {
        let listing = self
            .call(connection, (Method::GET, path), token, None)
            .await?;
        Ok(listing.get("data").and_then(|data| data.get(key)).is_some()
            || listing.get(key).is_some())
    }

    async fn role(
        &self,
        connection: &OpenBaoConnection,
        token: &str,
        name: &str,
    ) -> Result<StoreCredential, MmError> {
        let role = format!("auth/{STORE_AUTH_MOUNT}/role/{name}");
        let body = serde_json::json!({
            "token_policies": [name],
            "token_ttl": "15m",
            "token_max_ttl": "30m",
            "token_type": "service",
            "secret_id_num_uses": 0,
            "secret_id_ttl": "0",
        });
        self.call(connection, (Method::POST, &role), token, Some(&body))
            .await?;
        let role_id = self
            .call(
                connection,
                (Method::GET, &format!("{role}/role-id")),
                token,
                None,
            )
            .await?;
        let secret_id = self
            .call(
                connection,
                (Method::POST, &format!("{role}/secret-id")),
                token,
                Some(&serde_json::json!({})),
            )
            .await?;
        let text = |value: &serde_json::Value, key: &str| {
            value
                .pointer(&format!("/data/{key}"))
                .and_then(serde_json::Value::as_str)
                .map(|text| Zeroizing::new(text.to_owned()))
                .ok_or_else(|| malformed("an AppRole"))
        };
        Ok(StoreCredential {
            role_id: text(&role_id, "role_id")?,
            secret_id: text(&secret_id, "secret_id")?,
        })
    }

    /// Mounts, policies and logins; each step is safe to run again.
    async fn bootstrap(
        &self,
        connection: &OpenBaoConnection,
        root: &str,
    ) -> Result<(StoreCredential, StoreCredential), MmError> {
        if !self
            .listed(connection, "sys/mounts", root, &format!("{STORE_MOUNT}/"))
            .await?
        {
            let body = serde_json::json!({"type": "kv", "options": {"version": "2"}});
            self.call(
                connection,
                (Method::POST, &format!("sys/mounts/{STORE_MOUNT}")),
                root,
                Some(&body),
            )
            .await?;
        }
        if !self
            .listed(
                connection,
                "sys/auth",
                root,
                &format!("{STORE_AUTH_MOUNT}/"),
            )
            .await?
        {
            let body = serde_json::json!({"type": "approle"});
            self.call(
                connection,
                (Method::POST, &format!("sys/auth/{STORE_AUTH_MOUNT}")),
                root,
                Some(&body),
            )
            .await?;
        }
        for (name, policy) in [
            (DEPLOY, deploy_policy(self.project, &self.environment)),
            (OPERATIONS, operations_policy()),
        ] {
            self.call(
                connection,
                (Method::PUT, &format!("sys/policies/acl/{name}")),
                root,
                Some(&serde_json::json!({ "policy": policy })),
            )
            .await?;
        }
        let deploy = self.role(connection, root, DEPLOY).await?;
        let operations = self.role(connection, root, OPERATIONS).await?;
        Ok((deploy, operations))
    }

    /// The root token and unseal key of the store, initialising it when it is not.
    async fn initialised(
        &self,
        connection: &OpenBaoConnection,
    ) -> Result<ManagedRecoveryMaterial, MmError> {
        let id = self.id();
        let status = self.answering(connection).await?;
        if status.initialized {
            if self.connections.has_recovery(&id)? {
                return self.connections.recovery(&id);
            }
            return Err(MmError::Target {
                message: format!(
                    "the secret store of '{}' is initialised already; `mm target setup apply` gives its keys to the nodes that lack them from the recovery copy",
                    self.environment
                ),
            });
        }
        let body = serde_json::json!({"secret_shares": 1, "secret_threshold": 1});
        let response = self
            .transport
            .request(connection, Method::POST, "sys/init", None, Some(&body))
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(failed(&response, "its initialisation"));
        }
        let mut initialized: Initialized =
            serde_json::from_slice(&response.body).map_err(|_| malformed("its initialisation"))?;
        let pending = ManagedRecoveryMaterial {
            root_token: initialized.root_token,
            unseal_key: initialized
                .keys_base64
                .pop()
                .ok_or_else(|| malformed("its initialisation"))?,
        };
        self.connections.save_recovery(&id, pending.clone())?;
        Ok(pending)
    }

    /// `mmd openbao init`.
    pub async fn init(&self, request: StoreInitRequest) -> Result<StoreInitAnswer, MmError> {
        let id = self.id();
        let record = self.layout.record(&id)?.ok_or_else(|| MmError::Target {
            message: "this node runs no member of the store; `mm target setup apply` installs one"
                .to_owned(),
        })?;
        let ca =
            super::member::read_text(&self.layout.ca(&id))?.ok_or_else(|| MmError::Target {
                message: "the member holds no certificate authority".to_owned(),
            })?;
        let connection = self.loopback(ca.clone());
        let pending = self.initialised(&connection).await?;
        let keys = vec![pending.unseal_key.clone()];
        self.unsealed(&connection, &keys).await?;
        let (deploy, operations) = self.bootstrap(&connection, &pending.root_token).await?;
        let recovery = StoreRecovery {
            project_id: self.project,
            environment: self.environment.clone(),
            provider: record.provider.clone(),
            unseal_keys: keys,
            deploy,
            operations,
            ca_pem: ca,
            members: record.members.clone(),
        };
        let plaintext =
            Zeroizing::new(
                serde_json::to_vec(&recovery).map_err(|error| MmError::State {
                    message: format!("serializing the store's recovery document: {error}"),
                })?,
            );
        let mut sealed = BTreeMap::new();
        for (node, recipient) in &request.recipients {
            if *node != record.node {
                sealed.insert(
                    node.clone(),
                    crate::adapters::secrets::seal_to_recipient(recipient, &plaintext)?,
                );
            }
        }
        let answer = StoreInitAnswer {
            sealed,
            recovery: crate::adapters::secrets::seal_to_recipient(&request.operator, &plaintext)?,
        };
        self.keep(&recovery)?;
        self.call(
            &connection,
            (Method::POST, "auth/token/revoke-self"),
            &pending.root_token,
            None,
        )
        .await?;
        self.connections.remove_recovery(&id)?;
        if let Some(mut record) = self.layout.record(&id)? {
            record.initialized = true;
            self.layout.save_record(&id, &record)?;
        }
        Ok(answer)
    }
}
