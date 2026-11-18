//! A provided certificate for an environment's domains, read from secrets.

use base64::Engine;

use super::*;
use crate::domain::config::EnvironmentTlsConfig;

/// The PEM certificate chain and key, never logged.
pub struct ProvidedCertificate {
    pub certificate: Zeroizing<Vec<u8>>,
    pub key: Zeroizing<Vec<u8>>,
}

impl EnvironmentManager {
    /// The selected environment's `tls:` setting, if any.
    pub fn environment_tls(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<Option<EnvironmentTlsConfig>, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(document
            .config
            .environments
            .get(&environment)
            .and_then(|config| config.tls.clone()))
    }

    /// Decodes the certificate and key the environment's `tls:` names.
    pub async fn provided_certificate(
        &self,
        selection: &WorkspaceSelection,
        tls: &EnvironmentTlsConfig,
    ) -> Result<ProvidedCertificate, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(ProvidedCertificate {
            certificate: self
                .pem_secret(&document, &environment, &tls.certificate, "CERTIFICATE")
                .await?,
            key: self
                .pem_secret(&document, &environment, &tls.key, "PRIVATE KEY")
                .await?,
        })
    }

    async fn pem_secret(
        &self,
        document: &ConfigDocument,
        environment: &str,
        name: &str,
        label: &str,
    ) -> Result<Zeroizing<Vec<u8>>, MmError> {
        let declaration = document
            .config
            .secrets
            .get(name)
            .ok_or_else(|| MmError::Secret {
                message: format!("tls names secret '{name}', which secrets: does not declare"),
            })?;
        let reference = declaration
            .storage
            .reference()
            .map_err(|message| MmError::Secret { message })?;
        let material = self
            .resolver
            .resolve(&SecretScope::new(environment), &reference)
            .await?
            .material
            .ok_or_else(|| MmError::Secret {
                message: format!("tls secret '{name}' has no value in {environment}"),
            })?;
        let pem = material.expose(|value| {
            base64::engine::general_purpose::STANDARD
                .decode(value.trim())
                .map(Zeroizing::new)
        });
        let pem = pem.map_err(|_| MmError::Secret {
            message: format!(
                "tls secret '{name}' is not base64; store the PEM file with `base64 -w0 FILE | mm secret set {name} --stdin`"
            ),
        })?;
        let contains = |needle: &[u8]| pem.windows(needle.len()).any(|window| window == needle);
        if !contains(b"-----BEGIN ") || !contains(label.as_bytes()) {
            return Err(MmError::Secret {
                message: format!("tls secret '{name}' does not hold a PEM {label}"),
            });
        }
        Ok(pem)
    }
}
