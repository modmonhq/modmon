use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum IngressRouteProvenance {
    #[default]
    ExplicitConfig,
    AutomaticConvention,
    MmManagedService,
}

impl fmt::Display for IngressRouteProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExplicitConfig => "explicit config",
            Self::AutomaticConvention => "automatic convention",
            Self::MmManagedService => "mm-managed service",
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngressCertificateReference {
    #[default]
    None,
    LocalCa {
        host: String,
    },
    Acme {
        resolver: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IngressRouteOwnership {
    Project {
        project_id: ProjectId,
        environment: String,
        stack: String,
        service: String,
    },
    MmManaged {
        component: String,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum HttpEvidenceSource {
    GuidedPort,
    ComposeExpose,
    ComposePublishedPort,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum TransportProtocol {
    Tcp,
    Udp,
    Sctp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HttpPortCandidate {
    pub port: u16,
    pub protocol: TransportProtocol,
    pub sources: Vec<HttpEvidenceSource>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceHttpEvidence {
    pub declared_in_compose: bool,
    pub candidates: Vec<HttpPortCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IngressServiceStatus {
    pub module: String,
    pub stack: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<IngressRouteProvenance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_reference: Option<IngressCertificateReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<IngressRouteOwnership>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CanonicalMiddleware {
    ReplacePathRegex {
        regex: String,
        replacement: String,
    },
    ForwardAuth {
        address: String,
        trust_forward_header: bool,
        auth_response_headers: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        add_auth_cookies_to_response: Vec<String>,
    },
}

impl CanonicalMiddleware {
    pub fn digest(&self) -> Result<String, String> {
        let encoded = serde_json::to_vec(self)
            .map_err(|error| format!("could not canonicalize ingress middleware: {error}"))?;
        Ok(blake3::hash(&encoded).to_hex().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IngressRouteClaim {
    pub target: String,
    pub target_id: TargetId,
    pub project_id: ProjectId,
    pub project_name: String,
    pub environment: String,
    pub stack: String,
    pub route: String,
    pub host: String,
    pub path_prefix: String,
    pub service: String,
    pub port: u16,
    pub tls: bool,
    pub provenance: IngressRouteProvenance,
    pub certificate_reference: IngressCertificateReference,
    pub ownership: IngressRouteOwnership,
    pub priority: i64,
    pub router_name: String,
    pub backend_alias: String,
    pub artifact_hash: String,
    pub middlewares: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IngressRouteSet {
    pub target_id: TargetId,
    pub generation: u64,
    pub config_digest: Option<String>,
    pub claims: Vec<IngressRouteClaim>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub middlewares: BTreeMap<String, CanonicalMiddleware>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IngressRouteConflict {
    pub proposed: IngressRouteClaim,
    pub existing: IngressRouteClaim,
}

#[cfg(test)]
mod tests;
