use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier as _, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::app::ports::{
    CapSiteProvisioner, CapTokenVerifier, GateSessionCipher, PasswordVerifier, ProvisionedCapSite,
};
use crate::domain::error::MmError;
use crate::domain::gate::{
    ARGON2_ITERATIONS, ARGON2_MEMORY_KIB, ARGON2_PARALLELISM, ARGON2_VERSION, GateSessionClaims,
    PasswordHashRecord,
};
use crate::domain::secret::SecretMaterial;

#[derive(Debug, Clone, Copy, Default)]
pub struct Argon2idPasswordVerifier;

impl PasswordVerifier for Argon2idPasswordVerifier {
    fn hash(
        &self,
        password: &SecretMaterial,
        pepper: &SecretMaterial,
    ) -> Result<PasswordHashRecord, MmError> {
        let mut salt = [0_u8; 16];
        getrandom::fill(&mut salt).map_err(random_error)?;
        let salt = SaltString::encode_b64(&salt).map_err(|_| crypto_error())?;
        let encoded = combined_password(password, pepper, |combined| {
            argon2()?
                .hash_password(combined, &salt)
                .map(|hash| hash.to_string())
                .map_err(|_| crypto_error())
        })?;
        Ok(PasswordHashRecord {
            encoded,
            parameter_version: ARGON2_VERSION,
        })
    }

    fn verify(
        &self,
        password: &SecretMaterial,
        pepper: &SecretMaterial,
        record: &PasswordHashRecord,
    ) -> Result<bool, MmError> {
        let parsed = PasswordHash::new(&record.encoded).map_err(|_| crypto_error())?;
        combined_password(password, pepper, |combined| {
            Ok(argon2()?.verify_password(combined, &parsed).is_ok())
        })
    }

    fn requires_upgrade(&self, record: &PasswordHashRecord) -> bool {
        if record.parameter_version != ARGON2_VERSION {
            return true;
        }
        match PasswordHash::new(&record.encoded) {
            Ok(hash) => {
                hash.algorithm.as_str() != "argon2id"
                    || hash.params.get_decimal("m") != Some(ARGON2_MEMORY_KIB)
                    || hash.params.get_decimal("t") != Some(ARGON2_ITERATIONS)
                    || hash.params.get_decimal("p") != Some(ARGON2_PARALLELISM)
            }
            Err(_) => true,
        }
    }
}

fn argon2<'a>() -> Result<Argon2<'a>, MmError> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .map_err(|_| crypto_error())?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn combined_password<T>(
    password: &SecretMaterial,
    pepper: &SecretMaterial,
    action: impl FnOnce(&[u8]) -> Result<T, MmError>,
) -> Result<T, MmError> {
    let mut combined = zeroize::Zeroizing::new(Vec::new());
    password.expose(|value| combined.extend_from_slice(value.as_bytes()));
    combined.push(0);
    pepper.expose(|value| combined.extend_from_slice(value.as_bytes()));
    action(&combined)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct XChaChaSessionCipher;

impl GateSessionCipher for XChaChaSessionCipher {
    fn seal(
        &self,
        claims: &GateSessionClaims,
        key_version: u32,
        key: &SecretMaterial,
    ) -> Result<String, MmError> {
        let mut nonce = [0_u8; 24];
        getrandom::fill(&mut nonce).map_err(random_error)?;
        let plaintext = serde_json::to_vec(claims).map_err(|_| crypto_error())?;
        let aad = format!("mm-gate-session:v1:{key_version}");
        let encrypted = cipher(key)?
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| crypto_error())?;
        let encoding = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        Ok(format!(
            "v1.{key_version}.{}.{}",
            encoding.encode(nonce),
            encoding.encode(encrypted)
        ))
    }

    fn open(
        &self,
        value: &str,
        key_version: u32,
        key: &SecretMaterial,
    ) -> Result<GateSessionClaims, MmError> {
        let mut parts = value.split('.');
        let valid =
            parts.next() == Some("v1") && parts.next() == Some(key_version.to_string().as_str());
        let nonce = parts.next().ok_or_else(crypto_error)?;
        let encrypted = parts.next().ok_or_else(crypto_error)?;
        if !valid || parts.next().is_some() {
            return Err(crypto_error());
        }
        let encoding = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let nonce = encoding.decode(nonce).map_err(|_| crypto_error())?;
        let encrypted = encoding.decode(encrypted).map_err(|_| crypto_error())?;
        if nonce.len() != 24 {
            return Err(crypto_error());
        }
        let aad = format!("mm-gate-session:v1:{key_version}");
        let plaintext = cipher(key)?
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &encrypted,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| crypto_error())?;
        serde_json::from_slice(&plaintext).map_err(|_| crypto_error())
    }
}

fn cipher(material: &SecretMaterial) -> Result<XChaCha20Poly1305, MmError> {
    let bytes = material
        .expose(|value| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(value))
        .map_err(|_| crypto_error())?;
    if bytes.len() != 32 {
        return Err(crypto_error());
    }
    Ok(XChaCha20Poly1305::new(Key::from_slice(&bytes)))
}

#[derive(Debug, Clone)]
pub struct ReqwestCapTokenVerifier {
    client: reqwest::Client,
    endpoint: String,
    host_header: Option<String>,
}

impl ReqwestCapTokenVerifier {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, MmError> {
        let endpoint = endpoint.into();
        let loopback = reqwest::Url::parse(&endpoint).is_ok_and(|url| {
            url.scheme() == "http"
                && url.host_str() == Some("127.0.0.1")
                && url.port().is_some()
                && url.path() == "/"
                && url.query().is_none()
                && url.fragment().is_none()
        });
        if endpoint != "http://cap:3000" && !loopback && !endpoint.starts_with("https://") {
            return Err(MmError::Security {
                message: "Cap verification requires the internal managed endpoint or HTTPS"
                    .to_owned(),
            });
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| MmError::Secret {
                message: "could not initialize Cap verification".to_owned(),
            })?;
        Ok(Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            host_header: None,
        })
    }

    #[must_use]
    pub fn with_host_header(mut self, host: impl Into<String>) -> Self {
        self.host_header = Some(host.into());
        self
    }
}

#[derive(Deserialize)]
struct CapVerification {
    success: bool,
}

#[async_trait::async_trait]
impl CapTokenVerifier for ReqwestCapTokenVerifier {
    async fn verify(
        &self,
        site_key: &str,
        site_secret: &SecretMaterial,
        token: &SecretMaterial,
    ) -> Result<bool, MmError> {
        if !safe_site_key(site_key) {
            return Err(MmError::Security {
                message: "the configured Cap site key is invalid".to_owned(),
            });
        }
        let body = site_secret.expose(|secret| {
            token.expose(|response| serde_json::json!({ "secret": secret, "response": response }))
        });
        let mut request = self
            .client
            .post(format!("{}/{site_key}/siteverify", self.endpoint));
        if let Some(host) = &self.host_header {
            request = request.header(reqwest::header::HOST, host);
        }
        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !response.status().is_success() {
            return Err(unavailable());
        }
        response
            .json::<CapVerification>()
            .await
            .map(|value| value.success)
            .map_err(|_| unavailable())
    }
}

#[derive(Clone)]
pub struct ReqwestCapSiteProvisioner {
    client: reqwest::Client,
    endpoint: String,
    host: String,
    admin_key_file: PathBuf,
}

impl std::fmt::Debug for ReqwestCapSiteProvisioner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReqwestCapSiteProvisioner")
            .field("endpoint", &self.endpoint)
            .field("host", &self.host)
            .field("admin_key_file", &self.admin_key_file)
            .finish()
    }
}

impl ReqwestCapSiteProvisioner {
    pub fn new(
        endpoint: impl Into<String>,
        host: impl Into<String>,
        admin_key_file: PathBuf,
    ) -> Result<Self, MmError> {
        let endpoint = endpoint.into();
        let parsed = reqwest::Url::parse(&endpoint).map_err(|_| cap_admin_unavailable())?;
        if parsed.scheme() != "http"
            || parsed.host_str() != Some("127.0.0.1")
            || parsed.port().is_none()
            || parsed.path() != "/"
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(MmError::Security {
                message: "Cap administration requires an explicit loopback HTTP endpoint"
                    .to_owned(),
            });
        }
        let host = host.into();
        if host.is_empty() || host.chars().any(char::is_control) {
            return Err(MmError::Security {
                message: "Cap administration host is invalid".to_owned(),
            });
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| cap_admin_unavailable())?;
        Ok(Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_owned(),
            host,
            admin_key_file,
        })
    }

    fn admin_key(&self) -> Result<SecretMaterial, MmError> {
        use crate::adapters::private_fs::{ensure_private_file, read_regular_file};

        ensure_private_file(&self.admin_key_file).map_err(|_| cap_not_prepared())?;
        let bytes = read_regular_file(&self.admin_key_file, 64)
            .map_err(|_| cap_not_prepared())?
            .ok_or_else(cap_not_prepared)?;
        let value = String::from_utf8(bytes).map_err(|_| cap_not_prepared())?;
        if value.len() != 64 || value.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
            return Err(cap_not_prepared());
        }
        SecretMaterial::new(value).map_err(|_| cap_not_prepared())
    }

    async fn authorization(&self) -> Result<zeroize::Zeroizing<String>, MmError> {
        #[derive(Serialize)]
        struct LoginBody<'a> {
            admin_key: &'a str,
        }
        #[derive(Deserialize)]
        struct LoginResponse {
            success: bool,
            session_token: String,
            hashed_token: String,
        }

        let key = self.admin_key()?;
        let request = key.expose(|admin_key| {
            self.client
                .post(format!("{}/auth/login", self.endpoint))
                .header(reqwest::header::HOST, &self.host)
                .json(&LoginBody { admin_key })
        });
        let response = request.send().await.map_err(|_| cap_admin_unavailable())?;
        if !response.status().is_success() {
            return Err(cap_admin_unavailable());
        }
        let login = response
            .json::<LoginResponse>()
            .await
            .map_err(|_| cap_admin_unavailable())?;
        let session_token = zeroize::Zeroizing::new(login.session_token);
        let hashed_token = zeroize::Zeroizing::new(login.hashed_token);
        if !login.success || session_token.is_empty() || hashed_token.is_empty() {
            return Err(cap_admin_unavailable());
        }
        let payload = zeroize::Zeroizing::new(
            serde_json::to_vec(&serde_json::json!({
                "token": session_token.as_str(),
                "hash": hashed_token.as_str(),
            }))
            .map_err(|_| cap_admin_unavailable())?,
        );
        Ok(zeroize::Zeroizing::new(format!(
            "Bearer {}",
            base64::engine::general_purpose::STANDARD.encode(payload)
        )))
    }

    async fn remove_authorized(&self, site_key: &str, authorization: &str) -> Result<(), MmError> {
        if !safe_site_key(site_key) {
            return Err(MmError::Security {
                message: "the managed Cap site key is invalid".to_owned(),
            });
        }
        let response = self
            .client
            .delete(format!("{}/server/keys/{site_key}", self.endpoint))
            .header(reqwest::header::HOST, &self.host)
            .header(reqwest::header::AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|_| cap_admin_unavailable())?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(cap_admin_unavailable())
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapSiteSummary {
    site_key: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CapSiteCreation {
    site_key: String,
    secret_key: String,
}

#[async_trait::async_trait]
impl CapSiteProvisioner for ReqwestCapSiteProvisioner {
    async fn create(&self, name: &str) -> Result<ProvisionedCapSite, MmError> {
        if name.is_empty() || name.len() > 600 || name.chars().any(char::is_control) {
            return Err(MmError::Security {
                message: "managed Cap site name is invalid".to_owned(),
            });
        }
        let authorization = self.authorization().await?;
        let response = self
            .client
            .get(format!("{}/server/keys", self.endpoint))
            .header(reqwest::header::HOST, &self.host)
            .header(reqwest::header::AUTHORIZATION, authorization.as_str())
            .send()
            .await
            .map_err(|_| cap_admin_unavailable())?;
        if !response.status().is_success() {
            return Err(cap_admin_unavailable());
        }
        for site in response
            .json::<Vec<CapSiteSummary>>()
            .await
            .map_err(|_| cap_admin_unavailable())?
        {
            if site.name == name {
                self.remove_authorized(&site.site_key, authorization.as_str())
                    .await?;
            }
        }
        let response = self
            .client
            .post(format!("{}/server/keys", self.endpoint))
            .header(reqwest::header::HOST, &self.host)
            .header(reqwest::header::AUTHORIZATION, authorization.as_str())
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .map_err(|_| cap_admin_unavailable())?;
        if !response.status().is_success() {
            return Err(cap_admin_unavailable());
        }
        let created = response
            .json::<CapSiteCreation>()
            .await
            .map_err(|_| cap_admin_unavailable())?;
        if !safe_site_key(&created.site_key) {
            return Err(cap_admin_unavailable());
        }
        let site_secret =
            SecretMaterial::new(created.secret_key).map_err(|_| cap_admin_unavailable())?;
        Ok(ProvisionedCapSite {
            site_key: created.site_key,
            site_secret,
        })
    }

    async fn remove(&self, site_key: &str) -> Result<(), MmError> {
        let authorization = self.authorization().await?;
        self.remove_authorized(site_key, authorization.as_str())
            .await
    }
}

fn cap_not_prepared() -> MmError {
    MmError::State {
            message: "managed Cap is not prepared; review `mm dev services plan --gate`, then apply the exact plan with `mm dev services apply --gate`".to_owned(),
    }
}

fn cap_admin_unavailable() -> MmError {
    MmError::RuntimeResponse {
        message: "managed Cap administration is unavailable".to_owned(),
    }
}

fn safe_site_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
fn crypto_error() -> MmError {
    MmError::Security {
        message: "gate credential processing failed".to_owned(),
    }
}
fn random_error(_: getrandom::Error) -> MmError {
    MmError::Security {
        message: "secure randomness is unavailable".to_owned(),
    }
}
fn unavailable() -> MmError {
    MmError::RuntimeResponse {
        message: "Cap verification is unavailable; authentication failed closed".to_owned(),
    }
}

#[cfg(test)]
#[path = "gate/tests.rs"]
mod tests;
