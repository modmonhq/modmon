//! The plaintext of a sealed secret bundle: the secrets one environment's host
//! needs and, optionally, the environment's git deploy key. The operator's
//! machine builds it from its own age store, seals it to the host store's age
//! recipient, and `mmd secrets import` opens it on the host.
//!
//! The wire form is versioned JSON. Secret values travel as the text the age
//! store holds; the deploy key's private half travels as standard base64 of
//! its raw OpenSSH bytes. A store keeps a deploy key as one line of base64
//! (secret values reject control characters), so the sender decodes it and the
//! bundle carries the raw key. Nothing here prints a value: `Debug` is
//! redacted and decode errors report only a position.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use base64::Engine;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::domain::environment::is_safe_environment_value;
use crate::domain::host_policy::parse_public_key;
use crate::domain::secret::{SecretMaterial, SecretReference};
use crate::domain::workspace::ProjectId;

/// The only plaintext version this build reads and writes.
pub const SECRET_BUNDLE_VERSION: u32 = 1;
/// Upper bound on a bundle's plaintext; the host age store has the same limit.
pub const MAX_SECRET_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPLOY_KEY_BYTES: usize = 16 * 1024;
const MAX_REVISION_BYTES: usize = 128;
const MAX_SET_BY_BYTES: usize = 255;

/// Everything one host receives for one project environment.
pub struct SecretBundle {
    pub project_id: ProjectId,
    pub environment: String,
    pub secrets: Vec<BundledSecret>,
    pub deploy_key: Option<BundledDeployKey>,
}

/// One age secret: its reference as the store keys it within the
/// environment, its revision in the sender's store, its value, and when and
/// by whom that revision was written.
pub struct BundledSecret {
    pub reference: SecretReference,
    pub revision: String,
    pub material: SecretMaterial,
    pub set_at_unix_ms: Option<i64>,
    pub set_by: Option<String>,
}

/// The environment's read-only git deploy key.
pub struct BundledDeployKey {
    /// Raw OpenSSH private key bytes, as `ssh-keygen` writes them.
    pub private_key: Zeroizing<Vec<u8>>,
    /// The public line, `ssh-ed25519 AAAA… comment`, added to the git host.
    pub public_key: String,
}

impl SecretBundle {
    /// Checks the bundle's own rules: a valid environment name, unique age
    /// references without connections, bounded revisions, values the age
    /// store accepts, and an ed25519 deploy key.
    pub fn validate(&self) -> Result<(), String> {
        crate::domain::host_policy::validate_environment_name(&self.environment)?;
        let mut references = BTreeSet::new();
        for secret in &self.secrets {
            let reference = &secret.reference;
            if reference.scheme() != "age" || reference.connection().is_some() {
                return Err("secret bundles carry only age references".to_owned());
            }
            let display = format!("age://{}", reference.locator());
            if !references.insert(reference.locator().to_owned()) {
                return Err(format!("secret bundle lists {display} twice"));
            }
            validate_secret_revision(&secret.revision)
                .map_err(|message| format!("{display}: {message}"))?;
            if let Some(set_by) = &secret.set_by {
                validate_set_by(set_by).map_err(|message| format!("{display}: {message}"))?;
            }
            if !secret.material.expose(is_safe_environment_value) {
                return Err(format!(
                    "{display}: secret values may not contain control characters"
                ));
            }
        }
        if let Some(deploy_key) = &self.deploy_key {
            deploy_key.validate()?;
        }
        Ok(())
    }

    /// Serializes the validated bundle to its versioned JSON plaintext.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, String> {
        self.validate()?;
        let wire = WireBundle {
            version: SECRET_BUNDLE_VERSION,
            project: self.project_id,
            environment: self.environment.clone(),
            secrets: self
                .secrets
                .iter()
                .map(|secret| WireSecret {
                    reference: secret.reference.clone(),
                    revision: secret.revision.clone(),
                    value: secret
                        .material
                        .expose(|value| Zeroizing::new(value.to_owned())),
                    set_at_unix_ms: secret.set_at_unix_ms,
                    set_by: secret.set_by.clone(),
                })
                .collect(),
            deploy_key: self.deploy_key.as_ref().map(|key| WireDeployKey {
                private_key: Zeroizing::new(
                    base64::engine::general_purpose::STANDARD.encode(key.private_key.as_slice()),
                ),
                public_key: key.public_key.clone(),
            }),
        };
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&wire)
                .map_err(|error| format!("could not serialize the secret bundle: {error}"))?,
        );
        if plaintext.len() > MAX_SECRET_BUNDLE_BYTES {
            return Err(format!(
                "secret bundle exceeds the {MAX_SECRET_BUNDLE_BYTES}-byte limit"
            ));
        }
        Ok(plaintext)
    }

    /// Parses and validates a versioned JSON plaintext.
    pub fn decode(plaintext: &[u8]) -> Result<Self, String> {
        if plaintext.len() > MAX_SECRET_BUNDLE_BYTES {
            return Err(format!(
                "secret bundle exceeds the {MAX_SECRET_BUNDLE_BYTES}-byte limit"
            ));
        }
        let version = serde_json::from_slice::<WireVersion>(plaintext)
            .map_err(|error| redacted_json_error(&error))?
            .version;
        if version != SECRET_BUNDLE_VERSION {
            return Err(format!(
                "secret bundle version {version} is unsupported; expected {SECRET_BUNDLE_VERSION}"
            ));
        }
        let wire = serde_json::from_slice::<WireBundle>(plaintext)
            .map_err(|error| redacted_json_error(&error))?;
        let secrets = wire
            .secrets
            .into_iter()
            .map(|secret| {
                Ok(BundledSecret {
                    material: SecretMaterial::from_zeroizing(secret.value).map_err(|message| {
                        format!("age://{}: {message}", secret.reference.locator())
                    })?,
                    reference: secret.reference,
                    revision: secret.revision,
                    set_at_unix_ms: secret.set_at_unix_ms,
                    set_by: secret.set_by,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let deploy_key = wire
            .deploy_key
            .map(|key| {
                let private_key = Zeroizing::new(
                    base64::engine::general_purpose::STANDARD
                        .decode(key.private_key.as_bytes())
                        .map_err(|_| "deploy key private half is not valid base64".to_owned())?,
                );
                Ok::<_, String>(BundledDeployKey {
                    private_key,
                    public_key: key.public_key,
                })
            })
            .transpose()?;
        let bundle = Self {
            project_id: wire.project,
            environment: wire.environment,
            secrets,
            deploy_key,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    /// Rejects a bundle built for another project or environment.
    pub fn ensure_scope(&self, project_id: ProjectId, environment: &str) -> Result<(), String> {
        if self.project_id != project_id {
            return Err(format!(
                "secret bundle is for project {}, not {}",
                self.project_id.0, project_id.0
            ));
        }
        if self.environment != environment {
            return Err(format!(
                "secret bundle is for environment '{}', not '{environment}'",
                self.environment
            ));
        }
        Ok(())
    }
}

impl BundledDeployKey {
    fn validate(&self) -> Result<(), String> {
        if self.private_key.is_empty() || self.private_key.len() > MAX_DEPLOY_KEY_BYTES {
            return Err(format!(
                "deploy key private half must be 1-{MAX_DEPLOY_KEY_BYTES} bytes"
            ));
        }
        let public = parse_public_key(&self.public_key)
            .map_err(|message| format!("deploy key public half: {message}"))?;
        if public.algorithm != "ssh-ed25519" {
            return Err("deploy keys must be ssh-ed25519".to_owned());
        }
        Ok(())
    }
}

/// A revision is an opaque store token; bundles carry it as the sender's
/// store wrote it so the receiving store can keep it.
pub fn validate_secret_revision(revision: &str) -> Result<(), String> {
    if revision.is_empty()
        || revision.len() > MAX_REVISION_BYTES
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err(format!(
            "revision must be 1-{MAX_REVISION_BYTES} ASCII letters, numbers, '-', '_', '.' or ':'"
        ));
    }
    Ok(())
}

/// Who wrote a revision, as a store records it: 1-255 bytes of text
/// without control characters.
pub fn validate_set_by(set_by: &str) -> Result<(), String> {
    if set_by.is_empty() || set_by.len() > MAX_SET_BY_BYTES || set_by.chars().any(char::is_control)
    {
        return Err(format!(
            "the writer's name must be 1-{MAX_SET_BY_BYTES} bytes without control characters"
        ));
    }
    Ok(())
}

/// A serde error can quote the input it rejected, so only its kind and
/// position are kept.
fn redacted_json_error(error: &serde_json::Error) -> String {
    format!(
        "secret bundle is not valid ({:?} error at line {}, column {})",
        error.classify(),
        error.line(),
        error.column()
    )
}

#[derive(Deserialize)]
struct WireVersion {
    version: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct WireBundle {
    version: u32,
    project: ProjectId,
    environment: String,
    secrets: Vec<WireSecret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deploy_key: Option<WireDeployKey>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSecret {
    reference: SecretReference,
    revision: String,
    value: Zeroizing<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    set_at_unix_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    set_by: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct WireDeployKey {
    private_key: Zeroizing<String>,
    public_key: String,
}

/// What an import changed, keyed by reference. It holds no values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SecretImportSummary {
    pub project: ProjectId,
    pub environment: String,
    pub secrets: BTreeMap<String, SecretImportEntry>,
    pub deploy_key: Option<DeployKeyImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretImportEntry {
    pub revision: String,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DeployKeyImport {
    pub public_key: String,
    pub changed: bool,
}

impl fmt::Debug for SecretBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretBundle")
            .field("project_id", &self.project_id)
            .field("environment", &self.environment)
            .field("secrets", &self.secrets)
            .field("deploy_key", &self.deploy_key)
            .finish()
    }
}

impl fmt::Debug for BundledSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BundledSecret")
            .field("reference", &self.reference)
            .field("revision", &self.revision)
            .field("material", &"[REDACTED]")
            .field("set_at_unix_ms", &self.set_at_unix_ms)
            .field("set_by", &self.set_by)
            .finish()
    }
}

impl fmt::Debug for BundledDeployKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BundledDeployKey")
            .field("private_key", &"[REDACTED]")
            .field("public_key", &self.public_key)
            .finish()
    }
}

#[cfg(test)]
mod tests;
