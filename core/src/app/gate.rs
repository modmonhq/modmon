use std::collections::BTreeMap;
use std::sync::Arc;

use base64::Engine as _;
use uuid::Uuid;

use crate::app::ports::{
    CapSiteProvisioner, CapTokenVerifier, Clock, GateAuthenticationInput, GateDriver,
    GateIngressPublisher, GateRepository, GateSessionCipher, IngressClaimsReplacement,
    IngressRouteRegistry, PasswordVerifier, SecretCreator, SecretMaterialResolver,
    SecretMetadataInspector, SecretRemover, TargetOperationLock,
};
use crate::domain::error::MmError;
use crate::domain::gate::{
    GateAuditRecord, GateDriverId, GateDriverResult, GateEvaluation, GateMutationAction,
    GateMutationPlan, GatePolicy, GatePolicyMode, GatePolicyName, GatePrincipal,
    GateRouteAttachment, GateSessionClaims, GateSessionMetadata, GateUserMetadata, GateUserRecord,
    PASSWORD_CAP_DRIVER, PasswordHashRecord, PreviousSecretReference, SESSION_CLOCK_SKEW_SECONDS,
    VersionedSecretReferences, evaluate_gate_policy, gate_session_cookie_name, normalize_username,
};
use crate::domain::ingress::{CanonicalMiddleware, IngressRouteClaim, IngressRouteSet};
use crate::domain::secret::{SecretExpectedState, SecretMaterial, SecretReference, SecretScope};
use crate::domain::secret_contract::StorageProviderId;
use crate::domain::target::TargetId;
use crate::domain::workspace::ProjectId;

mod policy;
mod routes;
mod sessions;
mod users;

#[derive(Clone)]
pub struct GateManagerDependencies {
    pub repository: Arc<dyn GateRepository>,
    pub routes: Arc<dyn IngressRouteRegistry>,
    pub ingress: Arc<dyn GateIngressPublisher>,
    pub target_locks: Arc<dyn TargetOperationLock>,
    pub password: Arc<dyn PasswordVerifier>,
    pub sessions: Arc<dyn GateSessionCipher>,
    pub drivers: GateDriverRegistry,
    pub cap_sites: Arc<dyn CapSiteProvisioner>,
    pub secret_inspector: Arc<dyn SecretMetadataInspector>,
    pub secret_resolver: Arc<dyn SecretMaterialResolver>,
    pub secret_creator: Arc<dyn SecretCreator>,
    pub secret_remover: Arc<dyn SecretRemover>,
    pub clock: Arc<dyn Clock>,
}

#[derive(Clone)]
pub struct GateManager {
    dependencies: GateManagerDependencies,
}

impl std::fmt::Debug for GateManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GateManager")
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct GateDriverRegistry {
    drivers: Arc<BTreeMap<GateDriverId, Arc<dyn GateDriver>>>,
}

impl std::fmt::Debug for GateDriverRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GateDriverRegistry")
            .field("drivers", &self.drivers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl GateDriverRegistry {
    pub fn new(drivers: Vec<Arc<dyn GateDriver>>) -> Result<Self, MmError> {
        let mut registered = BTreeMap::new();
        for driver in drivers {
            let id = driver.id().clone();
            if registered.insert(id.clone(), driver).is_some() {
                return Err(MmError::State {
                    message: format!("gate driver '{}' is registered more than once", id.as_str()),
                });
            }
        }
        if registered.is_empty() {
            return Err(MmError::State {
                message: "at least one gate driver must be registered".to_owned(),
            });
        }
        Ok(Self {
            drivers: Arc::new(registered),
        })
    }

    pub async fn authenticate(
        &self,
        policy: &GatePolicy,
        input: &GateAuthenticationInput<'_>,
    ) -> Result<GateEvaluation, MmError> {
        let mut results = BTreeMap::new();
        for id in &policy.drivers {
            let result = match self.drivers.get(id) {
                Some(driver) => driver.authenticate(policy, input).await?,
                None => GateDriverResult::Unavailable,
            };
            results.insert(id.clone(), result);
        }
        Ok(evaluate_gate_policy(policy.mode, &policy.drivers, &results))
    }
}

#[derive(Clone)]
pub struct PasswordCapGateDriver {
    id: GateDriverId,
    repository: Arc<dyn GateRepository>,
    password: Arc<dyn PasswordVerifier>,
    cap: Arc<dyn CapTokenVerifier>,
    secret_resolver: Arc<dyn SecretMaterialResolver>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for PasswordCapGateDriver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PasswordCapGateDriver")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl PasswordCapGateDriver {
    pub fn new(
        repository: Arc<dyn GateRepository>,
        password: Arc<dyn PasswordVerifier>,
        cap: Arc<dyn CapTokenVerifier>,
        secret_resolver: Arc<dyn SecretMaterialResolver>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, MmError> {
        Ok(Self {
            id: GateDriverId::new(PASSWORD_CAP_DRIVER).map_err(secret_error)?,
            repository,
            password,
            cap,
            secret_resolver,
            clock,
        })
    }
}

#[async_trait::async_trait]
impl GateDriver for PasswordCapGateDriver {
    fn id(&self) -> &GateDriverId {
        &self.id
    }

    async fn authenticate(
        &self,
        policy: &GatePolicy,
        input: &GateAuthenticationInput<'_>,
    ) -> Result<GateDriverResult, MmError> {
        let now = self.clock.now_unix_ms();
        let cap_secret = resolve_policy_secret(
            self.secret_resolver.as_ref(),
            policy,
            &policy.cap_site_secret,
        )
        .await?;
        if !self
            .cap
            .verify(&policy.cap_site_key, &cap_secret, input.cap_token)
            .await?
        {
            return Ok(GateDriverResult::Denied);
        }
        if !self.repository.consume_cap_token(
            policy.id,
            &token_digest(input.cap_token),
            now,
            now.saturating_add(10 * 60 * 1000),
        )? {
            return Ok(GateDriverResult::Denied);
        }
        let normalized = match normalize_username(input.username) {
            Ok(value) => value,
            Err(_) => return Ok(GateDriverResult::Denied),
        };
        let user = self.repository.user(policy.id, &normalized)?;
        let pepper = resolve_policy_secret(
            self.secret_resolver.as_ref(),
            policy,
            &policy.password_pepper,
        )
        .await?;
        let (valid, user) = if let Some(user) = user {
            let locked = user.locked_until_unix_ms.is_some_and(|until| until > now);
            let valid = self
                .password
                .verify(input.password, &pepper, &user.password_hash)?
                && user.metadata.enabled
                && !locked;
            (valid, Some(user))
        } else {
            let dummy = dummy_hash()?;
            self.password.verify(input.password, &pepper, &dummy)?;
            (false, None)
        };
        let Some(mut user) = user else {
            return Ok(GateDriverResult::Denied);
        };
        if !valid {
            self.repository
                .record_authentication_failure(user.metadata.id, now)?;
            return Ok(GateDriverResult::Denied);
        }
        self.repository
            .clear_authentication_failures(user.metadata.id, now)?;
        if self.password.requires_upgrade(&user.password_hash) {
            let expected = user.metadata.password_version;
            user.password_hash = self.password.hash(input.password, &pepper)?;
            user.failed_attempts = 0;
            user.window_started_at_unix_ms = now;
            user.locked_until_unix_ms = None;
            user.metadata.password_version = user
                .metadata
                .password_version
                .checked_add(1)
                .ok_or_else(stale)?;
            user.metadata.updated_at_unix_ms = now;
            self.repository.replace_user(&user, expected)?;
            self.repository
                .revoke_user_sessions(user.metadata.id, now)?;
        }
        Ok(GateDriverResult::Authenticated(GatePrincipal {
            user_id: user.metadata.id,
            normalized_username: user.metadata.normalized_username,
            display_username: user.metadata.display_username,
            authenticated_drivers: vec![self.id.clone()],
        }))
    }
}

pub struct GateLoginRequest {
    pub policy_id: Uuid,
    pub username: String,
    pub password: SecretMaterial,
    pub cap_token: SecretMaterial,
    pub replace_session_id: Option<Uuid>,
}

impl std::fmt::Debug for GateLoginRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GateLoginRequest")
            .field("policy_id", &self.policy_id)
            .field("username", &self.username)
            .field("replace_session_id", &self.replace_session_id)
            .field("password", &"[REDACTED]")
            .field("cap_token", &"[REDACTED]")
            .finish()
    }
}

pub struct GateLoginResult {
    pub principal: GatePrincipal,
    pub session: GateSessionMetadata,
    pub cookie_value: zeroize::Zeroizing<String>,
    pub csrf_token: zeroize::Zeroizing<String>,
}

impl std::fmt::Debug for GateLoginResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GateLoginResult")
            .field("principal", &self.principal)
            .field("session", &self.session)
            .field("cookie_value", &"[REDACTED]")
            .field("csrf_token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateSessionValidation {
    pub principal: GatePrincipal,
    pub session: GateSessionMetadata,
    pub csrf_binding: String,
    pub requires_rotation: bool,
}

pub struct GateSessionRotation {
    pub session: GateSessionMetadata,
    pub cookie_value: zeroize::Zeroizing<String>,
}

impl std::fmt::Debug for GateSessionRotation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GateSessionRotation")
            .field("session", &self.session)
            .field("cookie_value", &"[REDACTED]")
            .finish()
    }
}

fn route_config_digest(
    target_id: TargetId,
    claims: &[crate::domain::ingress::IngressRouteClaim],
    middlewares: &BTreeMap<String, CanonicalMiddleware>,
) -> Result<String, MmError> {
    let local_hosts = claims
        .iter()
        .filter(|claim| claim.tls && crate::domain::ca::is_local_ca_host(&claim.host))
        .map(|claim| claim.host.clone())
        .collect::<std::collections::BTreeSet<_>>();
    crate::app::render_traefik_dynamic_routes(target_id, claims, middlewares, &local_hosts)
        .map(|rendered| rendered.manifest.digest)
        .map_err(|error| MmError::Target {
            message: format!("could not render access-gate routes: {error}"),
        })
}

#[derive(Debug, Clone)]
pub struct AddGatePolicyRequest {
    pub project_id: ProjectId,
    pub environment: String,
    pub name: GatePolicyName,
    pub mode: GatePolicyMode,
    pub session_ttl_seconds: u64,
    pub secret_provider: String,
}

pub struct ApplyGateRouteRequest<'a> {
    pub project_id: ProjectId,
    pub environment: &'a str,
    pub policy_name: &'a str,
    pub target_id: TargetId,
    pub router_name: &'a str,
    pub attach: bool,
    pub reviewed_plan_hash: &'a str,
}

pub enum GateUserMutation {
    Add(SecretMaterial),
    Disable,
    Enable,
    ResetPassword(SecretMaterial),
    Remove,
}

impl GateUserMutation {
    const fn action(&self) -> GateMutationAction {
        match self {
            Self::Add(_) => GateMutationAction::AddUser,
            Self::Disable => GateMutationAction::DisableUser,
            Self::Enable => GateMutationAction::EnableUser,
            Self::ResetPassword(_) => GateMutationAction::ResetPassword,
            Self::Remove => GateMutationAction::RemoveUser,
        }
    }
}

pub struct ApplyGateUserRequest<'a> {
    pub project_id: ProjectId,
    pub environment: &'a str,
    pub policy_name: &'a str,
    pub username: &'a str,
    pub reviewed_plan_hash: &'a str,
    pub mutation: GateUserMutation,
}

struct GateMutationPlanInput<'a> {
    action: GateMutationAction,
    policy: &'a str,
    project_id: ProjectId,
    environment: &'a str,
    subject: String,
    policy_revision: Option<u64>,
    subject_revision: Option<u64>,
    ingress_generation: Option<u64>,
    requires_hidden_input: bool,
    effects: Vec<String>,
}
impl AddGatePolicyRequest {
    fn validate(&self) -> Result<(), MmError> {
        if self.session_ttl_seconds < 60
            || self.session_ttl_seconds > crate::domain::gate::MAX_SESSION_TTL_SECONDS
        {
            return Err(MmError::Security {
                message: "gate session duration is outside the supported range".to_owned(),
            });
        }
        if self.secret_provider.is_empty() {
            return Err(MmError::Security {
                message: "gate secret provider is required".to_owned(),
            });
        }
        Ok(())
    }
}

fn plan(input: GateMutationPlanInput<'_>) -> Result<GateMutationPlan, MmError> {
    GateMutationPlan {
        schema: 1,
        action: input.action,
        policy: input.policy.to_owned(),
        project_id: input.project_id,
        environment: input.environment.to_owned(),
        subject: input.subject,
        policy_revision: input.policy_revision,
        subject_revision: input.subject_revision,
        ingress_generation: input.ingress_generation,
        requires_hidden_input: input.requires_hidden_input,
        effects: input.effects,
        plan_hash: String::new(),
    }
    .seal()
    .map_err(|message| MmError::State { message })
}
fn require_review(plan: &GateMutationPlan, reviewed: &str) -> Result<(), MmError> {
    plan.validate()
        .map_err(|message| MmError::State { message })?;
    if plan.plan_hash == reviewed {
        Ok(())
    } else {
        Err(MmError::State {
            message: format!(
                "gate plan changed (expected {reviewed}, current {}); review the new plan",
                plan.plan_hash
            ),
        })
    }
}
fn stale() -> MmError {
    MmError::State {
        message: "gate state changed after the reviewed plan; review a new plan".to_owned(),
    }
}
fn secret_error(message: String) -> MmError {
    MmError::Secret { message }
}
fn authentication_failed() -> MmError {
    MmError::AuthenticationFailed
}
async fn resolve_policy_secret(
    resolver: &dyn SecretMaterialResolver,
    policy: &GatePolicy,
    reference: &SecretReference,
) -> Result<SecretMaterial, MmError> {
    resolver
        .resolve(&SecretScope::new(&policy.environment), reference)
        .await?
        .material
        .ok_or_else(|| MmError::Secret {
            message: "required gate secret is unavailable".to_owned(),
        })
}
fn random_secret() -> Result<SecretMaterial, MmError> {
    SecretMaterial::new(random_token(32)?).map_err(secret_error)
}
fn random_token(length: usize) -> Result<String, MmError> {
    let mut bytes = vec![0_u8; length];
    getrandom::fill(&mut bytes).map_err(|_| MmError::Security {
        message: "secure randomness is unavailable".to_owned(),
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}
fn token_digest(token: &SecretMaterial) -> String {
    use sha2::{Digest as _, Sha256};

    token.expose(|value| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes()))
    })
}
fn validate_new_password(password: &SecretMaterial) -> Result<(), MmError> {
    let valid = password.expose(|value| (12..=1024).contains(&value.len()));
    if valid {
        Ok(())
    } else {
        Err(MmError::Security {
            message: "gate passwords must be between 12 and 1024 bytes".to_owned(),
        })
    }
}
fn dummy_hash() -> Result<PasswordHashRecord, MmError> {
    Ok(PasswordHashRecord { encoded: "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRmb3JkdW1teQ$58gKz9T9qNAyvhiK7LQGmbF5C6BSoM4iPeVEvIwJcM8".to_owned(), parameter_version: crate::domain::gate::ARGON2_VERSION })
}
