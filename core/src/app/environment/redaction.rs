//! The secret values hidden from what an environment's containers print.

use super::*;
use crate::app::Application;

/// Every secret the environment hands out, each of which must be readable
/// here, and every other declared secret in the same stores that is.
pub(crate) async fn environment_redactions(
    application: &Application,
    secrets: &dyn SecretMaterialResolver,
    document: &ConfigDocument,
    environment: &str,
) -> Result<Vec<Zeroizing<String>>, MmError> {
    let index = application.environment_index(document, environment);
    let delivered = delivered_secret_references(document, environment, &index)?;
    let scope = SecretScope::new(environment);
    let mut values = Vec::new();
    for (canonical, reference) in &delivered {
        let resolution = secrets.resolve(&scope, reference).await?;
        match (resolution.metadata.status, resolution.material) {
            (SecretStatus::Set, Some(material)) => values.push(material.into_zeroizing()),
            (SecretStatus::Unset, None) => {}
            (status, _) => return Err(unreadable(document, environment, canonical, status)),
        }
    }
    let stores = delivered
        .values()
        .map(SecretReference::provider)
        .collect::<BTreeSet<_>>();
    for declaration in document.config.secrets.values() {
        let Ok(reference) = declaration.storage.reference() else {
            continue;
        };
        if delivered.contains_key(&reference.canonical()) || !stores.contains(reference.provider())
        {
            continue;
        }
        if let Ok(resolution) = secrets.resolve(&scope, &reference).await
            && resolution.metadata.status == SecretStatus::Set
            && let Some(material) = resolution.material
        {
            values.push(material.into_zeroizing());
        }
    }
    Ok(values)
}

fn unreadable(
    document: &ConfigDocument,
    environment: &str,
    canonical: &str,
    status: SecretStatus,
) -> MmError {
    let name = document
        .config
        .secrets
        .iter()
        .find(|(_, declaration)| {
            configured_reference(declaration)
                .is_ok_and(|reference| reference.canonical() == canonical)
        })
        .map_or(canonical, |(name, _)| name.as_str());
    let state = match status {
        SecretStatus::ProviderUnavailable => "its store is unavailable",
        SecretStatus::AccessDenied => "its store denied access",
        SecretStatus::Invalid => "its store returned an unusable value",
        _ => "its store gave no usable answer",
    };
    MmError::Secret {
        message: format!(
            "secret '{name}', which {environment} hands to its services, cannot be read here ({state}), so output that may hold it cannot be redacted; `mm -e {environment} secret status {name}` shows its state"
        ),
    }
}
