use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A certificate the operator provides for an environment's domains, such
/// as a Cloudflare origin certificate, where ACME challenges cannot reach
/// the ingress. Both secrets hold a PEM file encoded as base64 on one line
/// (`base64 -w0 cert.pem | mm secret set NAME --stdin`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(deny_unknown_fields)]
pub struct EnvironmentTlsConfig {
    /// Secret holding the certificate chain.
    pub certificate: String,
    /// Secret holding the private key.
    pub key: String,
}
