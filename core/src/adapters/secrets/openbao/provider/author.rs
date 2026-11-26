//! Who wrote each version, in the secret's custom metadata.

use reqwest::Method;

use super::{OpenBaoConnection, OpenBaoSecretStore};
use crate::adapters::secrets::kv_author::author_metadata;

impl OpenBaoSecretStore {
    /// Records who wrote `version` in the secret's custom metadata. A store
    /// whose policy refuses the metadata write keeps the version without it.
    pub(super) async fn record_author(
        &self,
        connection: &OpenBaoConnection,
        token: &str,
        data_path: &str,
        (version, set_by): (u64, Option<&str>),
    ) {
        let Some(set_by) = set_by else {
            return;
        };
        let path = data_path.replacen("/data/", "/metadata/", 1);
        let body = author_metadata(version, set_by);
        match self
            .transport
            .request(connection, Method::POST, &path, Some(token), Some(&body))
            .await
        {
            Ok(response) if (200..300).contains(&response.status) => {}
            Ok(response) => {
                tracing::debug!(
                    status = response.status,
                    "the provider kept no writer for this secret version"
                );
            }
            Err(error) => {
                tracing::debug!(
                    code = error.code(),
                    "the provider kept no writer for this secret version"
                );
            }
        }
    }
}
