use std::collections::BTreeMap;

use async_trait::async_trait;
use reqwest::Method;
use zeroize::Zeroizing;

use crate::domain::error::MmError;

use super::connection_store::{VaultConnection, ca_sha256};

pub struct VaultHttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for VaultHttpResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VaultHttpResponse")
            .field("status", &self.status)
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("body", &"[REDACTED]")
            .finish()
    }
}

#[async_trait]
pub trait VaultHttpTransport: Send + Sync {
    async fn request(
        &self,
        connection: &VaultConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError>;
    async fn request_bytes(
        &self,
        connection: &VaultConnection,
        method: Method,
        path: &str,
        token: &str,
        body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError>;
}

#[derive(Debug, Default)]
pub struct ReqwestVaultTransport;

enum RequestBody<'a> {
    None,
    Json(&'a serde_json::Value),
    Bytes(&'a [u8]),
}

impl ReqwestVaultTransport {
    fn client(connection: &VaultConnection) -> Result<reqwest::Client, MmError> {
        if ca_sha256(&connection.ca_pem) != connection.ca_sha256 {
            return Err(MmError::Security {
                message: "Vault TLS trust-bundle pin mismatch".to_owned(),
            });
        }
        let certificate =
            reqwest::Certificate::from_pem(connection.ca_pem.as_bytes()).map_err(|_| {
                MmError::Security {
                    message: "Vault TLS CA certificate is invalid".to_owned(),
                }
            })?;
        reqwest::Client::builder()
            .https_only(true)
            .add_root_certificate(certificate)
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("mm-vault/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| MmError::Secret {
                message: format!("could not construct Vault TLS client: {error}"),
            })
    }

    fn url(connection: &VaultConnection, path: &str) -> Result<reqwest::Url, MmError> {
        let mut base = reqwest::Url::parse(&connection.address).map_err(|_| MmError::Security {
            message: "Vault address is invalid".to_owned(),
        })?;
        base.set_path("/");
        base.join(&format!("v1/{}", path.trim_start_matches('/')))
            .map_err(|_| MmError::Security {
                message: "Vault API path is invalid".to_owned(),
            })
    }

    async fn send(
        connection: &VaultConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: RequestBody<'_>,
    ) -> Result<VaultHttpResponse, MmError> {
        let client = Self::client(connection)?;
        let mut request = client.request(method.clone(), Self::url(connection, path)?);
        if let Some(namespace) = &connection.namespace {
            request = request.header("X-Vault-Namespace", namespace);
        }
        if let Some(token) = token {
            request = request.header("X-Vault-Token", token);
        }
        request = match body {
            RequestBody::None => request,
            RequestBody::Json(value) => request.json(value),
            RequestBody::Bytes(value) => request
                .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                .body(value.to_vec()),
        };
        let mut response = request.send().await.map_err(|error| MmError::Secret {
            message: if error.is_connect() || error.is_timeout() {
                "Vault is unavailable or its TLS identity could not be verified".to_owned()
            } else {
                format!("Vault request failed: {error}")
            },
        })?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| match value.to_str() {
                Ok(value) => Some((name.as_str().to_owned(), value.to_owned())),
                Err(_) => None,
            })
            .collect();
        let limit = if method == Method::GET && path == "sys/storage/raft/snapshot" {
            128 * 1024 * 1024
        } else {
            2 * 1024 * 1024
        };
        let mut bytes = Zeroizing::new(Vec::new());
        while let Some(chunk) = response.chunk().await.map_err(|error| MmError::Secret {
            message: format!("could not read Vault response: {error}"),
        })? {
            if bytes.len().saturating_add(chunk.len()) > limit {
                return Err(MmError::Security {
                    message: format!(
                        "Vault response exceeded the {} MiB safety limit",
                        limit / (1024 * 1024)
                    ),
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(VaultHttpResponse {
            status,
            headers,
            body: bytes,
        })
    }
}

#[async_trait]
impl VaultHttpTransport for ReqwestVaultTransport {
    async fn request(
        &self,
        connection: &VaultConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        Self::send(
            connection,
            method,
            path,
            token,
            body.map_or(RequestBody::None, RequestBody::Json),
        )
        .await
    }

    async fn request_bytes(
        &self,
        connection: &VaultConnection,
        method: Method,
        path: &str,
        token: &str,
        body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        Self::send(
            connection,
            method,
            path,
            Some(token),
            body.map_or(RequestBody::None, RequestBody::Bytes),
        )
        .await
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
