use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::target::TargetId;

pub const TRAEFIK_CERTIFICATE_RESOLVER: &str = "mm-http";
pub const TRAEFIK_LOCAL_CA_DIRECTORY: &str = "/var/lib/traefik-local-ca";
pub const TRAEFIK_DYNAMIC_CONFIG_DIRECTORY: &str = "/etc/traefik/dynamic";
pub const TRAEFIK_ACME_STORAGE_PATH: &str = "/var/lib/traefik/acme.json";
pub const TRAEFIK_ACME_FILE_MODE: &str = "0600";
pub const TARGET_INGRESS_COMPOSE_PATH: &str = "%h/.local/share/mm/ingress/current/compose.yaml";
pub const INGRESS_PUBLIC_HTTP_PORT: u16 = 80;
pub const INGRESS_PUBLIC_HTTPS_PORT: u16 = 443;
/// launchd job labels are fixed so mm only ever controls jobs it owns.
pub const LAUNCHD_INGRESS_LABEL: &str = "local.mm.ingress";
pub const LAUNCHD_FORWARDER_LABEL: &str = "local.mm.ingress-forwarder";
pub const LAUNCHD_DASHBOARD_LABEL: &str = "local.mm.dashboard";
/// macOS locations relative to the user's home directory.
pub const MACOS_INGRESS_DIRECTORY: &str = "Library/Application Support/mm/ingress";
pub const MACOS_LAUNCH_AGENTS_DIRECTORY: &str = "Library/LaunchAgents";
pub const MACOS_LOG_DIRECTORY: &str = "Library/Logs/mm";
/// macOS ingress files, relative to `MACOS_INGRESS_DIRECTORY`, and artifact names.
pub const MACOS_COMPOSE_FILE: &str = "current/compose.yaml";
pub const MACOS_HAPROXY_FILE: &str = "haproxy.cfg";
pub const MACOS_AUTOSTART_SUPPORT_FILE: &str = "local.mm.ingress.plist";
pub const MACOS_FORWARDER_SUPPORT_FILE: &str = "local.mm.ingress-forwarder.plist";
const INGRESS_LOOPBACK_PORT_START: u16 = 20_000;
const INGRESS_LOOPBACK_PORT_TRIPLE_COUNT: u16 = 20_000 / 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetIngressStep {
    CreateExternalNetwork,
    PreparePrivateIngressState,
    ApplyIngressStack,
    VerifyLoopbackIngress,
    InstallRootlessAutostart,
    InstallPrivilegedPortForwarder,
    InstallLaunchdAutostart,
    InstallLaunchdForwarder,
}

impl TargetIngressStep {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::CreateExternalNetwork => "create-external-network",
            Self::PreparePrivateIngressState => "prepare-private-ingress-state",
            Self::ApplyIngressStack => "apply-ingress-stack",
            Self::VerifyLoopbackIngress => "verify-loopback-ingress",
            Self::InstallRootlessAutostart => "install-rootless-autostart",
            Self::InstallPrivilegedPortForwarder => "install-privileged-port-forwarder",
            Self::InstallLaunchdAutostart => "install-launchd-autostart",
            Self::InstallLaunchdForwarder => "install-launchd-forwarder",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetIngressArtifactManifest {
    pub target_id: TargetId,
    pub target_name: String,
    pub target_fingerprint: String,
    pub provider: String,
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acme_email: Option<String>,
    pub compose_project_name: String,
    pub network_name: String,
    pub acme_volume_name: String,
    pub dynamic_config_volume_name: String,
    pub dynamic_config_directory: String,
    pub acme_storage_path: String,
    pub acme_file_mode: String,
    pub public_http_port: u16,
    pub public_https_port: u16,
    pub proxy_http_port: u16,
    pub proxy_https_port: u16,
    pub proxy_verify_port: u16,
    pub certificate_resolver: String,
    pub target_compose_path: String,
    pub compose_digest: String,
    pub support_files: Vec<TargetIngressSupportFileManifest>,
    pub steps: Vec<TargetIngressStep>,
    /// Absent for Linux/systemd hosts, which keeps their reviewed hashes stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos_host: Option<MacosIngressHost>,
    pub artifact_hash: String,
}

impl TargetIngressArtifactManifest {
    pub fn canonical_hash(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hash_field(&mut hasher, "mm-target-ingress-artifact");
        hash_field(&mut hasher, &self.target_id.0.to_string());
        hash_field(&mut hasher, &self.target_name);
        hash_field(&mut hasher, &self.target_fingerprint);
        hash_field(&mut hasher, &self.provider);
        hash_field(&mut hasher, &self.image);
        match &self.acme_email {
            Some(acme_email) => {
                hash_field(&mut hasher, "acme-email");
                hash_field(&mut hasher, acme_email);
            }
            None => hash_field(&mut hasher, "no-acme-email"),
        }
        hash_field(&mut hasher, &self.compose_project_name);
        hash_field(&mut hasher, &self.network_name);
        hash_field(&mut hasher, &self.acme_volume_name);
        hash_field(&mut hasher, &self.dynamic_config_volume_name);
        hash_field(&mut hasher, &self.dynamic_config_directory);
        hash_field(&mut hasher, &self.acme_storage_path);
        hash_field(&mut hasher, &self.acme_file_mode);
        hash_field(&mut hasher, &self.public_http_port.to_string());
        hash_field(&mut hasher, &self.public_https_port.to_string());
        hash_field(&mut hasher, &self.proxy_http_port.to_string());
        hash_field(&mut hasher, &self.proxy_https_port.to_string());
        hash_field(&mut hasher, &self.proxy_verify_port.to_string());
        hash_field(&mut hasher, &self.certificate_resolver);
        hash_field(&mut hasher, &self.target_compose_path);
        hash_field(&mut hasher, &self.compose_digest);
        hash_count(&mut hasher, self.support_files.len());
        for file in &self.support_files {
            hash_field(&mut hasher, &file.name);
            hash_field(&mut hasher, &file.destination);
            hash_field(&mut hasher, &file.mode);
            hash_field(&mut hasher, &file.digest);
        }
        hash_count(&mut hasher, self.steps.len());
        for step in &self.steps {
            hash_field(&mut hasher, step.label());
        }
        if let Some(host) = &self.macos_host {
            hash_field(&mut hasher, "macos-launchd");
            hash_field(&mut hasher, &host.haproxy_executable);
            hash_field(&mut hasher, &host.docker_executable);
            hash_field(&mut hasher, &host.docker_host);
            hash_field(&mut hasher, &host.home);
            hash_field(&mut hasher, &host.network_subnet);
            hash_field(&mut hasher, &host.network_gateway);
        }
        hasher.finalize().to_hex().to_string()
    }
}

/// How the host that runs a target's ingress integrates with its service manager.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum IngressHostProfile {
    /// systemd user and system units. Used for Linux hosts and every remote target.
    #[default]
    LinuxSystemd,
    /// Docker Desktop with launchd user agents. Only the local target on macOS.
    MacosLaunchd(MacosHostFacts),
    /// A macOS host whose prerequisites are missing; planning local ingress reports why.
    Unavailable { reason: String },
}

/// What a Mac contributes to its ingress plan, probed once per command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacosHostFacts {
    pub haproxy_executable: String,
    pub docker_executable: String,
    /// The Docker Desktop endpoint, for example `unix://$HOME/.docker/run/docker.sock`.
    pub docker_host: String,
    pub home: String,
}

/// Host facts that launchd jobs need explicitly: launchd provides no login shell
/// `PATH` or Docker context, so the reviewed artifact pins them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MacosIngressHost {
    pub haproxy_executable: String,
    pub docker_executable: String,
    /// The Docker Desktop endpoint, for example `unix://$HOME/.docker/run/docker.sock`.
    pub docker_host: String,
    pub home: String,
    pub network_subnet: String,
    pub network_gateway: String,
}

impl MacosIngressHost {
    #[must_use]
    pub fn for_target(facts: MacosHostFacts, target_id: TargetId) -> Self {
        let (network_subnet, network_gateway) = ingress_network_subnet(target_id);
        Self {
            haproxy_executable: facts.haproxy_executable,
            docker_executable: facts.docker_executable,
            docker_host: facts.docker_host,
            home: facts.home,
            network_subnet,
            network_gateway,
        }
    }

    /// Docker Desktop forwards loopback-published ports from the ingress network
    /// gateway, so that single address is the PROXY protocol peer Traefik trusts.
    #[must_use]
    pub fn trusted_proxy_peer(&self) -> String {
        format!("{}/32", self.network_gateway)
    }

    #[must_use]
    pub fn ingress_path(&self, name: &str) -> String {
        format!("{}/{MACOS_INGRESS_DIRECTORY}/{name}", self.home)
    }

    #[must_use]
    pub fn launch_agent_path(&self, label: &str) -> String {
        format!(
            "{}/{MACOS_LAUNCH_AGENTS_DIRECTORY}/{label}.plist",
            self.home
        )
    }

    #[must_use]
    pub fn log_path(&self, name: &str) -> String {
        format!("{}/{MACOS_LOG_DIRECTORY}/{name}", self.home)
    }
}

const INGRESS_SUBNET_SECOND_OCTET: u8 = 252;

/// A fixed /24 per target inside 10.252.0.0/16. Runtime network creation checks it
/// against existing Docker networks; the gateway is always the first host address.
#[must_use]
pub fn ingress_network_subnet(target_id: TargetId) -> (String, String) {
    let digest = blake3::hash(target_id.0.as_bytes());
    let third = digest.as_bytes()[2];
    (
        format!("10.{INGRESS_SUBNET_SECOND_OCTET}.{third}.0/24"),
        format!("10.{INGRESS_SUBNET_SECOND_OCTET}.{third}.1"),
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetIngressSupportFileManifest {
    pub name: String,
    pub destination: String,
    pub mode: String,
    pub digest: String,
}

pub struct CompiledTargetIngressArtifact {
    pub manifest: TargetIngressArtifactManifest,
    pub compose: Vec<u8>,
    pub support_files: Vec<CompiledTargetIngressSupportFile>,
}

pub struct CompiledTargetIngressSupportFile {
    pub manifest: TargetIngressSupportFileManifest,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for CompiledTargetIngressArtifact {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CompiledTargetIngressArtifact")
            .field("manifest", &self.manifest)
            .field("compose_bytes", &self.compose.len())
            .field("support_file_count", &self.support_files.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PreparedTargetIngressArtifact {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub compose_path: PathBuf,
    pub support_paths: Vec<PathBuf>,
    pub manifest: TargetIngressArtifactManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TargetIngressPlan {
    pub target_name: String,
    pub target_id: TargetId,
    pub plan_hash: String,
    pub artifact: PreparedTargetIngressArtifact,
}

#[must_use]
pub fn ingress_network_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-ingress-{}", &digest[..20])
}

#[must_use]
pub fn ingress_project_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-ingress-{}", &digest[..20])
}

#[must_use]
pub fn ingress_acme_volume_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-ingress-acme-{}", &digest[..20])
}

#[must_use]
pub fn ingress_dynamic_config_volume_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-ingress-config-{}", &digest[..20])
}

/// Whether two IPv4 CIDR blocks share any address; `None` when either is not IPv4 CIDR.
#[must_use]
pub fn ipv4_subnets_overlap(left: &str, right: &str) -> Option<bool> {
    let (left_start, left_end) = ipv4_range(left)?;
    let (right_start, right_end) = ipv4_range(right)?;
    Some(left_start <= right_end && right_start <= left_end)
}

fn ipv4_range(cidr: &str) -> Option<(u32, u32)> {
    let (address, prefix) = cidr.split_once('/')?;
    let address = u32::from(address.parse::<std::net::Ipv4Addr>().ok()?);
    let prefix = prefix.parse::<u32>().ok().filter(|prefix| *prefix <= 32)?;
    let host_mask = u32::MAX.checked_shr(prefix).unwrap_or(0);
    Some((address & !host_mask, address | host_mask))
}

#[must_use]
pub fn ingress_loopback_ports(target_id: TargetId) -> (u16, u16, u16) {
    let digest = blake3::hash(target_id.0.as_bytes());
    let value = u16::from_le_bytes([digest.as_bytes()[0], digest.as_bytes()[1]]);
    let http = INGRESS_LOOPBACK_PORT_START + (value % INGRESS_LOOPBACK_PORT_TRIPLE_COUNT) * 3;
    (http, http + 1, http + 2)
}

#[must_use]
pub fn ingress_proxy_ports(target_id: TargetId) -> (u16, u16) {
    let (http, https, _) = ingress_loopback_ports(target_id);
    (http, https)
}

#[must_use]
pub fn ingress_verify_port(target_id: TargetId) -> u16 {
    ingress_loopback_ports(target_id).2
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_count(hasher: &mut blake3::Hasher, value: usize) {
    hasher.update(&(value as u64).to_le_bytes());
}

#[cfg(test)]
mod tests;
