use std::collections::BTreeMap;

use base64::Engine;
use zeroize::Zeroizing;

/// One registry login, resolved from a secret for a single operation.
#[derive(Clone, PartialEq, Eq)]
pub struct RegistryCredential {
    pub registry: String,
    pub username: String,
    pub password: Zeroizing<String>,
}

impl std::fmt::Debug for RegistryCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RegistryCredential")
            .field("registry", &self.registry)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// A Docker client `config.json` holding these logins, which Docker, the
/// Compose plugin, and Podman (`REGISTRY_AUTH_FILE`) all read.
#[must_use]
pub fn registry_auth_config(credentials: &[RegistryCredential]) -> Zeroizing<String> {
    let auths = credentials
        .iter()
        .map(|credential| {
            let pair = Zeroizing::new(format!(
                "{}:{}",
                credential.username,
                credential.password.as_str()
            ));
            (
                credential.registry.clone(),
                serde_json::json!({
                    "auth": base64::engine::general_purpose::STANDARD.encode(pair.as_bytes())
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Zeroizing::new(serde_json::json!({ "auths": auths }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_holds_one_auth_per_registry_and_debug_hides_the_password() {
        let credential = RegistryCredential {
            registry: "ghcr.io".to_owned(),
            username: "bot".to_owned(),
            password: Zeroizing::new("token".to_owned()),
        };
        let config = registry_auth_config(std::slice::from_ref(&credential));
        assert_eq!(
            config.as_str(),
            r#"{"auths":{"ghcr.io":{"auth":"Ym90OnRva2Vu"}}}"#
        );
        assert!(!format!("{credential:?}").contains("token"));
    }
}
