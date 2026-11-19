use std::path::Path;

use super::{SecretManager, declaration, ensure_single_source, secret_reference, secret_scope};
use crate::app::generate_secret;
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::secret::{
    SecretGenerationItem, SecretGenerationReport, SecretGenerationSelection, SecretGenerationStatus,
};
use crate::domain::secret_contract::StorageOperation;

impl SecretManager {
    /// Creates the selected secrets that have no value yet. Outside `Named`,
    /// one whose provider has no connection from here is skipped.
    pub async fn generate(
        &self,
        start: &Path,
        environment: Option<&str>,
        selection: &SecretGenerationSelection,
    ) -> Result<SecretGenerationReport, MmError> {
        let document = self.documents.load_from(start)?;
        let scope = secret_scope(&document, environment)?;
        let names = generated_secrets(&document, scope.environment(), selection)?;
        let skips_unreachable = !matches!(selection, SecretGenerationSelection::Named(_));
        let now_unix_ms = self.clock.now_unix_ms();
        let mut secrets = Vec::with_capacity(names.len());
        for name in names {
            let declaration = declaration(&document, &name)?;
            ensure_single_source(&name, declaration)?;
            self.ensure_writable(&name, declaration, StorageOperation::Create)?;
            let generator = declaration
                .generate
                .as_ref()
                .ok_or_else(|| operator_supplied(&name))?;
            let material = generate_secret(generator)?;
            let reference = secret_reference(declaration)?;
            let provider = reference.scheme().to_owned();
            let item = match self
                .creator
                .create(&scope, &reference, material, now_unix_ms)
                .await
            {
                Ok(written) => SecretGenerationItem {
                    name,
                    provider,
                    status: if written.changed {
                        SecretGenerationStatus::Generated
                    } else {
                        SecretGenerationStatus::AlreadySet
                    },
                    revision: Some(written.revision),
                    reason: None,
                },
                Err(error) => match no_connection_here(&error).filter(|_| skips_unreachable) {
                    Some(reason) => SecretGenerationItem {
                        name,
                        provider,
                        status: SecretGenerationStatus::Skipped,
                        revision: None,
                        reason: Some(reason),
                    },
                    None => return Err(error),
                },
            };
            secrets.push(item);
        }
        Ok(SecretGenerationReport {
            project_name: document.config.project.name,
            environment: scope.environment().to_owned(),
            secrets,
        })
    }
}

/// The secrets `selection` generates in `environment`, by name. `Missing`
/// takes the ones `MmConfig::used_secrets` finds for the environment.
pub fn generated_secrets(
    document: &ConfigDocument,
    environment: &str,
    selection: &SecretGenerationSelection,
) -> Result<Vec<String>, MmError> {
    let used = match selection {
        SecretGenerationSelection::Named(name) => {
            return listed(document, std::slice::from_ref(name));
        }
        SecretGenerationSelection::Listed(names) => return listed(document, names),
        SecretGenerationSelection::Missing => Some(document.config.used_secrets(Some(environment))),
        SecretGenerationSelection::All => None,
    };
    Ok(document
        .config
        .secrets
        .iter()
        .filter(|(name, declaration)| {
            declaration.generate.is_some() && used.as_ref().is_none_or(|used| used.contains(*name))
        })
        .map(|(name, _)| name.clone())
        .collect())
}

fn listed(document: &ConfigDocument, names: &[String]) -> Result<Vec<String>, MmError> {
    names
        .iter()
        .map(|name| {
            if declaration(document, name)?.generate.is_none() {
                return Err(operator_supplied(name));
            }
            Ok(name.clone())
        })
        .collect()
}

fn operator_supplied(name: &str) -> MmError {
    MmError::Secret {
        message: format!(
            "secret '{name}' is operator-supplied and has no generator; use `mm secret set {name}` with hidden input or --stdin"
        ),
    }
}

/// Secret providers refuse with `MM_ENVIRONMENT` when this machine has no
/// usable connection to them for the environment.
fn no_connection_here(error: &MmError) -> Option<String> {
    match error {
        MmError::Environment { message } => Some(message.clone()),
        MmError::Unreachable { .. } => Some(error.to_string()),
        _ => None,
    }
}
