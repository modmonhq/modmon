//! Private image registries: credentials deploys pull with.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A registry, keyed by its host (`ghcr.io`, `registry.example.com:5000`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RegistryConfig {
    /// The account the token belongs to. GHCR accepts any name with a token.
    pub username: String,
    /// The secret holding the password or access token; set it per environment.
    pub password: RegistryPassword,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct RegistryPassword {
    pub secret: String,
}

/// Problems with a registry host as a key under `registries:`.
pub fn validate_registry_host(host: &str) -> Result<(), String> {
    let (name, port) = host
        .rsplit_once(':')
        .map_or((host, None), |(name, port)| (name, Some(port)));
    let name_ok = !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        });
    let port_ok = port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port > 0));
    if name_ok && port_ok {
        Ok(())
    } else {
        Err(format!(
            "registry '{host}' must be a lowercase host name with an optional port, such as ghcr.io"
        ))
    }
}

/// How to get a pull token for well-known registries, with a pre-filled link.
#[must_use]
pub fn registry_token_hint(host: &str, project: &str, environment: &str) -> Option<String> {
    let label = format!("mm {project} {environment}");
    let encoded = label.replace(' ', "%20");
    match host {
        "ghcr.io" => Some(format!(
            "create a classic personal access token with only the read:packages scope (GHCR accepts no other kind): https://github.com/settings/tokens/new?scopes=read:packages&description={encoded}%20pull ; choose \"No expiration\" (or the longest your organization allows), and authorize it for SSO if the organization requires it"
        )),
        "docker.io" | "registry-1.docker.io" | "index.docker.io" => Some(
            "create a read-only personal access token: https://app.docker.com/settings/personal-access-tokens/create ; choose no expiration"
                .to_owned(),
        ),
        "registry.gitlab.com" => Some(format!(
            "create a personal or project access token with only the read_registry scope: https://gitlab.com/-/user_settings/personal_access_tokens?name={encoded}%20pull&scopes=read_registry"
        )),
        _ => None,
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
