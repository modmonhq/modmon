use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::workspace::ProjectId;

/// Schema 2 identifies principals by SSH key alone, not by Unix account.
pub const ACCESS_POLICY_SCHEMA_VERSION: u32 = 2;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct PrincipalId(pub Uuid);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AccessAction {
    Plan,
    Status,
    Inspect,
    Logs,
    Restart,
    JobRun,
    JobCancel,
    JobRetry,
    DeployApply,
    BuildApply,
    ConfigApply,
    SecretMetadata,
    SecretSet,
    SecretRotate,
    Exec,
    Forward,
    TeamSync,
    Block,
    PrincipalAdmin,
    KeyAdmin,
    RoleAdmin,
    GrantAdmin,
    TargetAdmin,
    AuditRead,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AccessResourceKind {
    Project,
    Service,
    Job,
    Gate,
    Secret,
    Target,
    AccessPolicy,
    Audit,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct Capability {
    pub action: AccessAction,
    pub resource: AccessResourceKind,
}

impl Capability {
    #[must_use]
    pub const fn new(action: AccessAction, resource: AccessResourceKind) -> Self {
        Self { action, resource }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltInRole {
    Viewer,
    Operator,
    Deployer,
    SecretManager,
    Administrator,
}

impl BuiltInRole {
    #[must_use]
    pub fn capabilities(self) -> BTreeSet<Capability> {
        use AccessAction as A;
        use AccessResourceKind as R;
        let viewer = BTreeSet::from([
            Capability::new(A::Plan, R::Project),
            Capability::new(A::Status, R::Project),
            Capability::new(A::Inspect, R::Project),
            Capability::new(A::Logs, R::Service),
        ]);
        match self {
            Self::Viewer => viewer,
            Self::Operator => viewer
                .into_iter()
                .chain([
                    Capability::new(A::Restart, R::Service),
                    Capability::new(A::JobRun, R::Job),
                    Capability::new(A::JobCancel, R::Job),
                    Capability::new(A::JobRetry, R::Job),
                ])
                .collect(),
            Self::Deployer => Self::Operator
                .capabilities()
                .into_iter()
                .chain([
                    Capability::new(A::DeployApply, R::Project),
                    Capability::new(A::BuildApply, R::Project),
                    Capability::new(A::ConfigApply, R::Project),
                ])
                .collect(),
            Self::SecretManager => BTreeSet::from([
                Capability::new(A::SecretMetadata, R::Secret),
                Capability::new(A::SecretSet, R::Secret),
                Capability::new(A::SecretRotate, R::Secret),
            ]),
            Self::Administrator => BTreeSet::from([
                Capability::new(A::PrincipalAdmin, R::AccessPolicy),
                Capability::new(A::KeyAdmin, R::AccessPolicy),
                Capability::new(A::RoleAdmin, R::AccessPolicy),
                Capability::new(A::GrantAdmin, R::AccessPolicy),
                Capability::new(A::TargetAdmin, R::Target),
                Capability::new(A::AuditRead, R::Audit),
            ]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "name",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RoleRef {
    BuiltIn(BuiltInRole),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomRole {
    pub capabilities: BTreeSet<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrincipalKey {
    pub fingerprint: String,
    /// OpenSSH public key line (`<type> <base64>`), so the host can render
    /// its `authorized_keys`.
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub expires_at_unix_ms: Option<i64>,
}

impl PrincipalKey {
    #[must_use]
    pub fn active_at(&self, now_unix_ms: i64) -> bool {
        !self.disabled
            && self
                .expires_at_unix_ms
                .is_none_or(|expiry| now_unix_ms < expiry)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub id: PrincipalId,
    pub display_name: String,
    /// The project whose users file declares this principal; `None` is a
    /// host break-glass key.
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    /// Users are reconciled per environment, so a key removed from one
    /// environment's users file stops working there even if another
    /// environment still lists it. `None` for break-glass keys.
    #[serde(default)]
    pub environment: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    pub keys: Vec<PrincipalKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeMatcher {
    pub project_id: ProjectId,
    pub environments: BTreeSet<String>,
    #[serde(default)]
    pub services: BTreeSet<String>,
    #[serde(default)]
    pub jobs: BTreeSet<String>,
    #[serde(default)]
    pub gates: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub id: Uuid,
    pub principal_id: PrincipalId,
    pub role: RoleRef,
    pub scope: ScopeMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Denial {
    pub id: Uuid,
    pub principal_id: PrincipalId,
    pub capabilities: BTreeSet<Capability>,
    pub scope: ScopeMatcher,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccessPolicy {
    pub schema_version: u32,
    pub revision: u64,
    pub principals: Vec<Principal>,
    #[serde(default)]
    pub roles: BTreeMap<String, CustomRole>,
    #[serde(default)]
    pub grants: Vec<Grant>,
    #[serde(default)]
    pub denials: Vec<Denial>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRequest<'a> {
    pub principal_id: PrincipalId,
    pub capability: Capability,
    pub project_id: ProjectId,
    pub environment: &'a str,
    pub service: Option<&'a str>,
    pub job: Option<&'a str>,
    pub gate: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authorization {
    pub principal_id: PrincipalId,
    pub display_name: String,
    pub capability: Capability,
    pub policy_revision: u64,
    pub grant_ids: BTreeSet<Uuid>,
    pub authorization_hash: String,
}

impl AccessPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ACCESS_POLICY_SCHEMA_VERSION || self.revision == 0 {
            return Err(format!(
                "access policy schema/revision is invalid; expected schema {ACCESS_POLICY_SCHEMA_VERSION} and a positive revision"
            ));
        }
        if self.principals.is_empty() || self.principals.len() > 65_536 {
            return Err("access policy must contain 1-65536 principals".to_owned());
        }
        let mut ids = BTreeMap::new();
        let mut fingerprints = BTreeSet::new();
        for principal in &self.principals {
            if ids.insert(principal.id, principal.project_id).is_some()
                || !valid_label(&principal.display_name, 255)
                || principal.keys.is_empty()
            {
                return Err(
                    "principal IDs and names must be unique and every principal needs a key"
                        .to_owned(),
                );
            }
            for key in &principal.keys {
                // One key may belong to users of different projects, but only
                // once per project (or once among break-glass keys).
                if !valid_ssh_fingerprint(&key.fingerprint)
                    || !fingerprints.insert((
                        principal.project_id,
                        principal.environment.clone(),
                        key.fingerprint.clone(),
                    ))
                    || key.expires_at_unix_ms.is_some_and(|expiry| expiry <= 0)
                {
                    return Err(format!(
                        "principal '{}' has an invalid, duplicate, or expired key fingerprint",
                        principal.display_name
                    ));
                }
            }
        }
        for (name, role) in &self.roles {
            if !valid_machine_name(name) || role.capabilities.is_empty() {
                return Err("custom role names and capability sets must be valid".to_owned());
            }
        }
        let mut object_ids = BTreeSet::new();
        for grant in &self.grants {
            if !object_ids.insert(grant.id)
                || !ids.get(&grant.principal_id).is_some_and(|project| {
                    project.is_none_or(|project| project == grant.scope.project_id)
                })
            {
                return Err("grant ID, principal, or project reference is invalid".to_owned());
            }
            if let RoleRef::Custom(name) = &grant.role
                && !self.roles.contains_key(name)
            {
                return Err(format!("grant references unknown custom role '{name}'"));
            }
            grant.scope.validate()?;
        }
        for denial in &self.denials {
            if !object_ids.insert(denial.id)
                || !ids.contains_key(&denial.principal_id)
                || denial.capabilities.is_empty()
            {
                return Err("denial ID, principal, or capability set is invalid".to_owned());
            }
            denial.scope.validate()?;
        }
        Ok(())
    }

    /// The principal that owns `key_fingerprint` for `project_id`: the
    /// project's own user first, then a host break-glass key.
    pub fn authenticate(
        &self,
        key_fingerprint: &str,
        project_id: ProjectId,
        environment: &str,
        now_unix_ms: i64,
    ) -> Result<&Principal, String> {
        let owns_key = |principal: &&Principal| {
            principal
                .keys
                .iter()
                .any(|key| key.fingerprint == key_fingerprint)
        };
        let principal = self
            .principals
            .iter()
            .filter(owns_key)
            .find(|principal| {
                principal.project_id == Some(project_id)
                    && principal.environment.as_deref() == Some(environment)
            })
            .or_else(|| {
                self.principals
                    .iter()
                    .filter(owns_key)
                    .find(|principal| principal.project_id.is_none() && principal.environment.is_none())
            })
            .ok_or_else(|| {
                "this SSH key has no access to the project on this host; ask an admin to add it to the users file and deploy or sync".to_owned()
            })?;
        if principal.disabled {
            return Err("principal is disabled".to_owned());
        }
        let active = principal
            .keys
            .iter()
            .any(|key| key.fingerprint == key_fingerprint && key.active_at(now_unix_ms));
        if !active {
            return Err("authenticated SSH key is blocked or expired".to_owned());
        }
        Ok(principal)
    }

    /// Every distinct active public key with its earliest expiry, for the
    /// host's `authorized_keys`. sshd only decides who may reach the relay;
    /// the daemon still authorizes each request.
    #[must_use]
    pub fn authorized_keys(&self, now_unix_ms: i64) -> BTreeMap<String, Option<i64>> {
        let mut keys = BTreeMap::<String, Option<i64>>::new();
        for principal in self
            .principals
            .iter()
            .filter(|principal| !principal.disabled)
        {
            for key in principal
                .keys
                .iter()
                .filter(|key| key.active_at(now_unix_ms) && !key.public_key.is_empty())
            {
                keys.entry(key.public_key.clone())
                    .and_modify(|expiry| {
                        *expiry = match (*expiry, key.expires_at_unix_ms) {
                            (Some(left), Some(right)) => Some(left.max(right)),
                            _ => None,
                        };
                    })
                    .or_insert(key.expires_at_unix_ms);
            }
        }
        keys
    }

    pub fn authorize(&self, request: &AccessRequest<'_>) -> Result<Authorization, String> {
        let principal = self
            .principals
            .iter()
            .find(|principal| principal.id == request.principal_id && !principal.disabled)
            .ok_or_else(|| "principal is absent or disabled".to_owned())?;
        if self.denials.iter().any(|denial| {
            denial.principal_id == request.principal_id
                && denial.capabilities.contains(&request.capability)
                && denial.scope.matches(request)
        }) {
            return Err("an explicit denial matches this capability and scope".to_owned());
        }
        let grant_ids = self
            .grants
            .iter()
            .filter(|grant| {
                grant.principal_id == request.principal_id
                    && grant.scope.matches(request)
                    && self
                        .role_capabilities(&grant.role)
                        .is_some_and(|set| set.contains(&request.capability))
            })
            .map(|grant| grant.id)
            .collect::<BTreeSet<_>>();
        if grant_ids.is_empty() {
            return Err("no grant matches this capability and exact scope".to_owned());
        }
        let mut hasher = blake3::Hasher::new();
        for field in [
            "mm-access-authorization-v3".to_owned(),
            request.principal_id.0.to_string(),
            principal.display_name.clone(),
            format!("{:?}", request.capability.action),
            format!("{:?}", request.capability.resource),
            request.project_id.0.to_string(),
            request.environment.to_owned(),
            request.service.unwrap_or_default().to_owned(),
            request.job.unwrap_or_default().to_owned(),
            request.gate.unwrap_or_default().to_owned(),
        ] {
            hasher.update(&(field.len() as u64).to_be_bytes());
            hasher.update(field.as_bytes());
        }
        for grant in self
            .grants
            .iter()
            .filter(|grant| grant_ids.contains(&grant.id))
        {
            let content = format!("{}\0{:?}", grant.id, self.role_capabilities(&grant.role));
            hasher.update(&(content.len() as u64).to_be_bytes());
            hasher.update(content.as_bytes());
        }
        Ok(Authorization {
            principal_id: request.principal_id,
            display_name: principal.display_name.clone(),
            capability: request.capability,
            policy_revision: self.revision,
            grant_ids,
            authorization_hash: hasher.finalize().to_hex().to_string(),
        })
    }

    #[must_use]
    pub fn active_administrator_ids_at(&self, now_unix_ms: i64) -> BTreeSet<PrincipalId> {
        self.grants
            .iter()
            .filter(|grant| {
                matches!(grant.role, RoleRef::BuiltIn(BuiltInRole::Administrator))
                    && self.principals.iter().any(|principal| {
                        principal.id == grant.principal_id
                            && !principal.disabled
                            && principal.keys.iter().any(|key| key.active_at(now_unix_ms))
                    })
            })
            .map(|grant| grant.principal_id)
            .collect()
    }

    fn role_capabilities(&self, role: &RoleRef) -> Option<BTreeSet<Capability>> {
        match role {
            RoleRef::BuiltIn(role) => Some(role.capabilities()),
            RoleRef::Custom(name) => self.roles.get(name).map(|role| role.capabilities.clone()),
        }
    }
}

impl ScopeMatcher {
    fn validate(&self) -> Result<(), String> {
        if self.environments.is_empty()
            || self
                .environments
                .iter()
                .any(|value| !valid_machine_name(value))
        {
            return Err("grant/denial scope requires explicit environments".to_owned());
        }
        Ok(())
    }

    fn matches(&self, request: &AccessRequest<'_>) -> bool {
        self.project_id == request.project_id
            && self.environments.contains(request.environment)
            && scoped_name_matches(&self.services, request.service)
            && scoped_name_matches(&self.jobs, request.job)
            && scoped_name_matches(&self.gates, request.gate)
    }
}

fn scoped_name_matches(values: &BTreeSet<String>, requested: Option<&str>) -> bool {
    requested.is_none_or(|requested| values.is_empty() || values.contains(requested))
}

fn valid_label(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn valid_machine_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
}

fn valid_ssh_fingerprint(value: &str) -> bool {
    value.strip_prefix("SHA256:").is_some_and(|digest| {
        !digest.is_empty()
            && digest.len() <= 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
    })
}

#[cfg(test)]
#[path = "access/tests.rs"]
mod tests;
