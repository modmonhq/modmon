use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::operation::OperationId;
use crate::domain::runtime::RuntimeKind;
use crate::domain::secret::SecretMaterial;
use crate::domain::secret_contract::{DeliveryDriverId, LogicalSecretId, MaterializationName};

pub const RUNTIME_SECRET_PLAN_SCHEMA: &str = "mm-runtime-secret-plan-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSecretStoreKind {
    Podman,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeSecretCapability {
    pub delivery_driver: DeliveryDriverId,
    pub runtime: RuntimeKind,
    pub store: RuntimeSecretStoreKind,
    pub native_storage: bool,
    pub immutable_revisions: bool,
    pub max_material_bytes: usize,
    pub max_runtime_name_bytes: usize,
    pub supports_uid_gid_mode: bool,
    /// Where mm runs as an ordinary user: whom every secret file belongs
    /// to, whatever its entry names. Each file still has its entry's mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files_kept_for: Option<HostIds>,
    pub plaintext_file_lifetime: Option<String>,
}

/// A user and a group of the host, by number.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct HostIds {
    pub uid: u32,
    pub gid: u32,
}

impl HostIds {
    /// The user mm runs as, unless it is root: only root gives a file to
    /// other ids.
    #[must_use]
    pub fn unprivileged() -> Option<Self> {
        #[cfg(unix)]
        {
            let uid = rustix::process::geteuid();
            (!uid.is_root()).then(|| Self {
                uid: uid.as_raw(),
                gid: rustix::process::getegid().as_raw(),
            })
        }
        #[cfg(not(unix))]
        None
    }
}

/// A secret file that belongs to the user mm runs as, not to the ids its
/// entry names, where the difference can matter to the service.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct SecretFileOwnerNote {
    pub service: String,
    pub file: String,
    pub uid: String,
    pub gid: String,
    pub mode: u32,
    pub kept_for: HostIds,
}

impl SecretFileOwnerNote {
    /// None when `consumer` names `user`'s ids, or root's with a mode that
    /// lets every user read the file.
    #[must_use]
    pub fn of(consumer: &RuntimeSecretConsumer, user: HostIds) -> Option<Self> {
        let names = |value: &str, id: u32| value.parse::<u32>().is_ok_and(|value| value == id);
        let applied = names(&consumer.uid, user.uid) && names(&consumer.gid, user.gid);
        let root_for_all = consumer.uid == "0" && consumer.gid == "0" && consumer.mode & 0o004 != 0;
        (!applied && !root_for_all).then(|| Self {
            service: consumer.service.clone(),
            file: consumer.target.clone(),
            uid: consumer.uid.clone(),
            gid: consumer.gid.clone(),
            mode: consumer.mode,
            kept_for: user,
        })
    }

    /// The notes for the secret files `environment` mounts, where each
    /// belongs to `user`.
    #[must_use]
    pub fn in_config(
        config: &crate::domain::config::MmConfig,
        environment: &str,
        user: HostIds,
    ) -> Vec<Self> {
        let mut notes = config
            .active_services(environment)
            .flat_map(|(name, service)| {
                service
                    .secret_files_in(environment)
                    .map(move |(target, file)| RuntimeSecretConsumer {
                        stack: service.stack.clone(),
                        service: name.clone(),
                        target: target.clone(),
                        uid: file.uid.clone(),
                        gid: file.gid.clone(),
                        mode: file.mode,
                    })
            })
            .filter_map(|consumer| Self::of(&consumer, user))
            .collect::<Vec<_>>();
        notes.sort();
        notes
    }

    #[must_use]
    pub const fn readable_by_all(&self) -> bool {
        self.mode & 0o004 != 0
    }

    #[must_use]
    pub fn line(&self) -> String {
        let kept = format!(
            "secret file {} of service '{}' belongs to uid {} and gid {}, the user mm runs as on this machine, not to uid {} and gid {}: only root can give a file to other ids",
            self.file, self.service, self.kept_for.uid, self.kept_for.gid, self.uid, self.gid
        );
        if self.readable_by_all() {
            format!(
                "{kept}. Its mode {:04o} lets every user in the container read it",
                self.mode
            )
        } else {
            format!(
                "{kept}, and its mode {:04o} lets no other user read it, so the service's user may be refused. Use mode 0444, the default: every user in the container can read the file, and other users of this machine still cannot",
                self.mode
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeSecretConsumer {
    pub stack: String,
    pub service: String,
    pub target: String,
    pub uid: String,
    pub gid: String,
    pub mode: u32,
}

impl RuntimeSecretConsumer {
    fn owner_key(&self) -> String {
        format!("{}/{}", self.stack, self.service)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_component(&self.stack, "stack")?;
        validate_component(&self.service, "service")?;
        MaterializationName::file(self.target.clone())?;
        validate_numeric_identity(&self.uid, "uid")?;
        validate_numeric_identity(&self.gid, "gid")?;
        if self.mode == 0 || self.mode & !0o777 != 0 {
            return Err("secret mode must be a non-zero Unix permission mode".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeSecretPlanItem {
    pub logical_name: String,
    pub backend: String,
    pub delivery: DeliveryDriverId,
    pub source_revision: String,
    pub previous_source_revision: Option<String>,
    pub consumers: Vec<RuntimeSecretConsumer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeSecretPlan {
    pub schema: String,
    pub project_id: String,
    pub target_id: String,
    pub environment: String,
    pub runtime: RuntimeKind,
    pub items: Vec<RuntimeSecretPlanItem>,
    /// On rootful Podman: the host id that the container's root is, for
    /// each consumer (`stack/service`) with user ids of its own. Its file
    /// is given to the host ids its `uid` and `gid` are there.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub first_host_ids: BTreeMap<String, u32>,
    pub plan_hash: String,
}

impl RuntimeSecretPlan {
    pub fn new(
        project_id: String,
        target_id: String,
        environment: String,
        runtime: RuntimeKind,
        mut items: Vec<RuntimeSecretPlanItem>,
    ) -> Result<Self, String> {
        validate_identity(&project_id, "project id")?;
        validate_identity(&target_id, "target id")?;
        validate_component(&environment, "environment")?;
        items.sort_by(|left, right| left.logical_name.cmp(&right.logical_name));
        let mut logical_names = BTreeSet::new();
        for item in &mut items {
            LogicalSecretId::new(item.logical_name.clone())?;
            if !logical_names.insert(item.logical_name.as_str()) {
                return Err(format!(
                    "runtime secret '{}' is planned more than once",
                    item.logical_name
                ));
            }
            if item.backend.is_empty()
                || item.source_revision.is_empty()
                || item.source_revision.len() > 1024
                || item.source_revision.chars().any(char::is_control)
                || item.consumers.is_empty()
            {
                return Err(format!(
                    "runtime secret '{}' has incomplete backend, revision, or consumer metadata",
                    item.logical_name
                ));
            }
            item.consumers.sort();
            item.consumers.dedup();
            for consumer in &item.consumers {
                consumer.validate()?;
            }
        }
        let mut plan = Self {
            schema: RUNTIME_SECRET_PLAN_SCHEMA.to_owned(),
            project_id,
            target_id,
            environment,
            runtime,
            items,
            first_host_ids: BTreeMap::new(),
            plan_hash: String::new(),
        };
        plan.plan_hash = plan.canonical_hash()?;
        Ok(plan)
    }

    /// The plan with the first host id of each consumer that has user ids
    /// of its own, by stack and service.
    pub fn with_first_host_ids(
        mut self,
        ids: &BTreeMap<(String, String), u32>,
    ) -> Result<Self, String> {
        self.first_host_ids = self
            .items
            .iter()
            .flat_map(|item| &item.consumers)
            .filter_map(|consumer| {
                ids.get(&(consumer.stack.clone(), consumer.service.clone()))
                    .map(|first| (consumer.owner_key(), *first))
            })
            .collect();
        self.plan_hash = self.canonical_hash()?;
        Ok(self)
    }

    /// The host id the container root of `consumer` is, when its service
    /// has user ids of its own.
    #[must_use]
    pub fn first_host_id(&self, consumer: &RuntimeSecretConsumer) -> Option<u32> {
        self.first_host_ids.get(&consumer.owner_key()).copied()
    }

    /// What the plan's secret files lose by belonging to `user`.
    #[must_use]
    pub fn kept_for(&self, user: HostIds) -> Vec<SecretFileOwnerNote> {
        let mut notes = self
            .items
            .iter()
            .flat_map(|item| &item.consumers)
            .filter_map(|consumer| SecretFileOwnerNote::of(consumer, user))
            .collect::<Vec<_>>();
        notes.sort();
        notes.dedup();
        notes
    }

    pub fn canonical_hash(&self) -> Result<String, String> {
        if self.schema != RUNTIME_SECRET_PLAN_SCHEMA {
            return Err(format!(
                "unsupported runtime secret plan schema '{}'",
                self.schema
            ));
        }
        let mut canonical = self.clone();
        canonical.plan_hash.clear();
        canonical
            .items
            .sort_by(|left, right| left.logical_name.cmp(&right.logical_name));
        for item in &mut canonical.items {
            item.consumers.sort();
        }
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| format!("could not canonicalize runtime secret plan: {error}"))?;
        Ok(blake3::hash(&bytes).to_hex().to_string())
    }
}

pub struct RuntimeSecretMaterial {
    pub logical_name: String,
    pub material: SecretMaterial,
}

impl std::fmt::Debug for RuntimeSecretMaterial {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeSecretMaterial")
            .field("logical_name", &self.logical_name)
            .field("material", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRuntimeSecret {
    pub logical_name: String,
    pub source_revision: String,
    pub runtime_name: String,
    pub created_at_unix_ms: Option<i64>,
    pub consumers: Vec<RuntimeSecretConsumer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRuntimeSecrets {
    pub operation_id: OperationId,
    pub plan_hash: String,
    pub root: PathBuf,
    pub overlays: Vec<(String, PathBuf)>,
    pub rollback_overlays: Vec<(String, PathBuf)>,
    pub secrets: Vec<PreparedRuntimeSecret>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RuntimeSecretStatusItem {
    pub logical_name: String,
    pub backend: String,
    pub delivery: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub age_seconds: Option<u64>,
    pub consumers: Vec<String>,
}

fn validate_identity(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(format!(
            "runtime secret {kind} is empty, too long, or contains control characters"
        ))
    } else {
        Ok(())
    }
}

fn validate_component(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 255
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        Err(format!(
            "runtime secret {kind} must contain 1-255 ASCII letters, numbers, '-' or '_'"
        ))
    } else {
        Ok(())
    }
}

fn validate_numeric_identity(value: &str, kind: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        Err(format!(
            "runtime secret {kind} must be a decimal numeric identity"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
