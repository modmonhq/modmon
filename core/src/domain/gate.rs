use std::collections::BTreeMap;
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::secret::SecretReference;
use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

pub const PASSWORD_CAP_DRIVER: &str = "password-cap";
pub const DEFAULT_SESSION_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;
pub const MAX_SESSION_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
pub const SESSION_CLOCK_SKEW_SECONDS: i64 = 60;
pub const ARGON2_VERSION: u32 = 1;
pub const ARGON2_MEMORY_KIB: u32 = 19_456;
pub const ARGON2_ITERATIONS: u32 = 2;
pub const ARGON2_PARALLELISM: u32 = 1;
const GATE_COOKIE_PREFIX: &str = "__Host-mm_gate_";
const GATE_CSRF_COOKIE_PREFIX: &str = "__Host-mm_gate_csrf_";

#[must_use]
pub fn gate_session_cookie_name(policy_id: Uuid) -> String {
    format!("{GATE_COOKIE_PREFIX}{}", policy_id.simple())
}

#[must_use]
pub fn gate_csrf_cookie_name(policy_id: Uuid) -> String {
    format!("{GATE_CSRF_COOKIE_PREFIX}{}", policy_id.simple())
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatePolicyMode {
    Any,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct GateDriverId(String);

impl GateDriverId {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if is_resource_name(&value) {
            Ok(Self(value))
        } else {
            Err("gate driver IDs must be 1-63 lowercase ASCII letters, numbers, or '-'".to_owned())
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct GatePolicyName(String);

impl GatePolicyName {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if is_resource_name(&value) {
            Ok(Self(value))
        } else {
            Err(
                "gate policy names must be 1-63 lowercase ASCII letters, numbers, or '-'"
                    .to_owned(),
            )
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GatePolicyName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatePolicy {
    pub id: Uuid,
    pub name: GatePolicyName,
    pub project_id: ProjectId,
    pub environment: String,
    pub mode: GatePolicyMode,
    pub drivers: Vec<GateDriverId>,
    pub session_ttl_seconds: u64,
    pub secret_provider: String,
    pub password_pepper: SecretReference,
    pub session_keys: VersionedSecretReferences,
    pub cap_site_key: String,
    pub cap_site_secret: SecretReference,
    pub revision: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

impl GatePolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.drivers.is_empty() {
            return Err("gate policies require at least one driver".to_owned());
        }
        if !self.drivers.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err("gate policy drivers must be unique and canonically sorted".to_owned());
        }
        if !(60..=MAX_SESSION_TTL_SECONDS).contains(&self.session_ttl_seconds) {
            return Err(format!(
                "gate session duration must be between 60 and {MAX_SESSION_TTL_SECONDS} seconds"
            ));
        }
        if self.revision == 0 || self.session_keys.current_version == 0 {
            return Err("gate policy and key versions begin at one".to_owned());
        }
        if self.cap_site_key.is_empty()
            || self.cap_site_key.len() > 128
            || self.cap_site_key.chars().any(char::is_control)
        {
            return Err("Cap site keys must be 1-128 bytes without control characters".to_owned());
        }
        Ok(())
    }

    #[must_use]
    pub fn forward_auth_middleware_name(&self, target_id: TargetId) -> String {
        let identity = format!("{}:{}:{}", target_id.0, self.project_id.0, self.id);
        let digest = blake3::hash(identity.as_bytes()).to_hex();
        format!("mm-gate-{}-{}", self.name, &digest[..16])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionedSecretReferences {
    pub current_version: u32,
    pub current: SecretReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<PreviousSecretReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PreviousSecretReference {
    pub version: u32,
    pub reference: SecretReference,
    pub accept_until_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateRouteAttachment {
    pub policy_id: Uuid,
    pub target_id: TargetId,
    pub project_id: ProjectId,
    pub environment: String,
    pub router_name: String,
    pub host: String,
    pub path_prefix: String,
    pub middleware_name: String,
    pub attached_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatePrincipal {
    pub user_id: Uuid,
    pub normalized_username: String,
    pub display_username: String,
    pub authenticated_drivers: Vec<GateDriverId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateChallenge {
    pub driver: GateDriverId,
    pub kind: GateChallengeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GateChallengeKind {
    PasswordCap { cap_endpoint: String },
    Redirect { location: String },
    Header { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDriverResult {
    Authenticated(GatePrincipal),
    Challenge(GateChallenge),
    Denied,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateEvaluation {
    Authorized(GatePrincipal),
    Challenge(Vec<GateChallenge>),
    Denied,
    Unavailable,
}

pub fn evaluate_gate_policy(
    mode: GatePolicyMode,
    configured: &[GateDriverId],
    results: &BTreeMap<GateDriverId, GateDriverResult>,
) -> GateEvaluation {
    let mut principals = Vec::new();
    let mut challenges = Vec::new();
    let mut denied = false;
    let mut unavailable = false;
    for driver in configured {
        match results.get(driver) {
            Some(GateDriverResult::Authenticated(principal)) => principals.push(principal.clone()),
            Some(GateDriverResult::Challenge(challenge)) => challenges.push(challenge.clone()),
            Some(GateDriverResult::Denied) => denied = true,
            Some(GateDriverResult::Unavailable) | None => unavailable = true,
        }
    }
    match mode {
        GatePolicyMode::Any => principals.into_iter().next().map_or_else(
            || {
                if !challenges.is_empty() {
                    GateEvaluation::Challenge(challenges)
                } else if unavailable {
                    GateEvaluation::Unavailable
                } else {
                    GateEvaluation::Denied
                }
            },
            GateEvaluation::Authorized,
        ),
        GatePolicyMode::All => {
            if denied {
                GateEvaluation::Denied
            } else if unavailable {
                GateEvaluation::Unavailable
            } else if !challenges.is_empty() {
                GateEvaluation::Challenge(challenges)
            } else {
                merge_principals(principals)
                    .map_or(GateEvaluation::Denied, GateEvaluation::Authorized)
            }
        }
    }
}

fn merge_principals(mut principals: Vec<GatePrincipal>) -> Option<GatePrincipal> {
    let mut principal = principals.pop()?;
    for candidate in principals {
        if candidate.user_id != principal.user_id {
            return None;
        }
        principal
            .authenticated_drivers
            .extend(candidate.authenticated_drivers);
    }
    principal.authenticated_drivers.sort();
    principal.authenticated_drivers.dedup();
    Some(principal)
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordHashRecord {
    pub encoded: String,
    pub parameter_version: u32,
}

impl fmt::Debug for PasswordHashRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordHashRecord")
            .field("encoded", &"[REDACTED]")
            .field("parameter_version", &self.parameter_version)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateUserMetadata {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub normalized_username: String,
    pub display_username: String,
    pub enabled: bool,
    pub password_version: u64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateUserRecord {
    pub metadata: GateUserMetadata,
    pub password_hash: PasswordHashRecord,
    pub failed_attempts: u32,
    pub window_started_at_unix_ms: i64,
    pub locked_until_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateSessionMetadata {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub last_seen_at_unix_ms: i64,
    pub key_version: u32,
    pub revoked_at_unix_ms: Option<i64>,
    pub replaced_by: Option<Uuid>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateSessionClaims {
    pub session_id: Uuid,
    pub policy_id: Uuid,
    pub user_id: Uuid,
    pub password_version: u64,
    pub authenticated_drivers: Vec<GateDriverId>,
    pub issued_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub csrf_binding: String,
}

impl fmt::Debug for GateSessionClaims {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GateSessionClaims")
            .field("session_id", &self.session_id)
            .field("policy_id", &self.policy_id)
            .field("user_id", &self.user_id)
            .field("authenticated_drivers", &self.authenticated_drivers)
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .field("csrf_binding", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateMutationAction {
    AddPolicy,
    RemovePolicy,
    AttachRoute,
    DetachRoute,
    AddUser,
    DisableUser,
    EnableUser,
    ResetPassword,
    RemoveUser,
    RevokeSession,
    RotateSessionKey,
}

impl GateMutationAction {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddPolicy => "add_policy",
            Self::RemovePolicy => "remove_policy",
            Self::AttachRoute => "attach_route",
            Self::DetachRoute => "detach_route",
            Self::AddUser => "add_user",
            Self::DisableUser => "disable_user",
            Self::EnableUser => "enable_user",
            Self::ResetPassword => "reset_password",
            Self::RemoveUser => "remove_user",
            Self::RevokeSession => "revoke_session",
            Self::RotateSessionKey => "rotate_session_key",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateMutationPlan {
    pub schema: u32,
    pub action: GateMutationAction,
    pub policy: String,
    pub project_id: ProjectId,
    pub environment: String,
    pub subject: String,
    pub policy_revision: Option<u64>,
    pub subject_revision: Option<u64>,
    pub ingress_generation: Option<u64>,
    pub requires_hidden_input: bool,
    pub effects: Vec<String>,
    pub plan_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GateAuditRecord {
    pub id: Uuid,
    pub action: GateMutationAction,
    pub project_id: ProjectId,
    pub environment: String,
    pub policy: String,
    pub subject: String,
    pub plan_hash: String,
    pub occurred_at_unix_ms: i64,
}

impl GateMutationPlan {
    pub fn seal(mut self) -> Result<Self, String> {
        self.plan_hash.clear();
        let bytes = serde_json::to_vec(&self)
            .map_err(|error| format!("could not serialize gate mutation plan: {error}"))?;
        self.plan_hash = blake3::hash(&bytes).to_hex().to_string();
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 || self.plan_hash.len() != 64 {
            return Err("gate mutation plan schema or hash is invalid".to_owned());
        }
        let mut canonical = self.clone();
        let supplied = canonical.plan_hash.clone();
        canonical.plan_hash.clear();
        let bytes = serde_json::to_vec(&canonical)
            .map_err(|error| format!("could not serialize gate mutation plan: {error}"))?;
        if blake3::hash(&bytes).to_hex().as_str() != supplied {
            return Err("gate mutation plan changed after review".to_owned());
        }
        Ok(())
    }
}

pub fn normalize_username(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 128 || !trimmed.is_ascii() {
        return Err("usernames must be 1-128 ASCII characters".to_owned());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("usernames must not contain control characters".to_owned());
    }
    Ok(trimmed.to_ascii_lowercase())
}

fn is_resource_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
