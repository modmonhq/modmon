//! The token of an environment's Cloudflare Tunnel, read from its secret.

use base64::Engine;

use super::*;

/// A tunnel's token as the connector reads it, never logged, and the
/// tunnel it belongs to.
pub struct TunnelToken {
    pub token: Zeroizing<Vec<u8>>,
    pub tunnel: String,
}

impl EnvironmentManager {
    /// The secret that holds the selected environment's tunnel token and
    /// the connector's image, when the environment uses a tunnel.
    pub fn environment_tunnel(
        &self,
        selection: &WorkspaceSelection,
    ) -> Result<Option<(String, String)>, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(document
            .config
            .environments
            .get(&environment)
            .and_then(|config| config.ingress.as_ref()?.tunnel()))
    }

    /// The name an ingress node keeps the selected environment's routes
    /// under.
    pub fn ingress_table(&self, selection: &WorkspaceSelection) -> Result<String, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        Ok(crate::domain::cluster_ingress::table_name(
            document.config.project.id,
            &environment,
        ))
    }

    /// The token in the secret `name`, checked to be a Cloudflare Tunnel
    /// token.
    pub async fn tunnel_token(
        &self,
        selection: &WorkspaceSelection,
        name: &str,
    ) -> Result<TunnelToken, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let missing = |detail: String| MmError::Remote {
            code: "MM_TUNNEL_TOKEN_MISSING",
            message: format!(
                "{detail}; store the tunnel's token with `cloudflared tunnel token <tunnel> | mm -e {environment} secret set {name} --stdin`, then plan again"
            ),
        };
        let declaration = document.config.secrets.get(name).ok_or_else(|| {
            missing(format!(
                "ingress.cloudflared.token names secret '{name}', which secrets: does not declare"
            ))
        })?;
        let reference = declaration
            .storage
            .reference()
            .map_err(|message| MmError::Secret { message })?;
        let material = self
            .resolver
            .resolve(&SecretScope::new(&environment), &reference)
            .await?
            .material
            .ok_or_else(|| {
                missing(format!(
                    "the tunnel token secret '{name}' has no value in {environment} on this node"
                ))
            })?;
        let token = material.expose(|value| Zeroizing::new(value.trim().as_bytes().to_vec()));
        let tunnel = base64::engine::general_purpose::STANDARD
            .decode(token.as_slice())
            .map(Zeroizing::new)
            .map_err(|_| "it is not base64".to_owned())
            .and_then(|decoded| crate::domain::cluster_ingress::token_tunnel(&decoded))
            .map_err(|problem| {
                missing(format!(
                    "secret '{name}' does not hold a Cloudflare Tunnel token: {problem}"
                ))
            })?;
        Ok(TunnelToken { token, tunnel })
    }
}
