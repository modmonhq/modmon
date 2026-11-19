//! Host policy (`/etc/mm/host.yaml`): the root-owned limits a host puts on
//! every project it runs. Repo files say who may do what; the host decides
//! which projects it runs, the highest level a repo may grant, workload
//! exceptions, and the break-glass keys that always work.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::workspace::ProjectId;

pub const HOST_POLICY_SCHEMA_VERSION: u32 = 1;

/// Access levels a repo grants per environment. Each includes the previous.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum AccessLevel {
    /// Plan, status, inspect, and logs.
    View,
    /// `view` plus restart and job run, cancel, and retry.
    Operate,
    /// `operate` plus deploy, rollback, and team sync.
    Deploy,
    /// `deploy` plus exec, forward, setting secrets and variables,
    /// emergency blocks, and audit.
    Admin,
}

impl AccessLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Operate => "operate",
            Self::Deploy => "deploy",
            Self::Admin => "admin",
        }
    }
}

/// Services an add-on applies to: all of them, none, or a named list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ServiceSelection {
    All(bool),
    Only(BTreeSet<String>),
}

impl Default for ServiceSelection {
    fn default() -> Self {
        Self::All(false)
    }
}

impl ServiceSelection {
    #[must_use]
    pub fn is_none(&self) -> bool {
        match self {
            Self::All(enabled) => !enabled,
            Self::Only(services) => services.is_empty(),
        }
    }

    /// The services this selection allows, where an empty set means every
    /// service; `None` means the add-on is off.
    #[must_use]
    pub fn services(&self) -> Option<BTreeSet<String>> {
        match self {
            Self::All(true) => Some(BTreeSet::new()),
            Self::All(false) => None,
            Self::Only(services) if services.is_empty() => None,
            Self::Only(services) => Some(services.clone()),
        }
    }

    /// The narrower of two selections.
    #[must_use]
    pub fn intersect(&self, ceiling: &Self) -> Self {
        match (self.services(), ceiling.services()) {
            (None, _) | (_, None) => Self::All(false),
            (Some(mine), Some(limit)) if limit.is_empty() => {
                if mine.is_empty() {
                    Self::All(true)
                } else {
                    Self::Only(mine)
                }
            }
            (Some(mine), Some(limit)) if mine.is_empty() => Self::Only(limit),
            (Some(mine), Some(limit)) => {
                let both = mine.intersection(&limit).cloned().collect::<BTreeSet<_>>();
                if both.is_empty() {
                    Self::All(false)
                } else {
                    Self::Only(both)
                }
            }
        }
    }
}

/// One user's effective access in one environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct EnvironmentAccess {
    pub level: AccessLevel,
    #[serde(default, skip_serializing_if = "ServiceSelection::is_none")]
    pub exec: ServiceSelection,
    #[serde(default, skip_serializing_if = "ServiceSelection::is_none")]
    pub forward: ServiceSelection,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub secrets: bool,
}

impl EnvironmentAccess {
    #[must_use]
    pub const fn level(level: AccessLevel) -> Self {
        Self {
            level,
            exec: ServiceSelection::All(false),
            forward: ServiceSelection::All(false),
            secrets: false,
        }
    }

    /// `deploy +exec[api] +forward`, for one-line display.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut text = self.level.as_str().to_owned();
        for (name, selection) in [("exec", &self.exec), ("forward", &self.forward)] {
            match selection.services() {
                Some(services) if services.is_empty() => text.push_str(&format!(" +{name}")),
                Some(services) => text.push_str(&format!(
                    " +{name}[{}]",
                    services.into_iter().collect::<Vec<_>>().join(",")
                )),
                None => {}
            }
        }
        if self.secrets {
            text.push_str(" +secrets");
        }
        text
    }

    /// Exec, forward, and secrets come with `admin` and are otherwise opt-in.
    #[must_use]
    pub fn effective(&self) -> Self {
        if self.level == AccessLevel::Admin {
            Self {
                level: AccessLevel::Admin,
                exec: ServiceSelection::All(true),
                forward: ServiceSelection::All(true),
                secrets: true,
            }
        } else {
            self.clone()
        }
    }

    /// This access for an agent: at most `deploy`, with no forward, and
    /// only the exec and secrets it was given.
    #[must_use]
    pub fn without_admin(&self) -> Self {
        Self {
            level: self.level.min(AccessLevel::Deploy),
            exec: self.exec.clone(),
            forward: ServiceSelection::All(false),
            secrets: self.secrets,
        }
    }

    /// This access limited by the host's ceiling for the environment.
    #[must_use]
    pub fn within(&self, ceiling: &Self) -> Self {
        let mine = self.effective();
        let limit = ceiling.effective();
        Self {
            level: mine.level.min(limit.level),
            exec: mine.exec.intersect(&limit.exec),
            forward: mine.forward.intersect(&limit.forward),
            secrets: mine.secrets && limit.secrets,
        }
    }
}

/// A key that is admin on every enrolled project, for recovery. Keep the
/// private half offline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct BreakGlassKey {
    pub name: String,
    pub key: String,
}

/// A Compose feature that would escape to the host, allowed for one service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct GuardrailException {
    /// `<stack>/<service>`.
    pub service: String,
    pub features: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct EnrolledProject {
    pub name: String,
    /// Git URL `mmd` fetches from with its own read-only deploy key. Not
    /// used when clients push the project's files.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
    /// `push`: clients send each commit's files and `mmd` never contacts
    /// the git server.
    #[serde(
        default,
        skip_serializing_if = "crate::domain::delivery::Delivery::is_fetch"
    )]
    pub delivery: crate::domain::delivery::Delivery,
    /// Pinned SSH host key of the git server, for SSH origins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_host_key: Option<String>,
    /// Environments this host may run for the project.
    pub environments: BTreeSet<String>,
    /// Branches a commit must be reachable from before the repo sets its own
    /// `deploy-from`; starts the ratchet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub initial_deploy_from: BTreeMap<String, Vec<String>>,
    /// Highest access the repo may grant, per environment. Missing means
    /// `admin`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ceiling: BTreeMap<String, EnvironmentAccess>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<GuardrailException>,
}

impl EnrolledProject {
    #[must_use]
    pub fn ceiling_for(&self, environment: &str) -> EnvironmentAccess {
        self.ceiling
            .get(environment)
            .cloned()
            .unwrap_or_else(|| EnvironmentAccess::level(AccessLevel::Admin))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct HostPolicy {
    pub schema_version: u32,
    pub break_glass: Vec<BreakGlassKey>,
    #[serde(default)]
    pub projects: BTreeMap<ProjectId, EnrolledProject>,
    /// The OCI runtimes this host's engine has besides its default, by the
    /// name the engine knows them by. A service's `runtime:` has to be one
    /// of them; `mm doctor` runs a container under each.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub runtimes: BTreeSet<String>,
    /// One of `runtimes` that every service which publishes a port or
    /// takes ingress traffic runs under on this host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposed_runtime: Option<String>,
}

impl HostPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != HOST_POLICY_SCHEMA_VERSION {
            return Err(format!(
                "host policy schema-version {} is not supported; expected {HOST_POLICY_SCHEMA_VERSION}",
                self.schema_version
            ));
        }
        if self.break_glass.is_empty() {
            return Err("host policy needs at least one break-glass key so the host can always be recovered".to_owned());
        }
        let mut names = BTreeSet::new();
        let mut fingerprints = BTreeSet::new();
        for entry in &self.break_glass {
            if !valid_user_name(&entry.name) || !names.insert(entry.name.as_str()) {
                return Err(format!(
                    "break-glass name '{}' must be unique and match [a-z][a-z0-9-]{{0,62}}",
                    entry.name
                ));
            }
            let key = parse_public_key(&entry.key)
                .map_err(|message| format!("break-glass key '{}': {message}", entry.name))?;
            if !fingerprints.insert(key.fingerprint) {
                return Err(format!("break-glass key '{}' is listed twice", entry.name));
            }
        }
        self.validate_runtimes()?;
        for (id, project) in &self.projects {
            if project.name.is_empty()
                || (project.delivery.is_fetch() && project.origin.trim().is_empty())
            {
                return Err(format!(
                    "project {} needs a name and a git origin, or `delivery: push`",
                    id.0
                ));
            }
            if project.environments.is_empty() {
                return Err(format!(
                    "project '{}' must list the environments this host may run",
                    project.name
                ));
            }
            for environment in project
                .ceiling
                .keys()
                .chain(project.initial_deploy_from.keys())
            {
                if !project.environments.contains(environment) {
                    return Err(format!(
                        "project '{}' sets limits for environment '{environment}', which this host does not run",
                        project.name
                    ));
                }
            }
            for (environment, branches) in &project.initial_deploy_from {
                if branches.is_empty() {
                    return Err(format!(
                        "project '{}' initial-deploy-from for '{environment}' lists no branches",
                        project.name
                    ));
                }
                for branch in branches {
                    crate::domain::branch_pattern::validate(branch).map_err(|problem| {
                        format!(
                            "project '{}' initial-deploy-from for '{environment}': {problem}",
                            project.name
                        )
                    })?;
                }
            }
        }
        Ok(())
    }
}

pub const ACCESS_RECORD_SCHEMA_VERSION: u32 = 1;

/// One user's reconciled access in one environment, with checked keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ReconciledUser {
    #[serde(default)]
    pub machine: bool,
    pub keys: Vec<ReconciledKey>,
    pub access: EnvironmentAccess,
    /// The user's AI agent, when the users file marks keys `agent: true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<ReconciledAgent>,
}

/// A user's agent keys and what they may do: the user's access limited by
/// the `agents:` block of the same commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ReconciledAgent {
    pub keys: Vec<ReconciledKey>,
    pub access: EnvironmentAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct ReconciledKey {
    /// `<type> <base64>`, without the comment.
    pub public_key: String,
    pub fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<i64>,
}

/// The users file of the commit last deployed or synced to one environment,
/// already limited by the host ceiling. `mmd` stores one per environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct EnvironmentAccessRecord {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub environment: String,
    pub commit: String,
    pub users: BTreeMap<String, ReconciledUser>,
    /// Branches the environment's commits must come from, if the repo sets
    /// them; the next change must pass this list (the ratchet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deploy_from: Option<Vec<String>>,
    pub reconciled_at_unix_ms: i64,
    pub reconciled_by: String,
}

impl EnvironmentAccessRecord {
    /// What the holder of a key may do here within the host `ceiling`: a
    /// user's own access for one of their keys, and the narrower agent
    /// access for one of their agent keys.
    #[must_use]
    pub fn access_of_key(
        &self,
        fingerprint: &str,
        ceiling: &EnvironmentAccess,
    ) -> Option<EnvironmentAccess> {
        let holds = |keys: &[ReconciledKey]| keys.iter().any(|key| key.fingerprint == fingerprint);
        self.users.values().find_map(|user| {
            if holds(&user.keys) {
                return Some(user.access.within(ceiling));
            }
            user.agent
                .as_ref()
                .filter(|agent| holds(&agent.keys))
                .map(|agent| agent_access(&agent.access, &user.access, ceiling))
        })
    }
}

/// An agent's access: at most its user's and the ceiling, never admin, and
/// `secrets` only when its own grant names it, not through a level.
fn agent_access(
    agent: &EnvironmentAccess,
    user: &EnvironmentAccess,
    ceiling: &EnvironmentAccess,
) -> EnvironmentAccess {
    let mut access = agent.within(user).within(ceiling).without_admin();
    access.secrets &= agent.secrets;
    access
}

/// A server-side block that takes effect at once, before the repo catches up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct EmergencyBlock {
    pub project_id: ProjectId,
    pub environment: String,
    pub fingerprint: String,
    pub blocked_by: String,
    pub blocked_at_unix_ms: i64,
}

/// Capabilities of each level and add-on, as custom role names.
#[must_use]
pub fn level_roles() -> BTreeMap<String, crate::domain::access::CustomRole> {
    use crate::domain::access::{
        AccessAction as A, AccessResourceKind as R, Capability, CustomRole,
    };
    let view = BTreeSet::from([
        Capability::new(A::Plan, R::Project),
        Capability::new(A::Status, R::Project),
        Capability::new(A::Inspect, R::Project),
        Capability::new(A::Logs, R::Service),
        // The hosts the environment runs on.
        Capability::new(A::Inspect, R::Target),
    ]);
    let mut operate = view.clone();
    operate.extend([
        Capability::new(A::Restart, R::Service),
        Capability::new(A::JobRun, R::Job),
        Capability::new(A::JobCancel, R::Job),
        Capability::new(A::JobRetry, R::Job),
    ]);
    let mut deploy = operate.clone();
    deploy.extend([
        Capability::new(A::DeployApply, R::Project),
        Capability::new(A::ConfigApply, R::Project),
        Capability::new(A::TeamSync, R::Project),
    ]);
    let mut admin = deploy.clone();
    admin.extend([
        Capability::new(A::Block, R::AccessPolicy),
        Capability::new(A::AuditRead, R::Audit),
        // Placement groups, storage, and ingress nodes of the cluster.
        Capability::new(A::TargetAdmin, R::Target),
    ]);
    [
        ("view", view),
        ("operate", operate),
        ("deploy", deploy),
        ("admin", admin),
        (
            "exec",
            BTreeSet::from([Capability::new(A::Exec, R::Service)]),
        ),
        (
            "forward",
            BTreeSet::from([Capability::new(A::Forward, R::Service)]),
        ),
        (
            "secrets",
            BTreeSet::from([
                Capability::new(A::SecretMetadata, R::Secret),
                Capability::new(A::SecretSet, R::Secret),
                Capability::new(A::SecretRotate, R::Secret),
            ]),
        ),
    ]
    .into_iter()
    .map(|(name, capabilities)| (name.to_owned(), CustomRole { capabilities }))
    .collect()
}

/// A stable ID derived from its parts, so the same user in the same
/// environment keeps one principal across reconciles.
#[must_use]
pub fn derived_id(parts: &[&str]) -> uuid::Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mm-principal-v1\0");
    for part in parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    uuid::Builder::from_random_bytes(bytes).into_uuid()
}

/// The policy `mmd` enforces: break-glass keys are admin everywhere; each
/// project user gets what its environment's reconciled users file grants,
/// within the host ceiling, minus emergency blocks.
#[must_use]
pub fn effective_policy(
    host: &HostPolicy,
    records: &[EnvironmentAccessRecord],
    blocks: &[EmergencyBlock],
    revision: u64,
) -> crate::domain::access::AccessPolicy {
    let mut builder = PolicyBuilder::default();
    builder.break_glass(host);
    for record in records {
        builder.project_users(host, record, blocks);
    }
    crate::domain::access::AccessPolicy {
        schema_version: crate::domain::access::ACCESS_POLICY_SCHEMA_VERSION,
        revision: revision.max(1),
        principals: builder.principals,
        roles: level_roles(),
        grants: builder.grants,
        denials: Vec::new(),
    }
}

#[derive(Default)]
struct PolicyBuilder {
    principals: Vec<crate::domain::access::Principal>,
    grants: Vec<crate::domain::access::Grant>,
}

impl PolicyBuilder {
    fn grant(
        &mut self,
        principal: crate::domain::access::PrincipalId,
        role: &str,
        project: ProjectId,
        environments: BTreeSet<String>,
        services: BTreeSet<String>,
    ) {
        self.grants.push(crate::domain::access::Grant {
            id: derived_id(&[
                &principal.0.to_string(),
                role,
                &project.0.to_string(),
                &environments.iter().cloned().collect::<Vec<_>>().join(","),
            ]),
            principal_id: principal,
            role: crate::domain::access::RoleRef::Custom(role.to_owned()),
            scope: crate::domain::access::ScopeMatcher {
                project_id: project,
                environments,
                services,
                jobs: BTreeSet::new(),
                gates: BTreeSet::new(),
            },
        });
    }

    fn break_glass(&mut self, host: &HostPolicy) {
        use crate::domain::access::{Principal, PrincipalId, PrincipalKey};
        for entry in &host.break_glass {
            let Ok(key) = parse_public_key(&entry.key) else {
                continue;
            };
            let id = PrincipalId(derived_id(&["break-glass", &entry.name]));
            self.principals.push(Principal {
                id,
                display_name: format!("break-glass:{}", entry.name),
                project_id: None,
                environment: None,
                disabled: false,
                keys: vec![PrincipalKey {
                    fingerprint: key.fingerprint.clone(),
                    public_key: key.openssh(),
                    disabled: false,
                    expires_at_unix_ms: None,
                }],
            });
            for (project, enrolled) in &host.projects {
                for role in ["admin", "exec", "forward", "secrets"] {
                    self.grant(
                        id,
                        role,
                        *project,
                        enrolled.environments.clone(),
                        BTreeSet::new(),
                    );
                }
            }
        }
    }

    fn project_users(
        &mut self,
        host: &HostPolicy,
        record: &EnvironmentAccessRecord,
        blocks: &[EmergencyBlock],
    ) {
        let Some(enrolled) = host.projects.get(&record.project_id) else {
            return;
        };
        if !enrolled.environments.contains(&record.environment) {
            return;
        }
        let ceiling = enrolled.ceiling_for(&record.environment);
        for (name, user) in &record.users {
            self.project_principal(
                record,
                blocks,
                (&[name], name.clone()),
                &user.keys,
                &user.access.within(&ceiling),
            );
            if let Some(agent) = &user.agent {
                // An agent never gets more than its user, whatever the record says.
                let access = agent_access(&agent.access, &user.access, &ceiling);
                self.project_principal(
                    record,
                    blocks,
                    (
                        &[name, "agent"],
                        crate::domain::users::agent_display_name(name),
                    ),
                    &agent.keys,
                    &access,
                );
            }
        }
    }

    /// One principal for one set of keys in the record's environment, with
    /// the grants `access` gives it.
    fn project_principal(
        &mut self,
        record: &EnvironmentAccessRecord,
        blocks: &[EmergencyBlock],
        (identity, display_name): (&[&str], String),
        keys: &[ReconciledKey],
        access: &EnvironmentAccess,
    ) {
        use crate::domain::access::{Principal, PrincipalId, PrincipalKey};
        let project = record.project_id.0.to_string();
        let mut parts = vec![project.as_str(), record.environment.as_str()];
        parts.extend_from_slice(identity);
        let id = PrincipalId(derived_id(&parts));
        let keys = keys
            .iter()
            .map(|key| PrincipalKey {
                fingerprint: key.fingerprint.clone(),
                public_key: key.public_key.clone(),
                disabled: blocks.iter().any(|block| {
                    block.project_id == record.project_id
                        && block.environment == record.environment
                        && block.fingerprint == key.fingerprint
                }),
                expires_at_unix_ms: key.expires_at_unix_ms,
            })
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return;
        }
        self.principals.push(Principal {
            id,
            display_name,
            project_id: Some(record.project_id),
            environment: Some(record.environment.clone()),
            disabled: false,
            keys,
        });
        let environment = BTreeSet::from([record.environment.clone()]);
        self.grant(
            id,
            access.level.as_str(),
            record.project_id,
            environment.clone(),
            BTreeSet::new(),
        );
        for (role, selection) in [("exec", &access.exec), ("forward", &access.forward)] {
            if let Some(services) = selection.services() {
                self.grant(id, role, record.project_id, environment.clone(), services);
            }
        }
        if access.secrets {
            self.grant(
                id,
                "secrets",
                record.project_id,
                environment,
                BTreeSet::new(),
            );
        }
    }
}

/// An OpenSSH public key line, checked and fingerprinted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub algorithm: String,
    pub blob: String,
    pub comment: String,
    /// `SHA256:<unpadded base64>`, as `ssh-keygen -l` prints it.
    pub fingerprint: String,
}

impl PublicKey {
    /// `<algorithm> <base64>`, without the comment.
    #[must_use]
    pub fn openssh(&self) -> String {
        format!("{} {}", self.algorithm, self.blob)
    }
}

/// Key types a user may register. ECDSA and RSA are rejected in v1.
pub const ALLOWED_KEY_TYPES: [&str; 2] = ["ssh-ed25519", "sk-ssh-ed25519@openssh.com"];

pub fn parse_public_key(line: &str) -> Result<PublicKey, String> {
    use base64::Engine;
    use sha2::Digest;

    let mut parts = line.split_whitespace();
    let algorithm = parts.next().unwrap_or_default();
    let blob = parts.next().unwrap_or_default();
    let comment = parts.collect::<Vec<_>>().join(" ");
    if !ALLOWED_KEY_TYPES.contains(&algorithm) {
        return Err(format!(
            "key type '{algorithm}' is not allowed; use {}",
            ALLOWED_KEY_TYPES.join(" or ")
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(blob)
        .map_err(|_| "public key is not valid base64".to_owned())?;
    // The blob starts with its own length-prefixed algorithm name.
    let declared = bytes
        .get(..4)
        .map(|length| u32::from_be_bytes([length[0], length[1], length[2], length[3]]) as usize)
        .and_then(|length| bytes.get(4..4 + length))
        .ok_or_else(|| "public key blob is truncated".to_owned())?;
    if declared != algorithm.as_bytes() {
        return Err("public key blob does not match its key type".to_owned());
    }
    if comment.chars().any(char::is_control) {
        return Err("public key comment contains control characters".to_owned());
    }
    let digest = sha2::Sha256::digest(&bytes);
    Ok(PublicKey {
        algorithm: algorithm.to_owned(),
        blob: blob.to_owned(),
        comment,
        fingerprint: format!(
            "SHA256:{}",
            base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
        ),
    })
}

/// User names in the repo's `users:` key and break-glass names.
#[must_use]
pub fn valid_user_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && name.len() <= 63
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[path = "host_policy/enrollment.rs"]
mod enrollment;
#[path = "host_policy/runtimes.rs"]
mod runtimes;
pub use enrollment::validate_environment_name;
pub use enrollment::{
    ProjectEnrollment, duplicate_key_message, origin_uses_ssh, parse_break_glass, validate_origin,
};

#[cfg(test)]
#[path = "host_policy/tests.rs"]
mod tests;
