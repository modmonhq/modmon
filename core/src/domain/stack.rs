use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::build::{is_image_repository, is_sha256_digest};
use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ComposeSourceManifest {
    pub path: PathBuf,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExcludedService {
    pub module: String,
    pub stack: String,
    pub service: String,
    /// Set when `development:` keeps the service off on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_locally: Option<crate::domain::development_units::OffLocally>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackServiceManifest {
    pub module: String,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<crate::domain::config::ServiceRole>,
    #[serde(default)]
    pub http_evidence: crate::domain::ingress::ServiceHttpEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development_mount: Option<crate::domain::development_mount::DevelopmentMountConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mail_bindings: Vec<crate::domain::development_mail::MailBindingManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackRouteManifest {
    pub name: String,
    pub host: String,
    pub path_prefix: String,
    pub service: String,
    pub port: u16,
    pub tls: bool,
    pub provenance: crate::domain::ingress::IngressRouteProvenance,
    pub certificate_reference: crate::domain::ingress::IngressCertificateReference,
    pub ownership: crate::domain::ingress::IngressRouteOwnership,
    pub priority: i64,
    pub router_name: String,
    pub backend_alias: String,
    pub middlewares: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "crate::domain::cluster_ingress::Balance::is_local_first"
    )]
    pub balance: crate::domain::cluster_ingress::Balance,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<crate::domain::cluster_ingress::AllowRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackMiddlewareInput {
    pub bundle: String,
    pub path: PathBuf,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackArtifactManifest {
    pub project_id: ProjectId,
    pub project_name: String,
    pub target: String,
    pub target_id: TargetId,
    pub environment: String,
    pub stack: String,
    pub compose_project_name: String,
    pub user_files: Vec<ComposeSourceManifest>,
    pub generated_overlay_digest: String,
    pub services: Vec<StackServiceManifest>,
    pub routes: Vec<StackRouteManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub middleware_inputs: Vec<StackMiddlewareInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved_volumes: Vec<crate::domain::development_mount::PreservedVolumeManifest>,
    /// The security options of each service that is hardened or runs under
    /// a named OCI runtime. Empty when neither is used.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<crate::domain::hardening::ServiceSecurity>,
    /// The services the overlay lets open ports below 1024 as any user, as
    /// Docker Engine does: on a host that runs Podman.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub low_ports: Vec<String>,
    /// The services that run in the host's network namespace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub host_network: Vec<HostNetworkUse>,
    /// The `excludes` and `protected` paths of the development mounts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masked_paths: Vec<crate::domain::development_mount::MaskedPath>,
    pub artifact_hash: String,
}

/// A service that runs in its host's network namespace (`network_mode:
/// host`), and why, when the project says.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct HostNetworkUse {
    pub service: String,
    /// The service's `host-network` in `mm.yaml`. A remote host may allow
    /// the service without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl HostNetworkUse {
    /// The line a plan shows for the service.
    #[must_use]
    pub fn line(&self) -> String {
        match &self.reason {
            Some(reason) => format!("{} uses the host's network: {reason}", self.service),
            None => format!(
                "{} uses the host's network, which the host's policy allows (network-host)",
                self.service
            ),
        }
    }
}

impl StackServiceManifest {
    /// Whether the service exists only for its jobs (`role: job`).
    #[must_use]
    pub fn is_job_only(&self) -> bool {
        self.role == Some(crate::domain::config::ServiceRole::Job)
    }
}

impl StackArtifactManifest {
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut hasher = blake3::Hasher::new();
        hash_field(&mut hasher, "compose-stack-artifact");
        hash_field(&mut hasher, &self.project_id.0.to_string());
        hash_field(&mut hasher, &self.project_name);
        hash_field(&mut hasher, &self.target);
        hash_field(&mut hasher, &self.target_id.0.to_string());
        hash_field(&mut hasher, &self.environment);
        hash_field(&mut hasher, &self.stack);
        hash_field(&mut hasher, &self.compose_project_name);
        hash_count(&mut hasher, self.user_files.len());
        for file in &self.user_files {
            let path = file
                .path
                .to_str()
                .ok_or_else(|| "Compose file paths must be valid Unicode".to_owned())?;
            hash_field(&mut hasher, path);
            hash_field(&mut hasher, &file.digest);
        }
        hash_field(&mut hasher, &self.generated_overlay_digest);
        hash_count(&mut hasher, self.services.len());
        for service in &self.services {
            hash_field(&mut hasher, &service.module);
            hash_field(&mut hasher, &service.service);
            match &service.image {
                Some(image) => {
                    validate_digest_image(image)?;
                    hash_field(&mut hasher, "immutable-image");
                    hash_field(&mut hasher, image);
                }
                None => hash_field(&mut hasher, "configured-image"),
            }
            if let Some(mount) = &service.development_mount {
                hash_field(&mut hasher, "development-mount");
                hash_field(
                    &mut hasher,
                    &serde_json::to_string(mount)
                        .map_err(|error| format!("could not hash development mount: {error}"))?,
                );
            }
            for binding in &service.mail_bindings {
                hash_field(&mut hasher, &binding.key);
                hash_field(&mut hasher, binding.value.value());
                hash_field(
                    &mut hasher,
                    match binding.provenance {
                        crate::domain::development_mail::MailBindingProvenance::InjectedDefault => {
                            "injected-default"
                        }
                        crate::domain::development_mail::MailBindingProvenance::InjectedConfigured => {
                            "injected-configured"
                        }
                        crate::domain::development_mail::MailBindingProvenance::PreservedProjectValue => {
                            "preserved-project-value"
                        }
                        crate::domain::development_mail::MailBindingProvenance::PreservedComposeValue => {
                            "preserved-compose-value"
                        }
                    },
                );
            }
        }
        hash_count(&mut hasher, self.routes.len());
        for route in &self.routes {
            hash_field(&mut hasher, &route.name);
            hash_field(&mut hasher, &route.host);
            hash_field(&mut hasher, &route.path_prefix);
            hash_field(&mut hasher, &route.service);
            hash_field(&mut hasher, &route.port.to_string());
            hash_field(&mut hasher, if route.tls { "tls" } else { "plain" });
            hash_field(&mut hasher, &route.priority.to_string());
            hash_field(&mut hasher, &route.router_name);
            hash_field(&mut hasher, &route.backend_alias);
            hash_count(&mut hasher, route.middlewares.len());
            for middleware in &route.middlewares {
                hash_field(&mut hasher, middleware);
            }
        }
        hash_count(&mut hasher, self.middleware_inputs.len());
        for input in &self.middleware_inputs {
            let path = input
                .path
                .to_str()
                .ok_or_else(|| "middleware file paths must be valid Unicode".to_owned())?;
            hash_field(&mut hasher, &input.bundle);
            hash_field(&mut hasher, path);
            hash_field(&mut hasher, &input.digest);
        }
        self.hash_later_parts(&mut hasher)?;
        Ok(hasher.finalize().to_hex().to_string())
    }

    /// What later versions added to a manifest. Each part is hashed only
    /// when it is there, so a manifest without it keeps its hash.
    fn hash_later_parts(&self, hasher: &mut blake3::Hasher) -> Result<(), String> {
        if !self.preserved_volumes.is_empty() {
            hash_count(hasher, self.preserved_volumes.len());
            for volume in &self.preserved_volumes {
                hash_field(hasher, &volume.module);
                hash_field(hasher, &volume.container_path);
                hash_field(hasher, &volume.logical_name);
                hash_field(hasher, &volume.physical_name);
                hash_field(hasher, &volume.resource);
            }
        }
        let spread = self
            .routes
            .iter()
            .filter(|route| !route.balance.is_local_first())
            .collect::<Vec<_>>();
        if !spread.is_empty() {
            hash_field(hasher, "spread-routes-v1");
            hash_count(hasher, spread.len());
            for route in spread {
                hash_field(hasher, &route.router_name);
            }
        }
        let ruled = self
            .routes
            .iter()
            .filter(|route| !route.headers.is_empty() || !route.allow.is_empty())
            .collect::<Vec<_>>();
        if !ruled.is_empty() {
            hash_field(hasher, "route-rules-v1");
            hash_count(hasher, ruled.len());
            for route in ruled {
                hash_field(hasher, &route.router_name);
                hash_field(
                    hasher,
                    &serde_json::to_string(&(&route.headers, &route.allow))
                        .map_err(|error| format!("could not hash route rules: {error}"))?,
                );
            }
        }
        if !self.security.is_empty() {
            hash_field(hasher, "service-security-v1");
            hash_field(
                hasher,
                &serde_json::to_string(&self.security)
                    .map_err(|error| format!("could not hash security options: {error}"))?,
            );
        }
        if !self.host_network.is_empty() {
            hash_field(hasher, "host-network-v1");
            hash_count(hasher, self.host_network.len());
            for service in &self.host_network {
                hash_field(hasher, &service.service);
                hash_field(hasher, service.reason.as_deref().unwrap_or_default());
            }
        }
        if !self.masked_paths.is_empty() {
            hash_field(hasher, "masked-paths-v1");
            hash_field(
                hasher,
                &serde_json::to_string(&self.masked_paths)
                    .map_err(|error| format!("could not hash masked paths: {error}"))?,
            );
        }
        Ok(())
    }

    /// Whether an empty file covers one of the masked paths.
    #[must_use]
    pub fn masks_a_file(&self) -> bool {
        self.masked_paths
            .iter()
            .any(|path| path.kind == crate::domain::development_mount::MaskedPathKind::File)
    }
}

fn validate_digest_image(image: &str) -> Result<(), String> {
    if crate::domain::build::is_local_image_pin(image) {
        return Ok(());
    }
    let Some((repository, digest)) = image.rsplit_once('@') else {
        return Err("deployment image is not pinned by a sha256 digest".to_owned());
    };
    if !is_image_repository(repository) || !is_sha256_digest(digest) {
        return Err("deployment image has an invalid OCI repository or digest".to_owned());
    }
    Ok(())
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_count(hasher: &mut blake3::Hasher, value: usize) {
    hasher.update(&(value as u64).to_le_bytes());
}

pub struct CompiledStackArtifact {
    pub manifest: StackArtifactManifest,
    pub user_files: Vec<CompiledComposeSource>,
    pub generated_overlay: Vec<u8>,
    /// The config file versions the overlay mounts. Publishing the
    /// artifact puts each one at its path.
    pub config_files: Vec<CompiledConfigFile>,
}

/// One version of a config file: its bytes and where the host keeps them.
#[derive(Clone, PartialEq, Eq)]
pub struct CompiledConfigFile {
    /// The file on the host, named after `version`.
    pub path: PathBuf,
    pub version: String,
    pub bytes: Vec<u8>,
    pub mode: u32,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

impl std::fmt::Debug for CompiledConfigFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledConfigFile")
            .field("path", &self.path)
            .field("byte_count", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

pub struct CompiledComposeSource {
    pub manifest: ComposeSourceManifest,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for CompiledComposeSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledComposeSource")
            .field("manifest", &self.manifest)
            .field("byte_count", &self.bytes.len())
            .finish()
    }
}

impl std::fmt::Debug for CompiledStackArtifact {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledStackArtifact")
            .field("manifest", &self.manifest)
            .field("user_files", &self.user_files)
            .field("generated_overlay_bytes", &self.generated_overlay.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PreparedStackArtifact {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub generated_overlay_path: PathBuf,
    pub compose_files: Vec<PathBuf>,
    pub manifest: StackArtifactManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CanonicalNetworkMember {
    pub stack: String,
    pub service: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CanonicalNetworkIntent {
    pub logical_name: String,
    pub physical_name: String,
    /// `internal: true` in `mm.yaml`: no outbound access.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
    pub members: Vec<CanonicalNetworkMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StackCompilationReport {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_services: Vec<ExcludedService>,
    pub project_name: String,
    pub target: String,
    pub target_id: TargetId,
    pub environment: String,
    pub stacks: Vec<PreparedStackArtifact>,
    pub route_claims: Vec<crate::domain::ingress::IngressRouteClaim>,
    pub ingress: Vec<crate::domain::ingress::IngressServiceStatus>,
}

#[cfg(test)]
mod tests;
