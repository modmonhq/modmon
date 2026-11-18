use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::runtime::RuntimeKind;
use crate::domain::target::TargetId;

pub const MANAGED_SERVICE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedServiceFile {
    pub source: PathBuf,
    pub name: String,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedServicePlanRequest {
    pub target: String,
    pub runtime: RuntimeKind,
    pub provider_instance: String,
    pub component: String,
    pub image: String,
    pub host_port: u16,
    pub container_port: u16,
    pub user: String,
    pub entrypoint: String,
    pub command: Vec<String>,
    pub healthcheck: Vec<String>,
    pub config_mount: String,
    pub data_mount: String,
    pub config_files: Vec<ManagedServiceFile>,
    pub ingress_files: Vec<ManagedServiceFile>,
    pub generation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedServiceIdentity {
    pub target: String,
    pub target_id: TargetId,
    pub target_fingerprint: String,
    pub runtime: RuntimeKind,
    pub provider_instance: String,
    pub component: String,
    pub project_name: String,
    pub service_name: String,
    pub data_volume: String,
    pub config_volume: String,
    pub ingress_network: String,
    pub ingress_config_volume: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedServiceSpec {
    pub image: String,
    pub host_port: u16,
    pub container_port: u16,
    pub user: String,
    pub entrypoint: String,
    pub command: Vec<String>,
    pub healthcheck: Vec<String>,
    pub config_mount: String,
    pub data_mount: String,
    pub config_files: Vec<ManagedServiceFile>,
    pub ingress_files: Vec<ManagedServiceFile>,
    pub generation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ManagedServicePlan {
    pub schema: u32,
    pub identity: ManagedServiceIdentity,
    pub service: ManagedServiceSpec,
    pub artifact_root: PathBuf,
    pub compose_path: PathBuf,
    pub plan_hash: String,
}

impl ManagedServicePlan {
    pub fn canonical_hash(&self) -> Result<String, String> {
        let mut canonical = self.clone();
        canonical.plan_hash.clear();
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| format!("could not serialize managed service plan: {error}"))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != MANAGED_SERVICE_SCHEMA {
            return Err(format!(
                "unsupported managed service schema {}",
                self.schema
            ));
        }
        if self.service.host_port == 0
            || self.service.container_port == 0
            || self.service.command.is_empty()
            || self.service.healthcheck.is_empty()
            || self.service.config_files.is_empty()
            || self.identity.provider_instance.is_empty()
            || self.identity.component.is_empty()
            || !self.compose_path.starts_with(&self.artifact_root)
        {
            return Err("managed service plan contains incomplete typed intent".to_owned());
        }
        if self
            .service
            .config_files
            .iter()
            .chain(&self.service.ingress_files)
            .any(|file| !safe_file_name(&file.name) || !file.source.is_absolute())
        {
            return Err("managed service plan contains an unsafe file intent".to_owned());
        }
        if self.canonical_hash()? != self.plan_hash {
            return Err("managed service plan hash does not match its contents".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedServiceStatus {
    pub running: bool,
    pub healthy: bool,
    pub image: Option<String>,
}

fn safe_file_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value != "."
        && value != ".."
        && !value.contains(['/', '\\'])
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests;
