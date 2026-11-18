//! An environment's `ingress:`: how its routes come in on a cluster target.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::cluster_ingress::{Balance, DEFAULT_CLOUDFLARED_IMAGE};

/// How an environment's ingress takes requests on a cluster target. In
/// YAML: `balance`, `type`, `cloudflared`, and `headers`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "EnvironmentIngressFields",
    into = "EnvironmentIngressFields"
)]
pub struct EnvironmentIngressConfig {
    pub balance: Option<Balance>,
    pub kind: IngressType,
    pub cloudflared: Option<CloudflaredConfig>,
    pub headers: BTreeMap<String, String>,
}

impl EnvironmentIngressConfig {
    /// The secret that holds the tunnel's token and the connector's image,
    /// when the environment comes in through a Cloudflare Tunnel.
    #[must_use]
    pub fn tunnel(&self) -> Option<(String, String)> {
        let cloudflared = self.cloudflared.as_ref()?;
        (self.kind == IngressType::Cloudflared).then(|| {
            (
                cloudflared.token.clone().unwrap_or_default(),
                cloudflared
                    .image
                    .clone()
                    .unwrap_or_else(|| DEFAULT_CLOUDFLARED_IMAGE.to_owned()),
            )
        })
    }
}

/// Where an environment's requests come in on a cluster target.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum IngressType {
    /// Ports 80 and 443 of every ingress node.
    #[default]
    Public,
    /// A Cloudflare Tunnel, with no port open on any node.
    Cloudflared,
}

impl IngressType {
    #[must_use]
    pub const fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }
}

/// The Cloudflare Tunnel of an environment with `type: cloudflared`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct CloudflaredConfig {
    /// The secret that holds the tunnel's token, as
    /// `cloudflared tunnel token <name>` prints it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The connector's image. The default is a cloudflared release mm
    /// pins by digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// The YAML shape of [`EnvironmentIngressConfig`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentIngressFields {
    /// `local-first` (the default) sends each request to the service's
    /// containers on the ingress node that took it, and to every node's
    /// only when that node runs none; `spread` sends it to the service's
    /// containers on every node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    balance: Option<Balance>,
    /// `public` (the default) takes requests on ports 80 and 443 of the
    /// ingress nodes; `cloudflared` through a Cloudflare Tunnel, with no
    /// port open. Local development ignores it.
    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    kind: Option<IngressType>,
    /// The tunnel, with `type: cloudflared`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cloudflared: Option<CloudflaredConfig>,
    /// Response headers every route of the environment sets on a cluster
    /// target, such as `X-Robots-Tag: noindex`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    headers: BTreeMap<String, String>,
}

impl TryFrom<EnvironmentIngressFields> for EnvironmentIngressConfig {
    type Error = String;

    fn try_from(fields: EnvironmentIngressFields) -> Result<Self, Self::Error> {
        let kind = fields.kind.unwrap_or_default();
        let token = fields
            .cloudflared
            .as_ref()
            .and_then(|cloudflared| cloudflared.token.as_deref());
        match (kind, token, &fields.cloudflared) {
            (IngressType::Cloudflared, None | Some(""), _) => Err(
                "type: cloudflared needs cloudflared.token, the name of a secret that holds the tunnel's token: write `cloudflared: {token: cloudflare.tunnel-token}`, declare that secret under secrets:, and store the token with `mm secret set cloudflare.tunnel-token --stdin`"
                    .to_owned(),
            ),
            (IngressType::Public, _, Some(_)) => Err(
                "cloudflared: applies with type: cloudflared; add `type: cloudflared` next to it, or remove it".to_owned(),
            ),
            _ => Ok(Self {
                balance: fields.balance,
                kind,
                cloudflared: fields.cloudflared,
                headers: fields.headers,
            }),
        }
    }
}

impl From<EnvironmentIngressConfig> for EnvironmentIngressFields {
    fn from(config: EnvironmentIngressConfig) -> Self {
        Self {
            balance: config.balance,
            kind: (!config.kind.is_public()).then_some(config.kind),
            cloudflared: config.cloudflared,
            headers: config.headers,
        }
    }
}

impl JsonSchema for EnvironmentIngressConfig {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EnvironmentIngressConfig".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        EnvironmentIngressFields::json_schema(generator)
    }
}
