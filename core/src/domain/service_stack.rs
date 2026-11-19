use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::diagnostic::Diagnostic;
use crate::domain::runtime::RuntimeKind;
use crate::domain::target::TargetId;
use crate::domain::target_ingress::PreparedTargetIngressArtifact;

pub const SERVICE_STACK_SCHEMA: u32 = 1;
pub const SERVICE_STACK_VERSION: &str = "1.0.0";
pub const MAILPIT_VERSION: &str = "1.27.8";
pub const MAILPIT_IMAGE: &str = "docker.io/axllent/mailpit@sha256:6abc8e633df15eaf785cfcf38bae48e66f64beecdc03121e249d0f9ec15f0707";
pub const MAILPIT_HOST: &str = "mail.mm.test";
pub const MAILPIT_SMTP_PORT: u16 = 1025;
pub const MAILPIT_UI_PORT: u16 = 8025;
pub const DASHBOARD_HOST: &str = "mm.test";
pub const DASHBOARD_BACKEND_PORT: u16 = 42_831;
pub const CAP_VERSION: &str = "3.1.9";
pub const CAP_IMAGE: &str =
    "docker.io/tiago2/cap@sha256:98fdaaf4ffe6bea18549484a431c601399f24dd561370e196970e6fc0e6b79d0";
pub const CAP_HOST: &str = "cap.mm.test";
pub const CAP_PORT: u16 = 3000;
pub const CAP_WIDGET_VERSION: &str = "0.1.57";
pub const CAP_WASM_VERSION: &str = "0.0.7";
pub const VALKEY_VERSION: &str = "9";
pub const VALKEY_IMAGE: &str = "docker.io/valkey/valkey@sha256:de31910896150d5e754a07d57d227cfdde4e258ddd0d1aa4607f2d2f95843715";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DashboardServiceConfig {
    pub executable: PathBuf,
    pub mm_home: PathBuf,
}

#[must_use]
pub fn development_network_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-services-dev-{}", &digest[..20])
}

#[must_use]
pub fn services_project_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-services-{}", &digest[..20])
}

#[must_use]
pub fn mailpit_data_volume_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-mailpit-data-{}", &digest[..20])
}

#[must_use]
pub fn cap_valkey_data_volume_name(target_id: TargetId) -> String {
    let digest = blake3::hash(target_id.0.as_bytes()).to_hex();
    format!("mm-cap-valkey-data-{}", &digest[..20])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStackAction {
    Apply,
    Upgrade,
    Repair,
    Remove,
}

impl ServiceStackAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Upgrade => "upgrade",
            Self::Repair => "repair",
            Self::Remove => "remove",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceComponentKind {
    LocalIngress,
    MmControlApi,
    Mailpit,
    Cap,
    CapValkey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PersistentVolumeOwner {
    pub volume: String,
    pub uid: u32,
    pub gid: u32,
    pub retained_by_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceComponentDescriptor {
    pub kind: ServiceComponentKind,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub dependencies: Vec<ServiceComponentKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<PersistentVolumeOwner>,
    pub engine_socket_mounted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PreparedServiceStackArtifact {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub compose_path: PathBuf,
    pub route_path: PathBuf,
    pub compose_digest: String,
    pub route_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_unit_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_unit_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceStackPlan {
    pub schema: u32,
    pub action: ServiceStackAction,
    pub stack_version: String,
    pub runtime: RuntimeKind,
    pub target_id: TargetId,
    pub target_fingerprint: String,
    pub components: Vec<ServiceComponentDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dashboard_hosts: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_service: Option<DashboardServiceConfig>,
    pub development_network: String,
    pub mailpit_data_volume: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_valkey_data_volume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_admin_key_file: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_host_smtp_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_smtp_port: Option<u16>,
    pub purge_data: bool,
    pub previous_state_digest: String,
    pub ingress: PreparedTargetIngressArtifact,
    pub artifact: PreparedServiceStackArtifact,
    pub plan_hash: String,
}

impl ServiceStackPlan {
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut canonical = self.clone();
        canonical.plan_hash.clear();
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| format!("could not serialize service plan: {error}"))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SERVICE_STACK_SCHEMA {
            return Err(format!(
                "unsupported service stack plan schema {}",
                self.schema
            ));
        }
        if self.canonical_hash()? != self.plan_hash {
            return Err("service stack plan hash does not match its canonical contents".to_owned());
        }
        if self
            .components
            .iter()
            .any(|component| component.engine_socket_mounted)
        {
            return Err(
                "service stack plans must not mount an engine socket into a component".to_owned(),
            );
        }
        if newer_stack(&self.stack_version)? {
            return Err(format!(
                "unsupported service stack release '{}'; expected {SERVICE_STACK_VERSION} or earlier",
                self.stack_version
            ));
        }
        self.validate_dashboard()?;
        self.validate_components()?;
        if self.host_smtp_port == Some(0) || self.previous_host_smtp_port == Some(0) {
            return Err("host SMTP port must be between 1 and 65535".to_owned());
        }
        Ok(())
    }

    fn validate_dashboard(&self) -> Result<(), String> {
        if self.dashboard_hosts.is_empty()
            || !self
                .dashboard_hosts
                .windows(2)
                .all(|hosts| hosts[0] < hosts[1])
            || self.dashboard_hosts.iter().any(|host| {
                if host == DASHBOARD_HOST {
                    return false;
                }
                let Some(slug) = host
                    .strip_prefix("mm.")
                    .and_then(|value| value.strip_suffix(".test"))
                else {
                    return true;
                };
                slug.is_empty()
                    || slug.len() > 63
                    || slug.starts_with('-')
                    || slug.ends_with('-')
                    || slug.bytes().any(|byte| {
                        !byte.is_ascii_lowercase() && !byte.is_ascii_digit() && byte != b'-'
                    })
            })
        {
            return Err("dashboard certificate hosts are invalid or non-canonical".to_owned());
        }
        if let Some(service) = &self.dashboard_service {
            if !service.executable.is_absolute() || !service.mm_home.is_absolute() {
                return Err("dashboard service paths must be absolute".to_owned());
            }
            let Some(unit_path) = &self.artifact.dashboard_unit_path else {
                return Err("dashboard service unit artifact is missing".to_owned());
            };
            let Some(unit_digest) = &self.artifact.dashboard_unit_digest else {
                return Err("dashboard service unit digest is missing".to_owned());
            };
            if !unit_path.starts_with(&self.artifact.root)
                || unit_digest.len() != 64
                || unit_digest.bytes().any(|byte| !byte.is_ascii_hexdigit())
            {
                return Err("dashboard service unit artifact is invalid".to_owned());
            }
        } else if self.artifact.dashboard_unit_path.is_some()
            || self.artifact.dashboard_unit_digest.is_some()
        {
            return Err("dashboard unit artifact has no service configuration".to_owned());
        } else {
            return Err("current service stack plans require a dashboard service".to_owned());
        }
        Ok(())
    }

    fn validate_components(&self) -> Result<(), String> {
        for component in &self.components {
            if let Some(image) = &component.image
                && !is_digest_pinned(image)
            {
                return Err(format!(
                    "component {:?} image is not digest pinned",
                    component.kind
                ));
            }
        }
        let has_cap = self
            .components
            .iter()
            .any(|component| component.kind == ServiceComponentKind::Cap);
        let has_valkey = self
            .components
            .iter()
            .any(|component| component.kind == ServiceComponentKind::CapValkey);
        if has_cap != has_valkey
            || has_cap != self.cap_valkey_data_volume.is_some()
            || has_cap != self.cap_admin_key_file.is_some()
            || self
                .cap_admin_key_file
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
        {
            return Err(
                "Cap service components and private configuration are inconsistent".to_owned(),
            );
        }
        Ok(())
    }
}

/// Whether `version` is a later service stack than this release installs.
pub fn newer_stack(version: &str) -> Result<bool, String> {
    let parse = |value: &str| {
        semver::Version::parse(value)
            .map_err(|error| format!("service stack release '{value}' is invalid: {error}"))
    };
    Ok(parse(version)? > parse(SERVICE_STACK_VERSION)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStackJournalState {
    Applying,
    Applied,
    Removing,
    Failed,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStackState {
    pub schema: u32,
    pub installation_id: uuid::Uuid,
    pub runtime: RuntimeKind,
    pub target_id: TargetId,
    pub stack_version: String,
    pub plan_hash: String,
    pub artifact: PreparedServiceStackArtifact,
    pub ingress: PreparedTargetIngressArtifact,
    pub development_network: String,
    pub mailpit_data_volume: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_valkey_data_volume: Option<String>,
    pub host_smtp_port: Option<u16>,
    pub journal_state: ServiceStackJournalState,
    pub operation_id: uuid::Uuid,
    pub updated_at_unix_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Box<ServiceStackState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    pub applied_plan: Box<ServiceStackPlan>,
}

impl ServiceStackState {
    pub fn digest(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("could not serialize service stack state: {error}"))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceComponentStatus {
    pub kind: ServiceComponentKind,
    pub version: String,
    pub running: bool,
    pub healthy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceStackStatus {
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<RuntimeKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<TargetId>,
    pub route: String,
    pub internal_smtp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_smtp_port: Option<u16>,
    pub components: Vec<ServiceComponentStatus>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServiceStackResult {
    pub action: ServiceStackAction,
    pub operation_id: uuid::Uuid,
    pub plan_hash: String,
    pub state: ServiceStackJournalState,
    pub rolled_back: bool,
    pub data_purged: bool,
}

fn is_digest_pinned(image: &str) -> bool {
    image.rsplit_once("@sha256:").is_some_and(|(_, digest)| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
pub const DEVELOPMENT_NETWORK_RESOURCE: &str = "services-development";
