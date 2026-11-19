//! Where the hosts of a target read the project's external secret stores.
//!
//! A host holds its own connection to a store (`mmd secrets connect`: the
//! address, the certificate authority, and an AppRole credential that never
//! leaves it). The repo declares the address, so where an environment's
//! secrets come from is reviewed like the rest of the configuration, and a
//! host refuses to plan when it is connected somewhere else.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::config::MmConfig;
use crate::domain::secret_backend::{ProviderConfiguration, ProviderInstanceName};

/// What a target changes about one of the project's `providers:`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct TargetProviderConfig {
    /// Where this target's hosts reach the store, for example
    /// `https://127.0.0.1:8200` when it runs on the host itself, or a mesh
    /// address. Replaces the provider's own `address` on this target.
    pub address: String,
}

/// The address the repo declares for one external provider: the provider's
/// own, or the one the environment's target sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeclaredProviderAddress {
    default: String,
    environments: BTreeMap<String, String>,
}

impl DeclaredProviderAddress {
    #[must_use]
    pub fn for_environment(&self, environment: &str) -> &str {
        self.environments
            .get(environment)
            .map_or(self.default.as_str(), String::as_str)
    }

    /// Whether a host connected to `address` reads the store this
    /// environment declares. A trailing slash makes no difference.
    #[must_use]
    pub fn matches(&self, environment: &str, address: &str) -> bool {
        normalized(self.for_environment(environment)) == normalized(address)
    }
}

fn normalized(address: &str) -> String {
    address.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// An address a host may be told to read secrets from: HTTPS, a host name,
/// no credentials, and no path, query or fragment.
pub fn validate_provider_address(address: &str) -> Result<(), String> {
    let rest = address
        .strip_prefix("https://")
        .ok_or_else(|| format!("'{address}' must start with https://"))?;
    let authority = rest.trim_end_matches('/');
    if authority.is_empty()
        || authority
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '/' | '?' | '#'))
    {
        return Err(format!(
            "'{address}' must be https://HOST or https://HOST:PORT, without a path"
        ));
    }
    if authority.contains('@') {
        return Err("a provider address may not contain credentials".to_owned());
    }
    Ok(())
}

impl MmConfig {
    /// The name of the target an environment deploys to.
    pub(crate) fn target_of(&self, environment: &str) -> &str {
        self.environments
            .get(environment)
            .and_then(|environment| environment.target.as_deref())
            .unwrap_or(self.target.as_str())
    }

    /// For every external provider, the address each environment's hosts
    /// read it at.
    #[must_use]
    pub fn declared_provider_addresses(
        &self,
    ) -> BTreeMap<ProviderInstanceName, DeclaredProviderAddress> {
        self.providers
            .iter()
            .filter_map(|(name, instance)| {
                let ProviderConfiguration::External { address, .. } = &instance.configuration
                else {
                    return None;
                };
                let environments = self
                    .environments
                    .keys()
                    .filter_map(|environment| {
                        let target = self.targets.get(self.target_of(environment))?;
                        let provider = target.providers.get(name)?;
                        Some((environment.clone(), provider.address.clone()))
                    })
                    .collect();
                Some((
                    name.clone(),
                    DeclaredProviderAddress {
                        default: address.clone(),
                        environments,
                    },
                ))
            })
            .collect()
    }
}
