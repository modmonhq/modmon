//! A member whose data is gone: its old Raft entry removed through a
//! member that serves, and the versions the store keeps, read without a
//! value.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::Deserialize;

use super::{NodeStore, failed, login, malformed, peers, seal_status};
use crate::domain::error::MmError;
use crate::domain::secret_store::{
    STORE_MOUNT, StorePeerRemoval, StoreRecovery, StoreSecretVersion, StoreSecretVersions,
};

/// How many locations `mmd openbao versions` reads at most.
const MAX_LOCATIONS: usize = 4096;

#[derive(Deserialize)]
struct Listing {
    data: ListingData,
}

#[derive(Deserialize)]
struct ListingData {
    keys: Vec<String>,
}

#[derive(Deserialize)]
struct Metadata {
    data: MetadataData,
}

#[derive(Deserialize)]
struct MetadataData {
    current_version: u64,
    #[serde(default)]
    versions: BTreeMap<String, MetadataVersion>,
}

#[derive(Deserialize)]
struct MetadataVersion {
    #[serde(default)]
    created_time: String,
    #[serde(default)]
    deletion_time: String,
    #[serde(default)]
    destroyed: bool,
}

impl NodeStore {
    /// This node's member, unsealed, with the keys and logins it holds.
    async fn serving_here(
        &self,
    ) -> Result<(super::OpenBaoConnection, String, StoreRecovery), MmError> {
        let id = self.id();
        let record = self.layout.record(&id)?.ok_or_else(|| MmError::Target {
            message: "no member of the environment's secret store runs on this node".to_owned(),
        })?;
        let held = self.held()?;
        let connection = self.loopback(self.member_ca()?);
        let status = seal_status(self.transport.as_ref(), &connection).await?;
        if !status.initialized || status.sealed {
            return Err(MmError::Target {
                message: format!(
                    "the secret store's member on {} does not serve: it is sealed or not initialised",
                    record.node
                ),
            });
        }
        Ok((connection, record.node, held))
    }

    /// `mmd openbao remove-peer`: removes `peer` from the Raft configuration
    /// through this node's member. Never this node's own member, nor the
    /// member that leads.
    pub async fn remove_peer(&self, peer: &str) -> Result<StorePeerRemoval, MmError> {
        let (connection, node, held) = self.serving_here().await?;
        if peer == node {
            return Err(MmError::Target {
                message: format!("the member on {node} does not remove itself"),
            });
        }
        let transport = self.transport.as_ref();
        let token = login(transport, &connection, &held.operations).await?;
        let listed = peers(transport, &connection, &token).await?;
        let Some(entry) = listed.iter().find(|entry| entry.name == peer) else {
            return Ok(StorePeerRemoval {
                peer: peer.to_owned(),
                through: node,
                removed: false,
            });
        };
        if entry.leader {
            return Err(MmError::Target {
                message: format!(
                    "the member on {peer} leads the store; only a member that is gone is removed"
                ),
            });
        }
        let body = serde_json::json!({ "server_id": peer });
        let response = transport
            .request(
                &connection,
                Method::POST,
                "sys/storage/raft/remove-peer",
                Some(&token),
                Some(&body),
            )
            .await?;
        if !(200..300).contains(&response.status) {
            return Err(failed(&response, "the removal of a Raft peer"));
        }
        if peers(transport, &connection, &token)
            .await?
            .iter()
            .any(|entry| entry.name == peer)
        {
            return Err(MmError::Target {
                message: format!("the Raft configuration still lists {peer} after its removal"),
            });
        }
        Ok(StorePeerRemoval {
            peer: peer.to_owned(),
            through: node,
            removed: true,
        })
    }

    /// `mmd openbao versions`: every secret of the environment the store
    /// keeps, with its versions, read through this node's member.
    pub async fn versions(&self) -> Result<Vec<StoreSecretVersions>, MmError> {
        let (connection, _, held) = self.serving_here().await?;
        let transport = self.transport.as_ref();
        let token = login(transport, &connection, &held.deploy).await?;
        let prefix = format!("mm/{}/{}", self.project.0, self.environment);
        let mut pending = vec![String::new()];
        let mut locations = Vec::new();
        while let Some(folder) = pending.pop() {
            let path = format!("{STORE_MOUNT}/metadata/{prefix}/{folder}?list=true");
            let response = transport
                .request(&connection, Method::GET, &path, Some(&token), None)
                .await?;
            if response.status == 404 {
                continue;
            }
            if !(200..300).contains(&response.status) {
                return Err(failed(
                    &response,
                    "the listing of the environment's secrets",
                ));
            }
            let listing: Listing = serde_json::from_slice(&response.body)
                .map_err(|_| malformed("the listing of the environment's secrets"))?;
            for key in listing.data.keys {
                let location = format!("{folder}{key}");
                if key.ends_with('/') {
                    pending.push(location);
                } else {
                    locations.push(location);
                }
            }
            if locations.len() + pending.len() > MAX_LOCATIONS {
                return Err(MmError::Target {
                    message: format!(
                        "the environment keeps more than {MAX_LOCATIONS} secrets in the store"
                    ),
                });
            }
        }
        locations.sort();
        let mut secrets = Vec::with_capacity(locations.len());
        for location in locations {
            let path = format!("{STORE_MOUNT}/metadata/{prefix}/{location}");
            let response = transport
                .request(&connection, Method::GET, &path, Some(&token), None)
                .await?;
            if !(200..300).contains(&response.status) {
                return Err(failed(&response, "a secret's metadata"));
            }
            let metadata: Metadata = serde_json::from_slice(&response.body)
                .map_err(|_| malformed("a secret's metadata"))?;
            let mut versions = metadata
                .data
                .versions
                .into_iter()
                .map(|(number, version)| {
                    Ok(StoreSecretVersion {
                        version: number
                            .parse()
                            .map_err(|_| malformed("a secret's metadata"))?,
                        created: version.created_time,
                        deleted: !version.deletion_time.is_empty(),
                        destroyed: version.destroyed,
                    })
                })
                .collect::<Result<Vec<_>, MmError>>()?;
            versions.sort_by_key(|version| version.version);
            secrets.push(StoreSecretVersions {
                location,
                current: metadata.data.current_version,
                versions,
            });
        }
        Ok(secrets)
    }
}
