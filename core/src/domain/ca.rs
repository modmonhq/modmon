use std::collections::BTreeSet;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FINGERPRINT_PREFIX: &str = "sha256:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FingerprintAlgorithm {
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CertificateFingerprint {
    pub algorithm: FingerprintAlgorithm,
    pub bytes: [u8; 32],
}

impl CertificateFingerprint {
    #[must_use]
    pub fn from_der(der: &[u8]) -> Self {
        let digest = Sha256::digest(der);
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        Self {
            algorithm: FingerprintAlgorithm::Sha256,
            bytes,
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let hex = value.strip_prefix(FINGERPRINT_PREFIX).ok_or_else(|| {
            format!("certificate fingerprint '{value}' does not use the sha256: prefix")
        })?;
        if hex.len() != 64 || hex.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
            return Err(format!(
                "certificate fingerprint '{value}' is not a SHA-256 hex digest"
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, chunk) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            let text = std::str::from_utf8(chunk)
                .map_err(|_| format!("certificate fingerprint '{value}' is not valid UTF-8"))?;
            bytes[index] = u8::from_str_radix(text, 16)
                .map_err(|_| format!("certificate fingerprint '{value}' is not hexadecimal"))?;
        }
        Ok(Self {
            algorithm: FingerprintAlgorithm::Sha256,
            bytes,
        })
    }
}

impl fmt::Display for CertificateFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{FINGERPRINT_PREFIX}")?;
        for byte in self.bytes {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LocalCaStatus {
    pub installed: bool,
    pub fingerprint: Option<String>,
    pub generation: u64,
    pub issued_hosts: BTreeSet<String>,
    pub key_permissions_valid: bool,
    pub trust_disabled: bool,
    pub rotation_pending: bool,
    #[serde(default)]
    pub leaves: Vec<LeafCertificateStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<TrustStoreStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TrustStoreStatus {
    pub supported: bool,
    pub installed: bool,
    pub platform: String,
    pub scope: TrustScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    pub mismatch: bool,
    pub needs_privilege: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrustScope {
    CurrentUser,
    System,
}

impl fmt::Display for TrustScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CurrentUser => "current_user",
            Self::System => "system",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LeafCertificateStatus {
    pub hosts: Vec<String>,
    pub fingerprint: String,
    pub generation: u64,
    pub expires_at_unix: i64,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IssuedCertificate {
    pub hosts: Vec<String>,
    pub fingerprint: String,
    pub generation: u64,
    pub expires_at_unix: i64,
}

#[must_use]
pub fn is_local_ca_host(host: &str) -> bool {
    let Ok(host) = normalize_hostname(host) else {
        return false;
    };
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".test")
        || host.ends_with(".invalid")
        || host.ends_with(".local")
}

#[must_use]
pub fn local_ca_compatibility_warning(host: &str) -> Option<String> {
    let Ok(host) = normalize_hostname(host) else {
        return None;
    };
    if host == "localhost" || host.ends_with(".local") {
        Some(format!(
            "hostname '{host}' uses .local, which collides with mDNS; prefer .test or .localhost"
        ))
    } else {
        None
    }
}

pub fn certificate_der(pem_or_der: &[u8]) -> Result<Vec<u8>, String> {
    let Ok(text) = std::str::from_utf8(pem_or_der) else {
        return Ok(pem_or_der.to_vec());
    };
    if !text.contains("BEGIN CERTIFICATE") {
        return Ok(pem_or_der.to_vec());
    }
    let mut encoded = String::new();
    let mut in_body = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("-----BEGIN CERTIFICATE-----") {
            in_body = true;
            continue;
        }
        if line.starts_with("-----END CERTIFICATE-----") {
            break;
        }
        if in_body {
            encoded.push_str(line);
        }
    }
    if encoded.is_empty() {
        return Err("certificate PEM did not contain a CERTIFICATE body".to_owned());
    }
    STANDARD
        .decode(encoded.as_bytes())
        .map_err(|error| format!("certificate PEM is not valid base64: {error}"))
}

pub fn pem_certificates(pem: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let text = std::str::from_utf8(pem)
        .map_err(|_| "certificate bundle is not valid UTF-8 PEM".to_owned())?;
    let mut certificates = Vec::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in text.lines() {
        match line.trim() {
            "-----BEGIN CERTIFICATE-----" => {
                if in_body {
                    return Err("certificate bundle contains nested PEM boundaries".to_owned());
                }
                body.clear();
                in_body = true;
            }
            "-----END CERTIFICATE-----" if in_body => {
                let der = STANDARD.decode(body.as_bytes()).map_err(|error| {
                    format!("certificate bundle contains invalid base64: {error}")
                })?;
                certificates.push(der);
                in_body = false;
            }
            line if in_body => body.push_str(line),
            _ => {}
        }
    }
    if in_body {
        return Err("certificate bundle has an unterminated PEM certificate".to_owned());
    }
    Ok(certificates)
}

pub fn bundle_contains_fingerprint(bundle: &[u8], expected: &str) -> Result<bool, String> {
    let expected = CertificateFingerprint::parse(expected)?;
    Ok(pem_certificates(bundle)?
        .iter()
        .any(|der| CertificateFingerprint::from_der(der) == expected))
}

pub fn certificate_fingerprint(pem_or_der: &[u8]) -> Result<CertificateFingerprint, String> {
    Ok(CertificateFingerprint::from_der(&certificate_der(
        pem_or_der,
    )?))
}

pub fn normalize_hostname(host: &str) -> Result<String, String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host.len() > 253 || host.starts_with('.') || host.contains("..") {
        return Err(format!(
            "hostname '{host}' is not valid for local CA issuance"
        ));
    }
    for label in host.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(format!(
                "hostname '{host}' is not valid for local CA issuance"
            ));
        }
    }
    Ok(host)
}

pub fn validate_hostname(host: &str) -> Result<String, String> {
    normalize_hostname(host)
}

pub fn unique_dns_names(
    hosts: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for host in hosts {
        let host = normalize_hostname(host.as_ref())?;
        if !names.contains(&host) {
            names.push(host);
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests;
