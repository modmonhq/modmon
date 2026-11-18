//! Registry logins for one deploy, read from the environment's secrets.

use super::*;
use crate::domain::runtime::RegistryCredential;

impl EnvironmentManager {
    /// The project's `registries:` as declared, for the deployment plan.
    pub fn registries(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<BTreeMap<String, crate::domain::config::RegistryConfig>, MmError> {
        Ok(self
            .documents
            .load_from(&selection.start)?
            .config
            .registries)
    }

    /// Every `registries:` login for the selected environment. An unset
    /// password secret stops the deploy with how to create the token.
    pub async fn registry_credentials(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<Vec<RegistryCredential>, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        if document.config.registries.is_empty() {
            return Ok(Vec::new());
        }
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let scope = SecretScope::new(&environment);
        let mut credentials = Vec::with_capacity(document.config.registries.len());
        for (host, registry) in &document.config.registries {
            let name = &registry.password.secret;
            let declaration = document
                .config
                .secrets
                .get(name)
                .ok_or_else(|| MmError::Secret {
                    message: format!(
                        "registry '{host}' names secret '{name}', which secrets: does not declare"
                    ),
                })?;
            let reference = declaration
                .storage
                .reference()
                .map_err(|message| MmError::Secret { message })?;
            let material = self.resolver.resolve(&scope, &reference).await?.material;
            let Some(material) = material else {
                return Err(MmError::Secret {
                    message: unset_registry_secret(&document, &environment, host, name),
                });
            };
            credentials.push(RegistryCredential {
                registry: host.clone(),
                username: registry.username.clone(),
                password: material.into_zeroizing(),
            });
        }
        Ok(credentials)
    }
}

/// What to do when a registry's password secret has no value here.
#[must_use]
pub fn unset_registry_secret(
    document: &ConfigDocument,
    environment: &str,
    host: &str,
    secret: &str,
) -> String {
    let mut message = format!(
        "registry '{host}' needs secret '{secret}' in {environment}; set it with `mm -e {environment} secret set {secret}`"
    );
    if let Some(hint) =
        crate::domain::config::registry_token_hint(host, &document.config.project.name, environment)
    {
        message.push_str("; to get one, ");
        message.push_str(&hint);
    }
    message
}
