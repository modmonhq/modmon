//! The live versions of a managed cluster store: what a staged change
//! becomes once its deployment succeeded.

use reqwest::Method;
use zeroize::Zeroizing;

use super::{
    MetadataResponse, OpenBaoSecretStore, ReadResponse, parse_json, parse_timestamp,
    require_success, unavailable, unavailable_metadata, unset_metadata,
};
use crate::domain::error::MmError;
use crate::domain::secret::{SecretMetadata, SecretReference, SecretScope, SecretStatus};

/// How often a write races another node's for the next version.
const WRITE_ATTEMPTS: usize = 4;

impl OpenBaoSecretStore {
    /// The connection and its health; for a managed cluster store whose
    /// member at the connection's address does not serve, another member's.
    pub(super) async fn serving(
        &self,
        connection: super::OpenBaoConnection,
    ) -> Result<(super::OpenBaoConnection, super::OpenBaoHealth), MmError> {
        let first = self.health_for_connection(&connection).await;
        let serves = |health: &super::OpenBaoHealth| health.initialized && !health.sealed;
        if first.as_ref().is_ok_and(serves) {
            return first.map(|health| (connection, health));
        }
        let Some(recovery) = connection
            .managed_installation
            .as_deref()
            .map(|id| self.connections.cluster_store(id))
            .transpose()?
            .flatten()
        else {
            return first.map(|health| (connection, health));
        };
        for member in &recovery.members {
            let mut other = connection.clone();
            other.address = format!(
                "https://{}:{}",
                member.address,
                crate::domain::secret_store::STORE_API_PORT
            );
            if other.address == connection.address {
                continue;
            }
            if let Ok(health) = self.health_for_connection(&other).await
                && serves(&health)
            {
                return Ok((other, health));
            }
        }
        first.map(|health| (connection, health))
    }

    /// The current version, never what is staged: its revision and when it
    /// was written.
    pub async fn live(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        self.authenticated(scope, reference, async |connection, token, path| {
            let response = self
                .transport
                .request(connection, Method::GET, &path, Some(token), None)
                .await?;
            if response.status == 404 {
                return Ok(unset_metadata());
            }
            require_success(&response, "secret read")?;
            let read = parse_json::<ReadResponse>(&response, "secret read response")?;
            Ok(SecretMetadata {
                status: SecretStatus::Set,
                revision: Some(read.revision()),
                created_at_unix_ms: parse_timestamp(&read.data.metadata.created_time),
                candidate_revision: None,
            })
        })
        .await
        .or_else(|error| {
            if unavailable(&error) {
                Ok(unavailable_metadata())
            } else {
                Err(error)
            }
        })
    }

    /// Writes `value` as a version carrying `revision`, unless the current one does.
    pub async fn write_live(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
        (value, revision): (&Zeroizing<String>, &str),
        now_unix_ms: i64,
    ) -> Result<bool, MmError> {
        self.authenticated(scope, reference, async |connection, token, path| {
            for _ in 0..WRITE_ATTEMPTS {
                let response = self
                    .transport
                    .request(connection, Method::GET, &path, Some(token), None)
                    .await?;
                let version = if response.status == 404 {
                    let metadata_path = path.replacen("/data/", "/metadata/", 1);
                    let metadata = self
                        .transport
                        .request(connection, Method::GET, &metadata_path, Some(token), None)
                        .await?;
                    if metadata.status == 404 {
                        0
                    } else {
                        require_success(&metadata, "secret metadata inspection")?;
                        parse_json::<MetadataResponse>(&metadata, "secret metadata response")?
                            .data
                            .current_version
                    }
                } else {
                    require_success(&response, "secret read")?;
                    let read = parse_json::<ReadResponse>(&response, "secret read response")?;
                    if read.data.data.mm_revision.as_deref() == Some(revision) {
                        return Ok(false);
                    }
                    read.data.metadata.version
                };
                let body = serde_json::json!({
                    "options": { "cas": version },
                    "data": {
                        "value": value.as_str(),
                        "mm_revision": revision,
                        "mm_created_at_unix_ms": now_unix_ms,
                    }
                });
                let written = self
                    .transport
                    .request(connection, Method::POST, &path, Some(token), Some(&body))
                    .await?;
                if matches!(written.status, 400 | 409) {
                    continue;
                }
                require_success(&written, "secret write")?;
                return Ok(true);
            }
            Err(MmError::Environment {
                message: format!(
                    "secret '{}' kept changing in the store while this node wrote its new version; deploy again",
                    reference.locator()
                ),
            })
        })
        .await
    }

    /// Removes the current version, unless there is none. True when this
    /// call removed it.
    pub async fn remove_live(
        &self,
        scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<bool, MmError> {
        self.authenticated(scope, reference, async |connection, token, path| {
            let response = self
                .transport
                .request(connection, Method::GET, &path, Some(token), None)
                .await?;
            if response.status == 404 {
                return Ok(false);
            }
            require_success(&response, "secret read")?;
            let removed = self
                .transport
                .request(connection, Method::DELETE, &path, Some(token), None)
                .await?;
            require_success(&removed, "secret removal")?;
            Ok(true)
        })
        .await
    }
}
