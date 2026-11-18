//! The access-gate manager against scripted ports: policies, users,
//! sessions, key rotation, and route attachment, with every failure and
//! rollback path.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use base64::Engine as _;
use mm::app::ports::{
    CapSiteProvisioner, CapTokenVerifier, Clock, GateAuthenticationInput, GateDriver,
    GateIngressPublisher, GateRepository, GateSessionCipher, IngressClaimsReplacement,
    IngressRouteRegistry, PasswordVerifier, ProvisionedCapSite, SecretCreator,
    SecretMaterialResolver, SecretMetadataInspector, SecretRemover, TargetOperationLock,
    TargetOperationLockGuard,
};
use mm::app::*;
use mm::domain::error::MmError;
use mm::domain::gate::{
    DEFAULT_SESSION_TTL_SECONDS, GateDriverResult, GateEvaluation, GatePolicyMode,
};
use mm::domain::gate::{
    GateAuditRecord, GateDriverId, GateMutationAction, GatePolicy, GatePolicyName, GatePrincipal,
    GateRouteAttachment, GateSessionClaims, GateSessionMetadata, GateUserMetadata, GateUserRecord,
    PASSWORD_CAP_DRIVER, PasswordHashRecord, PreviousSecretReference, VersionedSecretReferences,
};
use mm::domain::ingress::{CanonicalMiddleware, IngressRouteClaim, IngressRouteSet};
use mm::domain::ingress::{
    IngressCertificateReference, IngressRouteOwnership, IngressRouteProvenance,
};
use mm::domain::secret::{SecretExpectedState, SecretMaterial, SecretReference, SecretScope};
use mm::domain::secret::{SecretMetadata, SecretResolution, SecretStatus, SecretWriteResult};
use mm::domain::secret_contract::StorageProviderId;
use mm::domain::target::TargetId;
use mm::domain::workspace::ProjectId;
use uuid::Uuid;

/// What the gate manager answers when its state changed after a review.
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

/// The digest an ingress publisher returns for the routes it published:
/// the one of the dynamic configuration rendered from them.
fn route_config_digest(
    target_id: TargetId,
    claims: &[IngressRouteClaim],
    middlewares: &BTreeMap<String, CanonicalMiddleware>,
) -> Result<String, MmError> {
    let local_hosts = claims
        .iter()
        .filter(|claim| claim.tls && mm::domain::ca::is_local_ca_host(&claim.host))
        .map(|claim| claim.host.clone())
        .collect::<BTreeSet<_>>();
    render_traefik_dynamic_routes(target_id, claims, middlewares, &local_hosts)
        .map(|rendered| rendered.manifest.digest)
        .map_err(|error| MmError::Target {
            message: format!("could not render access-gate routes: {error}"),
        })
}

/// The plan a user mutation asks for.
const fn action_of(mutation: &GateUserMutation) -> GateMutationAction {
    match mutation {
        GateUserMutation::Add(_) => GateMutationAction::AddUser,
        GateUserMutation::Disable => GateMutationAction::DisableUser,
        GateUserMutation::Enable => GateMutationAction::EnableUser,
        GateUserMutation::ResetPassword(_) => GateMutationAction::ResetPassword,
        GateUserMutation::Remove => GateMutationAction::RemoveUser,
    }
}

fn test_error(operation: &str) -> MmError {
    MmError::State {
        message: format!("injected {operation} failure"),
    }
}

fn lock<'a, T>(mutex: &'a Mutex<T>, name: &str) -> Result<MutexGuard<'a, T>, MmError> {
    mutex.lock().map_err(|_| MmError::State {
        message: format!("fake {name} lock poisoned"),
    })
}

#[derive(Default)]
struct RepositoryState {
    policies: Vec<GatePolicy>,
    users: Vec<GateUserRecord>,
    sessions: Vec<GateSessionMetadata>,
    attachments: Vec<GateRouteAttachment>,
    audits: Vec<GateAuditRecord>,
    consumed_tokens: BTreeSet<String>,
}

struct FakeRepository {
    state: Mutex<RepositoryState>,
    failures: Mutex<VecDeque<&'static str>>,
    no_effects: Mutex<VecDeque<&'static str>>,
    bump_policy_after_lookup: AtomicBool,
    policy_lookups_before_bump: AtomicI64,
    bump_user_after_lookup: AtomicBool,
    remove_user_after_lookup: AtomicBool,
}

impl Default for FakeRepository {
    fn default() -> Self {
        Self {
            state: Mutex::new(RepositoryState::default()),
            failures: Mutex::new(VecDeque::new()),
            no_effects: Mutex::new(VecDeque::new()),
            bump_policy_after_lookup: AtomicBool::new(false),
            policy_lookups_before_bump: AtomicI64::new(-1),
            bump_user_after_lookup: AtomicBool::new(false),
            remove_user_after_lookup: AtomicBool::new(false),
        }
    }
}

impl FakeRepository {
    fn state(&self) -> Result<MutexGuard<'_, RepositoryState>, MmError> {
        lock(&self.state, "repository")
    }

    fn fail_next(&self, operation: &'static str) -> Result<(), MmError> {
        lock(&self.failures, "repository failures")?.push_back(operation);
        Ok(())
    }

    fn no_effect_next(&self, operation: &'static str) -> Result<(), MmError> {
        lock(&self.no_effects, "repository no-effects")?.push_back(operation);
        Ok(())
    }

    fn check(&self, operation: &'static str) -> Result<(), MmError> {
        let mut failures = lock(&self.failures, "repository failures")?;
        if failures.front() == Some(&operation) {
            failures.pop_front();
            Err(test_error(operation))
        } else {
            Ok(())
        }
    }

    fn no_effect(&self, operation: &'static str) -> Result<bool, MmError> {
        let mut effects = lock(&self.no_effects, "repository no-effects")?;
        if effects.front() == Some(&operation) {
            effects.pop_front();
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl GateRepository for FakeRepository {
    fn policy(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
    ) -> Result<Option<GatePolicy>, MmError> {
        self.check("policy")?;
        let mut state = self.state()?;
        let value = state
            .policies
            .iter()
            .find(|policy| {
                policy.project_id == project_id
                    && policy.environment == environment
                    && policy.name.as_str() == name
            })
            .cloned();
        let delayed_bump = self
            .policy_lookups_before_bump
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                (remaining >= 0).then_some(remaining - 1)
            })
            .is_ok_and(|remaining| remaining == 0);
        if (self.bump_policy_after_lookup.swap(false, Ordering::SeqCst) || delayed_bump)
            && let Some(policy) = state.policies.iter_mut().find(|policy| {
                policy.project_id == project_id
                    && policy.environment == environment
                    && policy.name.as_str() == name
            })
        {
            policy.revision = policy.revision.saturating_add(1);
        }
        Ok(value)
    }

    fn policy_by_id(&self, id: Uuid) -> Result<Option<GatePolicy>, MmError> {
        self.check("policy_by_id")?;
        Ok(self
            .state()?
            .policies
            .iter()
            .find(|policy| policy.id == id)
            .cloned())
    }

    fn list_policies(
        &self,
        project_id: ProjectId,
        environment: &str,
    ) -> Result<Vec<GatePolicy>, MmError> {
        self.check("list_policies")?;
        Ok(self
            .state()?
            .policies
            .iter()
            .filter(|policy| policy.project_id == project_id && policy.environment == environment)
            .cloned()
            .collect())
    }

    fn insert_policy(&self, policy: &GatePolicy) -> Result<(), MmError> {
        self.check("insert_policy")?;
        self.state()?.policies.push(policy.clone());
        Ok(())
    }

    fn replace_policy(&self, policy: &GatePolicy, expected_revision: u64) -> Result<(), MmError> {
        self.check("replace_policy")?;
        let mut state = self.state()?;
        let current = state
            .policies
            .iter_mut()
            .find(|current| current.id == policy.id && current.revision == expected_revision)
            .ok_or_else(stale)?;
        *current = policy.clone();
        Ok(())
    }

    fn remove_policy(&self, id: Uuid, expected_revision: u64) -> Result<bool, MmError> {
        self.check("remove_policy")?;
        if self.no_effect("remove_policy")? {
            return Ok(false);
        }
        let mut state = self.state()?;
        let before = state.policies.len();
        state
            .policies
            .retain(|policy| policy.id != id || policy.revision != expected_revision);
        Ok(state.policies.len() != before)
    }

    fn user(
        &self,
        policy_id: Uuid,
        normalized_username: &str,
    ) -> Result<Option<GateUserRecord>, MmError> {
        self.check("user")?;
        let mut state = self.state()?;
        let value = state
            .users
            .iter()
            .find(|user| {
                user.metadata.policy_id == policy_id
                    && user.metadata.normalized_username == normalized_username
            })
            .cloned();
        if self.bump_user_after_lookup.swap(false, Ordering::SeqCst)
            && let Some(user) = state.users.iter_mut().find(|user| {
                user.metadata.policy_id == policy_id
                    && user.metadata.normalized_username == normalized_username
            })
        {
            user.metadata.password_version = user.metadata.password_version.saturating_add(1);
        }
        if self.remove_user_after_lookup.swap(false, Ordering::SeqCst) {
            state.users.retain(|user| {
                user.metadata.policy_id != policy_id
                    || user.metadata.normalized_username != normalized_username
            });
        }
        Ok(value)
    }

    fn user_by_id(&self, id: Uuid) -> Result<Option<GateUserRecord>, MmError> {
        self.check("user_by_id")?;
        Ok(self
            .state()?
            .users
            .iter()
            .find(|user| user.metadata.id == id)
            .cloned())
    }

    fn list_users(&self, policy_id: Uuid) -> Result<Vec<GateUserMetadata>, MmError> {
        self.check("list_users")?;
        Ok(self
            .state()?
            .users
            .iter()
            .filter(|user| user.metadata.policy_id == policy_id)
            .map(|user| user.metadata.clone())
            .collect())
    }

    fn insert_user(&self, user: &GateUserRecord) -> Result<(), MmError> {
        self.check("insert_user")?;
        self.state()?.users.push(user.clone());
        Ok(())
    }

    fn replace_user(
        &self,
        user: &GateUserRecord,
        expected_password_version: u64,
    ) -> Result<(), MmError> {
        self.check("replace_user")?;
        let mut state = self.state()?;
        let current = state
            .users
            .iter_mut()
            .find(|current| {
                current.metadata.id == user.metadata.id
                    && current.metadata.password_version == expected_password_version
            })
            .ok_or_else(stale)?;
        *current = user.clone();
        Ok(())
    }

    fn remove_user(
        &self,
        policy_id: Uuid,
        normalized_username: &str,
        expected_password_version: u64,
    ) -> Result<bool, MmError> {
        self.check("remove_user")?;
        if self.no_effect("remove_user")? {
            return Ok(false);
        }
        let mut state = self.state()?;
        let before = state.users.len();
        state.users.retain(|user| {
            user.metadata.policy_id != policy_id
                || user.metadata.normalized_username != normalized_username
                || user.metadata.password_version != expected_password_version
        });
        Ok(state.users.len() != before)
    }

    fn record_authentication_failure(&self, user_id: Uuid, _now: i64) -> Result<(), MmError> {
        self.check("record_authentication_failure")?;
        if let Some(user) = self
            .state()?
            .users
            .iter_mut()
            .find(|user| user.metadata.id == user_id)
        {
            user.failed_attempts = user.failed_attempts.saturating_add(1);
        }
        Ok(())
    }

    fn clear_authentication_failures(&self, user_id: Uuid, _now: i64) -> Result<(), MmError> {
        self.check("clear_authentication_failures")?;
        if let Some(user) = self
            .state()?
            .users
            .iter_mut()
            .find(|user| user.metadata.id == user_id)
        {
            user.failed_attempts = 0;
            user.locked_until_unix_ms = None;
        }
        Ok(())
    }

    fn insert_session(&self, session: &GateSessionMetadata) -> Result<(), MmError> {
        self.check("insert_session")?;
        self.state()?.sessions.push(session.clone());
        Ok(())
    }

    fn insert_session_replacing(
        &self,
        session: &GateSessionMetadata,
        replaced_session_id: Uuid,
        now: i64,
    ) -> Result<(), MmError> {
        self.check("insert_session_replacing")?;
        let mut state = self.state()?;
        let replaced = state
            .sessions
            .iter_mut()
            .find(|value| {
                value.id == replaced_session_id
                    && value.policy_id == session.policy_id
                    && value.revoked_at_unix_ms.is_none()
            })
            .ok_or_else(stale)?;
        replaced.revoked_at_unix_ms = Some(now);
        replaced.replaced_by = Some(session.id);
        state.sessions.push(session.clone());
        Ok(())
    }

    fn session(&self, id: Uuid) -> Result<Option<GateSessionMetadata>, MmError> {
        self.check("session")?;
        Ok(self
            .state()?
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned())
    }

    fn list_sessions(
        &self,
        policy_id: Uuid,
        now: i64,
    ) -> Result<Vec<GateSessionMetadata>, MmError> {
        self.check("list_sessions")?;
        Ok(self
            .state()?
            .sessions
            .iter()
            .filter(|session| session.policy_id == policy_id && session.expires_at_unix_ms >= now)
            .cloned()
            .collect())
    }

    fn revoke_session(
        &self,
        id: Uuid,
        now: i64,
        replaced_by: Option<Uuid>,
    ) -> Result<bool, MmError> {
        self.check("revoke_session")?;
        if self.no_effect("revoke_session")? {
            return Ok(false);
        }
        let mut state = self.state()?;
        let Some(session) = state
            .sessions
            .iter_mut()
            .find(|session| session.id == id && session.revoked_at_unix_ms.is_none())
        else {
            return Ok(false);
        };
        session.revoked_at_unix_ms = Some(now);
        session.replaced_by = replaced_by;
        Ok(true)
    }

    fn revoke_user_sessions(&self, user_id: Uuid, now: i64) -> Result<u64, MmError> {
        self.check("revoke_user_sessions")?;
        let mut changed = 0;
        for session in &mut self.state()?.sessions {
            if session.user_id == user_id && session.revoked_at_unix_ms.is_none() {
                session.revoked_at_unix_ms = Some(now);
                changed += 1;
            }
        }
        Ok(changed)
    }

    fn attachment(
        &self,
        target_id: TargetId,
        router_name: &str,
    ) -> Result<Option<GateRouteAttachment>, MmError> {
        self.check("attachment")?;
        Ok(self
            .state()?
            .attachments
            .iter()
            .find(|attachment| {
                attachment.target_id == target_id && attachment.router_name == router_name
            })
            .cloned())
    }

    fn list_attachments(&self, policy_id: Uuid) -> Result<Vec<GateRouteAttachment>, MmError> {
        self.check("list_attachments")?;
        Ok(self
            .state()?
            .attachments
            .iter()
            .filter(|attachment| attachment.policy_id == policy_id)
            .cloned()
            .collect())
    }

    fn insert_attachment(&self, attachment: &GateRouteAttachment) -> Result<(), MmError> {
        self.check("insert_attachment")?;
        self.state()?.attachments.push(attachment.clone());
        Ok(())
    }

    fn remove_attachment(
        &self,
        target_id: TargetId,
        router_name: &str,
        policy_id: Uuid,
    ) -> Result<bool, MmError> {
        self.check("remove_attachment")?;
        if self.no_effect("remove_attachment")? {
            return Ok(false);
        }
        let mut state = self.state()?;
        let before = state.attachments.len();
        state.attachments.retain(|attachment| {
            attachment.target_id != target_id
                || attachment.router_name != router_name
                || attachment.policy_id != policy_id
        });
        Ok(state.attachments.len() != before)
    }

    fn consume_cap_token(
        &self,
        policy_id: Uuid,
        token_digest: &str,
        _now: i64,
        _expires: i64,
    ) -> Result<bool, MmError> {
        self.check("consume_cap_token")?;
        Ok(self
            .state()?
            .consumed_tokens
            .insert(format!("{policy_id}:{token_digest}")))
    }

    fn append_audit(&self, record: &GateAuditRecord) -> Result<(), MmError> {
        self.check("append_audit")?;
        self.state()?.audits.push(record.clone());
        Ok(())
    }
}

struct FakeClock(AtomicI64);

impl Clock for FakeClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakePassword {
    upgrade: AtomicBool,
    fail_hash: AtomicBool,
    fail_verify: AtomicBool,
}

impl PasswordVerifier for FakePassword {
    fn hash(
        &self,
        password: &SecretMaterial,
        pepper: &SecretMaterial,
    ) -> Result<PasswordHashRecord, MmError> {
        if self.fail_hash.swap(false, Ordering::SeqCst) {
            return Err(test_error("password hash"));
        }
        Ok(PasswordHashRecord {
            encoded: format!(
                "{}:{}",
                password.expose(str::to_owned),
                pepper.expose(str::to_owned)
            ),
            parameter_version: 1,
        })
    }

    fn verify(
        &self,
        password: &SecretMaterial,
        pepper: &SecretMaterial,
        record: &PasswordHashRecord,
    ) -> Result<bool, MmError> {
        if self.fail_verify.swap(false, Ordering::SeqCst) {
            return Err(test_error("password verify"));
        }
        Ok(record.encoded
            == format!(
                "{}:{}",
                password.expose(str::to_owned),
                pepper.expose(str::to_owned)
            ))
    }

    fn requires_upgrade(&self, _record: &PasswordHashRecord) -> bool {
        self.upgrade.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakeSecrets {
    values: Mutex<BTreeMap<String, String>>,
    create_results: Mutex<VecDeque<Result<bool, &'static str>>>,
    inspect_failures: Mutex<VecDeque<bool>>,
    unset_failures: Mutex<VecDeque<bool>>,
}

impl FakeSecrets {
    fn put(&self, reference: &SecretReference, value: &str) -> Result<(), MmError> {
        lock(&self.values, "secrets")?.insert(reference.canonical(), value.to_owned());
        Ok(())
    }

    fn contains(&self, reference: &SecretReference) -> Result<bool, MmError> {
        Ok(lock(&self.values, "secrets")?.contains_key(&reference.canonical()))
    }

    fn queue_create(&self, result: Result<bool, &'static str>) -> Result<(), MmError> {
        lock(&self.create_results, "secret create results")?.push_back(result);
        Ok(())
    }

    fn queue_inspect_failure(&self, fail: bool) -> Result<(), MmError> {
        lock(&self.inspect_failures, "secret inspect failures")?.push_back(fail);
        Ok(())
    }

    fn queue_unset_failure(&self, fail: bool) -> Result<(), MmError> {
        lock(&self.unset_failures, "secret unset failures")?.push_back(fail);
        Ok(())
    }
}

#[async_trait::async_trait]
impl SecretCreator for FakeSecrets {
    async fn create(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        _now: i64,
    ) -> Result<SecretWriteResult, MmError> {
        let scripted = lock(&self.create_results, "secret create results")?.pop_front();
        let changed = match scripted {
            Some(Ok(changed)) => changed,
            Some(Err(operation)) => return Err(test_error(operation)),
            None => true,
        };
        if changed {
            self.put(reference, &material.expose(str::to_owned))?;
        }
        Ok(SecretWriteResult {
            revision: "revision-1".to_owned(),
            changed,
        })
    }
}

#[async_trait::async_trait]
impl SecretMaterialResolver for FakeSecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        let value = lock(&self.values, "secrets")?
            .get(&reference.canonical())
            .cloned();
        Ok(SecretResolution {
            metadata: secret_metadata(value.is_some()),
            material: value
                .map(SecretMaterial::new)
                .transpose()
                .map_err(secret_error)?,
        })
    }
}

#[async_trait::async_trait]
impl SecretMetadataInspector for FakeSecrets {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        if lock(&self.inspect_failures, "secret inspect failures")?
            .pop_front()
            .unwrap_or(false)
        {
            return Err(test_error("secret inspect"));
        }
        Ok(secret_metadata(self.contains(reference)?))
    }
}

#[async_trait::async_trait]
impl SecretRemover for FakeSecrets {
    async fn unset(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
        _expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        if lock(&self.unset_failures, "secret unset failures")?
            .pop_front()
            .unwrap_or(false)
        {
            return Err(test_error("secret unset"));
        }
        Ok(lock(&self.values, "secrets")?
            .remove(&reference.canonical())
            .is_some())
    }
}

fn secret_metadata(present: bool) -> SecretMetadata {
    SecretMetadata {
        status: if present {
            SecretStatus::Set
        } else {
            SecretStatus::Unset
        },
        revision: present.then(|| "revision-1".to_owned()),
        created_at_unix_ms: present.then_some(1),
        candidate_revision: None,
    }
}

#[derive(Default)]
struct FakeCap {
    create_failure: AtomicBool,
    remove_failures: Mutex<VecDeque<bool>>,
}

#[async_trait::async_trait]
impl CapTokenVerifier for FakeCap {
    async fn verify(
        &self,
        _site_key: &str,
        _site_secret: &SecretMaterial,
        token: &SecretMaterial,
    ) -> Result<bool, MmError> {
        token.expose(|value| match value {
            "error" => Err(test_error("cap verify")),
            "denied" => Ok(false),
            _ => Ok(true),
        })
    }
}

#[async_trait::async_trait]
impl CapSiteProvisioner for FakeCap {
    async fn create(&self, name: &str) -> Result<ProvisionedCapSite, MmError> {
        if self.create_failure.swap(false, Ordering::SeqCst) {
            return Err(test_error("cap create"));
        }
        Ok(ProvisionedCapSite {
            site_key: format!("site-{name}"),
            site_secret: material("cap-secret")?,
        })
    }

    async fn remove(&self, _site_key: &str) -> Result<(), MmError> {
        if lock(&self.remove_failures, "cap remove failures")?
            .pop_front()
            .unwrap_or(false)
        {
            Err(test_error("cap remove"))
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct FakeSessionCipher {
    fail_seal: AtomicBool,
    fail_open: AtomicBool,
}

impl FakeSessionCipher {
    fn cookie(claims: &GateSessionClaims, version: u32) -> Result<String, MmError> {
        let claims = serde_json::to_vec(claims).map_err(|error| MmError::State {
            message: error.to_string(),
        })?;
        Ok(format!(
            "gate.{version}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims)
        ))
    }
}

impl GateSessionCipher for FakeSessionCipher {
    fn seal(
        &self,
        claims: &GateSessionClaims,
        key_version: u32,
        _key: &SecretMaterial,
    ) -> Result<String, MmError> {
        if self.fail_seal.swap(false, Ordering::SeqCst) {
            Err(test_error("session seal"))
        } else {
            Self::cookie(claims, key_version)
        }
    }

    fn open(
        &self,
        value: &str,
        _key_version: u32,
        _key: &SecretMaterial,
    ) -> Result<GateSessionClaims, MmError> {
        if self.fail_open.swap(false, Ordering::SeqCst) {
            return Err(test_error("session open"));
        }
        let encoded = value.split('.').nth(2).ok_or_else(authentication_failed)?;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| authentication_failed())?;
        serde_json::from_slice(&bytes).map_err(|_| authentication_failed())
    }
}

#[derive(Default)]
struct FakeTargetLock {
    fail: AtomicBool,
}

struct FakeTargetLockGuard;

impl TargetOperationLockGuard for FakeTargetLockGuard {}

impl TargetOperationLock for FakeTargetLock {
    fn acquire(&self, _target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        if self.fail.swap(false, Ordering::SeqCst) {
            Err(test_error("target lock"))
        } else {
            Ok(Box::new(FakeTargetLockGuard))
        }
    }
}

#[derive(Default)]
struct FakeRoutes {
    sets: Mutex<BTreeMap<TargetId, IngressRouteSet>>,
    replace_failures: Mutex<VecDeque<bool>>,
    fail_route_set: AtomicBool,
    clear_claims_after_route_set: AtomicBool,
    bump_after_route_set: AtomicBool,
}

impl FakeRoutes {
    fn put(&self, routes: IngressRouteSet) -> Result<(), MmError> {
        lock(&self.sets, "routes")?.insert(routes.target_id, routes);
        Ok(())
    }

    fn queue_replace_failure(&self, fail: bool) -> Result<(), MmError> {
        lock(&self.replace_failures, "route failures")?.push_back(fail);
        Ok(())
    }
}

impl IngressRouteRegistry for FakeRoutes {
    fn route_set(&self, target_id: TargetId) -> Result<IngressRouteSet, MmError> {
        if self.fail_route_set.swap(false, Ordering::SeqCst) {
            return Err(test_error("route set"));
        }
        let mut sets = lock(&self.sets, "routes")?;
        let value = sets.get(&target_id).cloned().unwrap_or(IngressRouteSet {
            target_id,
            generation: 0,
            config_digest: None,
            claims: Vec::new(),
            middlewares: BTreeMap::new(),
        });
        if self
            .clear_claims_after_route_set
            .swap(false, Ordering::SeqCst)
            && let Some(routes) = sets.get_mut(&target_id)
        {
            routes.claims.clear();
        }
        if self.bump_after_route_set.swap(false, Ordering::SeqCst)
            && let Some(routes) = sets.get_mut(&target_id)
        {
            routes.generation = routes.generation.saturating_add(1);
        }
        Ok(value)
    }

    fn conflicts(
        &self,
        _proposed: &[IngressRouteClaim],
    ) -> Result<Vec<mm::domain::ingress::IngressRouteConflict>, MmError> {
        Ok(Vec::new())
    }

    fn replace_owner_claims_cas(
        &self,
        replacement: IngressClaimsReplacement<'_>,
    ) -> Result<IngressRouteSet, MmError> {
        if lock(&self.replace_failures, "route failures")?
            .pop_front()
            .unwrap_or(false)
        {
            return Err(test_error("route replace"));
        }
        let mut sets = lock(&self.sets, "routes")?;
        let current = sets.get(&replacement.target_id);
        let generation = current.map_or(0, |routes| routes.generation);
        if generation != replacement.expected_generation {
            return Err(stale());
        }
        let routes = IngressRouteSet {
            target_id: replacement.target_id,
            generation: generation.saturating_add(1),
            config_digest: replacement.config_digest.map(str::to_owned),
            claims: replacement.claims.to_vec(),
            middlewares: replacement.middlewares.clone(),
        };
        sets.insert(replacement.target_id, routes.clone());
        Ok(routes)
    }

    fn replace_owner_claims(
        &self,
        target_id: TargetId,
        _project_id: ProjectId,
        _environment: &str,
        claims: &[IngressRouteClaim],
        middlewares: &BTreeMap<String, CanonicalMiddleware>,
        _now: i64,
    ) -> Result<(), MmError> {
        let generation = self.route_set(target_id)?.generation.saturating_add(1);
        self.put(IngressRouteSet {
            target_id,
            generation,
            config_digest: None,
            claims: claims.to_vec(),
            middlewares: middlewares.clone(),
        })
    }
}

enum PublishResult {
    Valid,
    WrongDigest,
    Failure,
}

#[derive(Default)]
struct FakeIngress {
    results: Mutex<VecDeque<PublishResult>>,
    publications: Mutex<Vec<IngressRouteSet>>,
}

impl FakeIngress {
    fn queue(&self, result: PublishResult) -> Result<(), MmError> {
        lock(&self.results, "ingress results")?.push_back(result);
        Ok(())
    }
}

#[async_trait::async_trait]
impl GateIngressPublisher for FakeIngress {
    async fn publish(
        &self,
        target_id: TargetId,
        claims: &[IngressRouteClaim],
        middlewares: &BTreeMap<String, CanonicalMiddleware>,
    ) -> Result<String, MmError> {
        lock(&self.publications, "ingress publications")?.push(IngressRouteSet {
            target_id,
            generation: 0,
            config_digest: None,
            claims: claims.to_vec(),
            middlewares: middlewares.clone(),
        });
        match lock(&self.results, "ingress results")?
            .pop_front()
            .unwrap_or(PublishResult::Valid)
        {
            PublishResult::Valid => route_config_digest(target_id, claims, middlewares),
            PublishResult::WrongDigest => Ok("wrong-digest".to_owned()),
            PublishResult::Failure => Err(test_error("ingress publish")),
        }
    }
}

struct Fixture {
    manager: GateManager,
    /// What the manager was built from.
    dependencies: GateManagerDependencies,
    repository: Arc<FakeRepository>,
    routes: Arc<FakeRoutes>,
    ingress: Arc<FakeIngress>,
    target_locks: Arc<FakeTargetLock>,
    password: Arc<FakePassword>,
    sessions: Arc<FakeSessionCipher>,
    cap: Arc<FakeCap>,
    secrets: Arc<FakeSecrets>,
    clock: Arc<FakeClock>,
    project_id: ProjectId,
}

impl Fixture {
    fn new() -> Result<Self, MmError> {
        let repository = Arc::new(FakeRepository::default());
        let routes = Arc::new(FakeRoutes::default());
        let ingress = Arc::new(FakeIngress::default());
        let target_locks = Arc::new(FakeTargetLock::default());
        let password = Arc::new(FakePassword::default());
        let sessions = Arc::new(FakeSessionCipher::default());
        let cap = Arc::new(FakeCap::default());
        let secrets = Arc::new(FakeSecrets::default());
        let clock = Arc::new(FakeClock(AtomicI64::new(1_700_000_000_000)));
        let driver = Arc::new(PasswordCapGateDriver::new(
            repository.clone(),
            password.clone(),
            cap.clone(),
            secrets.clone(),
            clock.clone(),
        )?);
        let dependencies = GateManagerDependencies {
            repository: repository.clone(),
            routes: routes.clone(),
            ingress: ingress.clone(),
            target_locks: target_locks.clone(),
            password: password.clone(),
            sessions: sessions.clone(),
            drivers: GateDriverRegistry::new(vec![driver])?,
            cap_sites: cap.clone(),
            secret_inspector: secrets.clone(),
            secret_resolver: secrets.clone(),
            secret_creator: secrets.clone(),
            secret_remover: secrets.clone(),
            clock: clock.clone(),
        };
        let manager = GateManager::new(dependencies.clone());
        Ok(Self {
            manager,
            dependencies,
            repository,
            routes,
            ingress,
            target_locks,
            password,
            sessions,
            cap,
            secrets,
            clock,
            project_id: ProjectId(Uuid::new_v4()),
        })
    }

    fn policy(&self, name: &str) -> Result<GatePolicy, MmError> {
        let provider = StorageProviderId::new("age").map_err(secret_error)?;
        let reference = |name| SecretReference::new(provider.clone(), name).map_err(secret_error);
        Ok(GatePolicy {
            id: Uuid::new_v4(),
            name: GatePolicyName::new(name).map_err(secret_error)?,
            project_id: self.project_id,
            environment: "development".to_owned(),
            mode: GatePolicyMode::Any,
            drivers: vec![GateDriverId::new(PASSWORD_CAP_DRIVER).map_err(secret_error)?],
            session_ttl_seconds: DEFAULT_SESSION_TTL_SECONDS,
            secret_provider: "age".to_owned(),
            password_pepper: reference(&format!("gate/{name}/pepper"))?,
            session_keys: VersionedSecretReferences {
                current_version: 1,
                current: reference(&format!("gate/{name}/session/1"))?,
                previous: None,
            },
            cap_site_key: format!("site-{name}"),
            cap_site_secret: reference(&format!("gate/{name}/cap"))?,
            revision: 1,
            created_at_unix_ms: self.clock.now_unix_ms(),
            updated_at_unix_ms: self.clock.now_unix_ms(),
        })
    }

    fn insert_policy(&self, policy: &GatePolicy) -> Result<(), MmError> {
        self.repository.insert_policy(policy)?;
        self.secrets.put(&policy.password_pepper, "pepper")?;
        self.secrets
            .put(&policy.session_keys.current, "session-key")?;
        self.secrets.put(&policy.cap_site_secret, "cap-secret")
    }

    fn user(&self, policy: &GatePolicy, name: &str) -> Result<GateUserRecord, MmError> {
        let password = material("correct-password")?;
        let pepper = material("pepper")?;
        Ok(GateUserRecord {
            metadata: GateUserMetadata {
                id: Uuid::new_v4(),
                policy_id: policy.id,
                normalized_username: name.to_owned(),
                display_username: name.to_owned(),
                enabled: true,
                password_version: 1,
                created_at_unix_ms: self.clock.now_unix_ms(),
                updated_at_unix_ms: self.clock.now_unix_ms(),
            },
            password_hash: self.password.hash(&password, &pepper)?,
            failed_attempts: 0,
            window_started_at_unix_ms: self.clock.now_unix_ms(),
            locked_until_unix_ms: None,
        })
    }

    fn insert_user(&self, user: &GateUserRecord) -> Result<(), MmError> {
        self.repository.insert_user(user)
    }

    fn route(&self, target_id: TargetId) -> IngressRouteClaim {
        IngressRouteClaim {
            target: "local".to_owned(),
            target_id,
            project_id: self.project_id,
            project_name: "example".to_owned(),
            environment: "development".to_owned(),
            stack: "web".to_owned(),
            route: "app".to_owned(),
            host: "app.example.test".to_owned(),
            path_prefix: "/".to_owned(),
            service: "app".to_owned(),
            port: 8080,
            tls: true,
            provenance: IngressRouteProvenance::ExplicitConfig,
            certificate_reference: IngressCertificateReference::LocalCa {
                host: "app.example.test".to_owned(),
            },
            ownership: IngressRouteOwnership::Project {
                project_id: self.project_id,
                environment: "development".to_owned(),
                stack: "web".to_owned(),
                service: "app".to_owned(),
            },
            priority: 1,
            router_name: "mm-example-app".to_owned(),
            backend_alias: "app".to_owned(),
            artifact_hash: "a".repeat(64),
            middlewares: Vec::new(),
        }
    }

    fn insert_route(&self, claim: IngressRouteClaim) -> Result<(), MmError> {
        self.routes.put(IngressRouteSet {
            target_id: claim.target_id,
            generation: 1,
            config_digest: Some("previous-digest".to_owned()),
            claims: vec![claim],
            middlewares: BTreeMap::new(),
        })
    }

    fn session(
        &self,
        policy: &GatePolicy,
        user: &GateUserRecord,
        key_version: u32,
    ) -> GateSessionMetadata {
        GateSessionMetadata {
            id: Uuid::new_v4(),
            policy_id: policy.id,
            user_id: user.metadata.id,
            username: user.metadata.display_username.clone(),
            issued_at_unix_ms: self.clock.now_unix_ms(),
            expires_at_unix_ms: self.clock.now_unix_ms() + 60_000,
            last_seen_at_unix_ms: self.clock.now_unix_ms(),
            key_version,
            revoked_at_unix_ms: None,
            replaced_by: None,
        }
    }
}

fn material(value: &str) -> Result<SecretMaterial, MmError> {
    SecretMaterial::new(value.to_owned()).map_err(secret_error)
}

fn add_policy_request(project_id: ProjectId, name: &str) -> Result<AddGatePolicyRequest, MmError> {
    Ok(AddGatePolicyRequest {
        project_id,
        environment: "development".to_owned(),
        name: GatePolicyName::new(name).map_err(secret_error)?,
        mode: GatePolicyMode::Any,
        session_ttl_seconds: DEFAULT_SESSION_TTL_SECONDS,
        secret_provider: "age".to_owned(),
    })
}

fn session_claims(
    policy: &GatePolicy,
    user: &GateUserRecord,
    session: &GateSessionMetadata,
) -> Result<GateSessionClaims, MmError> {
    Ok(GateSessionClaims {
        session_id: session.id,
        policy_id: policy.id,
        user_id: user.metadata.id,
        password_version: user.metadata.password_version,
        authenticated_drivers: vec![GateDriverId::new(PASSWORD_CAP_DRIVER).map_err(secret_error)?],
        issued_at_unix_ms: session.issued_at_unix_ms,
        expires_at_unix_ms: session.expires_at_unix_ms,
        csrf_binding: "csrf-binding".to_owned(),
    })
}

fn login_request(
    policy_id: Uuid,
    token: &str,
    replace_session_id: Option<Uuid>,
) -> Result<GateLoginRequest, MmError> {
    Ok(GateLoginRequest {
        policy_id,
        username: "alice".to_owned(),
        password: material("correct-password")?,
        cap_token: material(token)?,
        replace_session_id,
    })
}

fn route_request<'a>(
    fixture: &Fixture,
    claim: &'a IngressRouteClaim,
    attach: bool,
    reviewed_plan_hash: &'a str,
) -> ApplyGateRouteRequest<'a> {
    ApplyGateRouteRequest {
        project_id: fixture.project_id,
        environment: "development",
        policy_name: "internal",
        target_id: claim.target_id,
        router_name: &claim.router_name,
        attach,
        reviewed_plan_hash,
    }
}

fn route_fixture() -> Result<(Fixture, GatePolicy, IngressRouteClaim), MmError> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let claim = fixture.route(TargetId(Uuid::new_v4()));
    fixture.insert_route(claim.clone())?;
    Ok((fixture, policy, claim))
}

async fn attach_route(
    fixture: &Fixture,
    claim: &IngressRouteClaim,
) -> Result<GateRouteAttachment, MmError> {
    let plan = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        claim.target_id,
        &claim.router_name,
        true,
    )?;
    fixture
        .manager
        .apply_attach_reviewed(route_request(fixture, claim, true, &plan.plan_hash))
        .await
}

struct FakeDriver {
    id: GateDriverId,
    result: GateDriverResult,
}

struct ErrorDriver(GateDriverId);

#[async_trait::async_trait]
impl GateDriver for ErrorDriver {
    fn id(&self) -> &GateDriverId {
        &self.0
    }

    async fn authenticate(
        &self,
        _policy: &GatePolicy,
        _input: &GateAuthenticationInput<'_>,
    ) -> Result<GateDriverResult, MmError> {
        Err(test_error("driver authenticate"))
    }
}

#[async_trait::async_trait]
impl GateDriver for FakeDriver {
    fn id(&self) -> &GateDriverId {
        &self.id
    }

    async fn authenticate(
        &self,
        _policy: &GatePolicy,
        _input: &GateAuthenticationInput<'_>,
    ) -> Result<GateDriverResult, MmError> {
        Ok(self.result.clone())
    }
}

#[tokio::test]
async fn adding_a_fake_driver_does_not_change_evaluation_logic()
-> Result<(), Box<dyn std::error::Error>> {
    let principal = GatePrincipal {
        user_id: Uuid::nil(),
        normalized_username: "a".to_owned(),
        display_username: "A".to_owned(),
        authenticated_drivers: vec![],
    };
    let future = GateDriverId::new("future-oidc")?;
    let password = GateDriverId::new(PASSWORD_CAP_DRIVER)?;
    let drivers: Vec<Arc<dyn GateDriver>> = vec![
        Arc::new(FakeDriver {
            id: future.clone(),
            result: GateDriverResult::Authenticated(principal),
        }),
        Arc::new(FakeDriver {
            id: password.clone(),
            result: GateDriverResult::Denied,
        }),
    ];
    let registry = GateDriverRegistry::new(drivers)?;
    let provider = StorageProviderId::new("age")?;
    let reference = |name| SecretReference::new(provider.clone(), name);
    let mut policy = GatePolicy {
        id: Uuid::new_v4(),
        name: GatePolicyName::new("test")?,
        project_id: ProjectId(Uuid::new_v4()),
        environment: "development".to_owned(),
        mode: GatePolicyMode::Any,
        drivers: [future, password]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        session_ttl_seconds: 60,
        secret_provider: "age".to_owned(),
        password_pepper: reference("pepper")?,
        session_keys: VersionedSecretReferences {
            current_version: 1,
            current: reference("session")?,
            previous: None,
        },
        cap_site_key: "site".to_owned(),
        cap_site_secret: reference("cap")?,
        revision: 1,
        created_at_unix_ms: 0,
        updated_at_unix_ms: 0,
    };
    let username = "a".to_owned();
    let password_material = SecretMaterial::new("password".to_owned())?;
    let cap_token = SecretMaterial::new("token".to_owned())?;
    let input = GateAuthenticationInput {
        username: &username,
        password: &password_material,
        cap_token: &cap_token,
    };
    assert!(matches!(
        registry.authenticate(&policy, &input).await?,
        GateEvaluation::Authorized(_)
    ));
    policy.mode = GatePolicyMode::All;
    assert!(matches!(
        registry.authenticate(&policy, &input).await?,
        GateEvaluation::Denied
    ));
    Ok(())
}

#[tokio::test]
async fn registry_rejects_invalid_sets_and_marks_missing_drivers_unavailable()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(GateDriverRegistry::new(Vec::new()).is_err());
    let id = GateDriverId::new("driver")?;
    let first: Arc<dyn GateDriver> = Arc::new(FakeDriver {
        id: id.clone(),
        result: GateDriverResult::Denied,
    });
    let second: Arc<dyn GateDriver> = Arc::new(FakeDriver {
        id: id.clone(),
        result: GateDriverResult::Unavailable,
    });
    assert!(GateDriverRegistry::new(vec![first, second]).is_err());

    let fixture = Fixture::new()?;
    let mut policy = fixture.policy("internal")?;
    policy.drivers = vec![GateDriverId::new("unregistered")?];
    let username = "user".to_owned();
    let password = material("correct-password")?;
    let token = material("accepted")?;
    let input = GateAuthenticationInput {
        username: &username,
        password: &password,
        cap_token: &token,
    };
    assert!(matches!(
        fixture
            .dependencies
            .drivers
            .authenticate(&policy, &input)
            .await?,
        GateEvaluation::Unavailable
    ));
    assert!(format!("{:?}", fixture.manager).contains("GateManager"));
    assert!(format!("{:?}", fixture.dependencies.drivers).contains(PASSWORD_CAP_DRIVER));
    let registry = GateDriverRegistry::new(vec![Arc::new(ErrorDriver(GateDriverId::new(
        "error-driver",
    )?))])?;
    policy.drivers = vec![GateDriverId::new("error-driver")?];
    assert!(registry.authenticate(&policy, &input).await.is_err());
    Ok(())
}

#[tokio::test]
async fn password_cap_driver_covers_denial_replay_lock_and_upgrade_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let driver = PasswordCapGateDriver::new(
        fixture.repository.clone(),
        fixture.password.clone(),
        fixture.cap.clone(),
        fixture.secrets.clone(),
        fixture.clock.clone(),
    )?;
    assert_eq!(driver.id().as_str(), PASSWORD_CAP_DRIVER);
    assert!(format!("{driver:?}").contains(PASSWORD_CAP_DRIVER));

    async fn authenticate(
        driver: &PasswordCapGateDriver,
        policy: &GatePolicy,
        username: &str,
        password: &str,
        token: &str,
    ) -> Result<GateDriverResult, MmError> {
        let password = material(password)?;
        let token = material(token)?;
        driver
            .authenticate(
                policy,
                &GateAuthenticationInput {
                    username,
                    password: &password,
                    cap_token: &token,
                },
            )
            .await
    }

    assert!(matches!(
        authenticate(&driver, &policy, "alice", "correct-password", "denied").await?,
        GateDriverResult::Denied
    ));
    assert!(
        authenticate(&driver, &policy, "alice", "correct-password", "error")
            .await
            .is_err()
    );
    assert!(matches!(
        authenticate(&driver, &policy, "alice", "correct-password", "once").await?,
        GateDriverResult::Authenticated(_)
    ));
    assert!(matches!(
        authenticate(&driver, &policy, "alice", "correct-password", "once").await?,
        GateDriverResult::Denied
    ));
    assert!(matches!(
        authenticate(&driver, &policy, "bad name", "correct-password", "bad-name").await?,
        GateDriverResult::Denied
    ));
    assert!(matches!(
        authenticate(&driver, &policy, "missing", "wrong-password", "missing").await?,
        GateDriverResult::Denied
    ));
    assert!(matches!(
        authenticate(&driver, &policy, "alice", "wrong-password", "wrong").await?,
        GateDriverResult::Denied
    ));
    assert_eq!(
        fixture
            .repository
            .user(policy.id, "alice")?
            .ok_or_else(authentication_failed)?
            .failed_attempts,
        1
    );

    {
        let mut state = fixture.repository.state()?;
        let stored = state
            .users
            .iter_mut()
            .find(|value| value.metadata.id == user.metadata.id)
            .ok_or_else(authentication_failed)?;
        stored.locked_until_unix_ms = Some(fixture.clock.now_unix_ms() + 1);
    }
    assert!(matches!(
        authenticate(&driver, &policy, "alice", "correct-password", "locked").await?,
        GateDriverResult::Denied
    ));
    {
        let mut state = fixture.repository.state()?;
        let stored = state
            .users
            .iter_mut()
            .find(|value| value.metadata.id == user.metadata.id)
            .ok_or_else(authentication_failed)?;
        stored.locked_until_unix_ms = None;
        stored.metadata.enabled = false;
    }
    assert!(matches!(
        authenticate(&driver, &policy, "alice", "correct-password", "disabled").await?,
        GateDriverResult::Denied
    ));
    {
        let mut state = fixture.repository.state()?;
        let stored = state
            .users
            .iter_mut()
            .find(|value| value.metadata.id == user.metadata.id)
            .ok_or_else(authentication_failed)?;
        stored.metadata.enabled = true;
    }
    fixture.password.upgrade.store(true, Ordering::SeqCst);
    assert!(matches!(
        authenticate(&driver, &policy, "alice", "correct-password", "upgrade").await?,
        GateDriverResult::Authenticated(_)
    ));
    let upgraded = fixture
        .repository
        .user(policy.id, "alice")?
        .ok_or_else(authentication_failed)?;
    assert_eq!(upgraded.metadata.password_version, 2);
    assert_eq!(upgraded.failed_attempts, 0);

    {
        let mut state = fixture.repository.state()?;
        let stored = state
            .users
            .iter_mut()
            .find(|value| value.metadata.id == user.metadata.id)
            .ok_or_else(authentication_failed)?;
        stored.metadata.password_version = u64::MAX;
    }
    assert!(
        authenticate(&driver, &policy, "alice", "correct-password", "overflow")
            .await
            .is_err()
    );
    let mut missing_secret = policy.clone();
    missing_secret.cap_site_secret =
        SecretReference::new(StorageProviderId::new("age")?, "missing-cap-secret")?;
    assert!(
        authenticate(
            &driver,
            &missing_secret,
            "alice",
            "correct-password",
            "missing-secret"
        )
        .await
        .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn policy_plans_validate_and_creation_rolls_back_partial_resources()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut request = add_policy_request(fixture.project_id, "internal")?;
    request.session_ttl_seconds = 59;
    assert!(matches!(
        fixture.manager.plan_add_policy(&request),
        Err(MmError::Security { .. })
    ));
    request.session_ttl_seconds = mm::domain::gate::MAX_SESSION_TTL_SECONDS + 1;
    assert!(fixture.manager.plan_add_policy(&request).is_err());
    request.session_ttl_seconds = DEFAULT_SESSION_TTL_SECONDS;
    request.secret_provider.clear();
    assert!(fixture.manager.plan_add_policy(&request).is_err());
    request.secret_provider = "age".to_owned();

    let plan = fixture.manager.plan_add_policy(&request)?;
    let mut invalid_provider = request.clone();
    invalid_provider.secret_provider = "Invalid Provider".to_owned();
    assert!(matches!(
        fixture
            .manager
            .add_policy_reviewed(invalid_provider, &plan.plan_hash)
            .await,
        Err(MmError::Security { .. })
    ));
    assert!(
        fixture
            .manager
            .add_policy_reviewed(request.clone(), "wrong-review")
            .await
            .is_err()
    );
    fixture.cap.create_failure.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .add_policy_reviewed(request.clone(), &plan.plan_hash)
            .await
            .is_err()
    );

    fixture.secrets.queue_create(Ok(true))?;
    fixture.secrets.queue_create(Ok(false))?;
    assert!(
        fixture
            .manager
            .add_policy_reviewed(request.clone(), &plan.plan_hash)
            .await
            .is_err()
    );
    assert!(lock(&fixture.secrets.values, "secrets")?.is_empty());

    fixture.secrets.queue_create(Ok(true))?;
    fixture.secrets.queue_create(Err("second create"))?;
    fixture.secrets.queue_unset_failure(true)?;
    lock(&fixture.cap.remove_failures, "cap remove failures")?.push_back(true);
    assert!(matches!(
        fixture
            .manager
            .add_policy_reviewed(request.clone(), &plan.plan_hash)
            .await,
        Err(MmError::OperationFailure { .. })
    ));
    lock(&fixture.secrets.values, "secrets")?.clear();

    fixture.repository.fail_next("insert_policy")?;
    assert!(
        fixture
            .manager
            .add_policy_reviewed(request.clone(), &plan.plan_hash)
            .await
            .is_err()
    );
    assert!(lock(&fixture.secrets.values, "secrets")?.is_empty());

    let policy = fixture
        .manager
        .add_policy_reviewed(request.clone(), &plan.plan_hash)
        .await?;
    assert_eq!(policy.revision, 1);
    assert_eq!(fixture.manager.policy_by_id(policy.id)?, policy);
    assert_eq!(
        fixture
            .manager
            .show(fixture.project_id, "development", Some("internal"))?,
        vec![policy.clone()]
    );
    assert_eq!(
        fixture
            .manager
            .show(fixture.project_id, "development", None)?,
        vec![policy]
    );
    assert!(fixture.manager.plan_add_policy(&request).is_err());
    assert!(
        fixture
            .manager
            .add_policy_reviewed(request, &plan.plan_hash)
            .await
            .is_err()
    );
    assert!(fixture.manager.policy_by_id(Uuid::new_v4()).is_err());
    Ok(())
}

#[tokio::test]
async fn policy_repository_and_audit_failures_propagate() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let request = add_policy_request(fixture.project_id, "internal")?;
    fixture.repository.fail_next("policy")?;
    assert!(fixture.manager.plan_add_policy(&request).is_err());
    let plan = fixture.manager.plan_add_policy(&request)?;
    fixture.repository.fail_next("append_audit")?;
    assert!(
        fixture
            .manager
            .add_policy_reviewed(request, &plan.plan_hash)
            .await
            .is_err()
    );
    assert_eq!(fixture.repository.state()?.policies.len(), 1);

    fixture.repository.fail_next("policy_by_id")?;
    assert!(fixture.manager.policy_by_id(Uuid::new_v4()).is_err());
    fixture.repository.fail_next("policy")?;
    assert!(
        fixture
            .manager
            .plan_remove_policy(fixture.project_id, "development", "internal")
            .is_err()
    );
    fixture.repository.fail_next("list_attachments")?;
    assert!(
        fixture
            .manager
            .plan_remove_policy(fixture.project_id, "development", "internal")
            .is_err()
    );
    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    fixture.repository.fail_next("remove_policy")?;
    assert!(
        fixture
            .manager
            .remove_policy_reviewed(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn policy_removal_cleans_all_secrets_before_reporting_audit_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut policy = fixture.policy("internal")?;
    let previous = SecretReference::new(StorageProviderId::new("age")?, "previous-key")?;
    policy.session_keys.previous = Some(PreviousSecretReference {
        version: 0,
        reference: previous.clone(),
        accept_until_unix_ms: 0,
    });
    fixture.insert_policy(&policy)?;
    fixture.secrets.put(&previous, "previous")?;
    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    fixture.repository.fail_next("append_audit")?;
    assert!(
        fixture
            .manager
            .remove_policy_reviewed(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );
    assert!(fixture.repository.state()?.policies.is_empty());
    assert!(lock(&fixture.secrets.values, "secrets")?.is_empty());
    Ok(())
}

#[tokio::test]
async fn policy_removal_reports_previous_key_cleanup_failure_and_can_complete()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut policy = fixture.policy("internal")?;
    let previous = SecretReference::new(StorageProviderId::new("age")?, "previous-key")?;
    policy.session_keys.previous = Some(PreviousSecretReference {
        version: 0,
        reference: previous.clone(),
        accept_until_unix_ms: 0,
    });
    fixture.insert_policy(&policy)?;
    fixture.secrets.put(&previous, "previous")?;
    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    for _ in 0..3 {
        fixture.secrets.queue_unset_failure(false)?;
    }
    fixture.secrets.queue_unset_failure(true)?;
    assert!(matches!(
        fixture
            .manager
            .remove_policy_reviewed(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await,
        Err(MmError::OperationFailure { .. })
    ));

    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    assert!(
        fixture
            .manager
            .remove_policy_reviewed(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await?
    );
    assert_eq!(fixture.repository.state()?.audits.len(), 1);
    Ok(())
}

#[tokio::test]
async fn policy_removal_is_stale_safe_and_reports_applied_cleanup_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut policy = fixture.policy("internal")?;
    let previous = SecretReference::new(StorageProviderId::new("age")?, "previous-key")?;
    policy.session_keys.previous = Some(PreviousSecretReference {
        version: 0,
        reference: previous.clone(),
        accept_until_unix_ms: fixture.clock.now_unix_ms() - 1,
    });
    fixture.insert_policy(&policy)?;
    fixture.secrets.put(&previous, "previous")?;
    let attachment = GateRouteAttachment {
        policy_id: policy.id,
        target_id: TargetId(Uuid::new_v4()),
        project_id: fixture.project_id,
        environment: "development".to_owned(),
        router_name: "router".to_owned(),
        host: "app.example.test".to_owned(),
        path_prefix: "/".to_owned(),
        middleware_name: "gate".to_owned(),
        attached_at_unix_ms: fixture.clock.now_unix_ms(),
    };
    fixture.repository.insert_attachment(&attachment)?;
    assert!(
        fixture
            .manager
            .plan_remove_policy(fixture.project_id, "development", "internal")
            .is_err()
    );
    fixture.repository.remove_attachment(
        attachment.target_id,
        &attachment.router_name,
        policy.id,
    )?;

    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    fixture
        .repository
        .bump_policy_after_lookup
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .remove_policy_reviewed(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );

    let current = fixture
        .repository
        .policy(fixture.project_id, "development", "internal")?
        .ok_or_else(authentication_failed)?;
    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    fixture.repository.no_effect_next("remove_policy")?;
    assert!(
        fixture
            .manager
            .remove_policy_reviewed(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );

    let plan = fixture
        .manager
        .plan_remove_policy(fixture.project_id, "development", "internal")?;
    lock(&fixture.cap.remove_failures, "cap remove failures")?.push_back(true);
    fixture.secrets.queue_inspect_failure(true)?;
    fixture.secrets.queue_inspect_failure(false)?;
    fixture.secrets.queue_unset_failure(true)?;
    fixture.secrets.queue_inspect_failure(false)?;
    fixture.secrets.queue_unset_failure(false)?;
    fixture.secrets.queue_inspect_failure(true)?;
    let error = fixture
        .manager
        .remove_policy_reviewed(
            fixture.project_id,
            "development",
            "internal",
            &plan.plan_hash,
        )
        .await;
    assert!(matches!(error, Err(MmError::OperationFailure { .. })));
    assert!(fixture.repository.policy_by_id(current.id)?.is_none());
    Ok(())
}

#[tokio::test]
async fn user_mutations_cover_success_failure_stale_and_no_effect_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    assert!(
        fixture
            .manager
            .plan_user(
                GateMutationAction::AddUser,
                fixture.project_id,
                "development",
                "internal",
                "álîce",
            )
            .is_err()
    );
    assert!(matches!(
        fixture.manager.plan_user(
            GateMutationAction::AddPolicy,
            fixture.project_id,
            "development",
            "internal",
            "alice",
        ),
        Err(MmError::Security { .. })
    ));
    assert!(
        fixture
            .manager
            .plan_user(
                GateMutationAction::DisableUser,
                fixture.project_id,
                "development",
                "internal",
                "alice",
            )
            .is_err()
    );

    let add = fixture.manager.plan_user(
        GateMutationAction::AddUser,
        fixture.project_id,
        "development",
        "internal",
        " Alice ",
    )?;
    assert!(add.requires_hidden_input);
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: " Alice ",
                reviewed_plan_hash: &add.plan_hash,
                mutation: GateUserMutation::Add(material("short")?),
            })
            .await
            .is_err()
    );
    let add = fixture.manager.plan_user(
        GateMutationAction::AddUser,
        fixture.project_id,
        "development",
        "internal",
        " Alice ",
    )?;
    let metadata = fixture
        .manager
        .apply_user_reviewed(ApplyGateUserRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            username: " Alice ",
            reviewed_plan_hash: &add.plan_hash,
            mutation: GateUserMutation::Add(material("long-enough-password")?),
        })
        .await?;
    assert_eq!(metadata.normalized_username, "alice");
    assert_eq!(metadata.display_username, "Alice");
    assert_eq!(
        fixture
            .manager
            .list_users(fixture.project_id, "development", "internal")?,
        vec![metadata.clone()]
    );
    assert!(
        fixture
            .manager
            .plan_user(
                GateMutationAction::AddUser,
                fixture.project_id,
                "development",
                "internal",
                "ALICE",
            )
            .is_err()
    );

    let disable = fixture.manager.plan_user(
        GateMutationAction::DisableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture
        .repository
        .remove_user_after_lookup
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &disable.plan_hash,
                mutation: GateUserMutation::Disable,
            })
            .await
            .is_err()
    );
    fixture.insert_user(&fixture.user(&policy, "alice")?)?;

    let disable = fixture.manager.plan_user(
        GateMutationAction::DisableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture
        .repository
        .bump_policy_after_lookup
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &disable.plan_hash,
                mutation: GateUserMutation::Disable,
            })
            .await
            .is_err()
    );

    let disable = fixture.manager.plan_user(
        GateMutationAction::DisableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture
        .repository
        .bump_user_after_lookup
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &disable.plan_hash,
                mutation: GateUserMutation::Disable,
            })
            .await
            .is_err()
    );

    let disable = fixture.manager.plan_user(
        GateMutationAction::DisableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    let disabled = fixture
        .manager
        .apply_user_reviewed(ApplyGateUserRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            username: "alice",
            reviewed_plan_hash: &disable.plan_hash,
            mutation: GateUserMutation::Disable,
        })
        .await?;
    assert!(!disabled.enabled);

    let enable = fixture.manager.plan_user(
        GateMutationAction::EnableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    let enabled = fixture
        .manager
        .apply_user_reviewed(ApplyGateUserRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            username: "alice",
            reviewed_plan_hash: &enable.plan_hash,
            mutation: GateUserMutation::Enable,
        })
        .await?;
    assert!(enabled.enabled);

    let reset = fixture.manager.plan_user(
        GateMutationAction::ResetPassword,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture
        .manager
        .apply_user_reviewed(ApplyGateUserRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            username: "alice",
            reviewed_plan_hash: &reset.plan_hash,
            mutation: GateUserMutation::ResetPassword(material("replacement-password")?),
        })
        .await?;

    let remove = fixture.manager.plan_user(
        GateMutationAction::RemoveUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture.repository.no_effect_next("remove_user")?;
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &remove.plan_hash,
                mutation: GateUserMutation::Remove,
            })
            .await
            .is_err()
    );
    let remove = fixture.manager.plan_user(
        GateMutationAction::RemoveUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture
        .manager
        .apply_user_reviewed(ApplyGateUserRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            username: "alice",
            reviewed_plan_hash: &remove.plan_hash,
            mutation: GateUserMutation::Remove,
        })
        .await?;
    assert!(
        fixture
            .manager
            .list_users(fixture.project_id, "development", "internal")?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn user_version_overflow_and_port_failures_do_not_apply_partial_mutations()
-> Result<(), Box<dyn std::error::Error>> {
    for mutation in [GateUserMutation::Disable, GateUserMutation::Enable] {
        let fixture = Fixture::new()?;
        let policy = fixture.policy("internal")?;
        fixture.insert_policy(&policy)?;
        let mut user = fixture.user(&policy, "alice")?;
        user.metadata.password_version = u64::MAX;
        fixture.insert_user(&user)?;
        let action = action_of(&mutation);
        let plan = fixture.manager.plan_user(
            action,
            fixture.project_id,
            "development",
            "internal",
            "alice",
        )?;
        assert!(
            fixture
                .manager
                .apply_user_reviewed(ApplyGateUserRequest {
                    project_id: fixture.project_id,
                    environment: "development",
                    policy_name: "internal",
                    username: "alice",
                    reviewed_plan_hash: &plan.plan_hash,
                    mutation,
                })
                .await
                .is_err()
        );
    }

    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let disable = fixture.manager.plan_user(
        GateMutationAction::DisableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    fixture.repository.fail_next("replace_user")?;
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &disable.plan_hash,
                mutation: GateUserMutation::Disable,
            })
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn user_password_and_repository_failures_propagate() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let reset = fixture.manager.plan_user(
        GateMutationAction::ResetPassword,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &reset.plan_hash,
                mutation: GateUserMutation::ResetPassword(material("short")?),
            })
            .await
            .is_err()
    );
    lock(&fixture.secrets.values, "secrets")?.remove(&policy.password_pepper.canonical());
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &reset.plan_hash,
                mutation: GateUserMutation::ResetPassword(material("replacement-password")?),
            })
            .await
            .is_err()
    );
    fixture.secrets.put(&policy.password_pepper, "pepper")?;
    fixture.password.fail_hash.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &reset.plan_hash,
                mutation: GateUserMutation::ResetPassword(material("replacement-password")?),
            })
            .await
            .is_err()
    );

    let mut overflow = fixture
        .repository
        .user(policy.id, "alice")?
        .ok_or_else(authentication_failed)?;
    overflow.metadata.password_version = u64::MAX;
    fixture.repository.state()?.users = vec![overflow];
    let reset = fixture.manager.plan_user(
        GateMutationAction::ResetPassword,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &reset.plan_hash,
                mutation: GateUserMutation::ResetPassword(material("replacement-password")?),
            })
            .await
            .is_err()
    );

    fixture.repository.fail_next("list_users")?;
    assert!(
        fixture
            .manager
            .list_users(fixture.project_id, "development", "internal")
            .is_err()
    );
    fixture.repository.fail_next("policy")?;
    assert!(
        fixture
            .manager
            .plan_user(
                GateMutationAction::DisableUser,
                fixture.project_id,
                "development",
                "internal",
                "alice",
            )
            .is_err()
    );
    fixture.repository.fail_next("user")?;
    assert!(
        fixture
            .manager
            .plan_user(
                GateMutationAction::DisableUser,
                fixture.project_id,
                "development",
                "internal",
                "alice",
            )
            .is_err()
    );

    let add_fixture = Fixture::new()?;
    let add_policy = add_fixture.policy("internal")?;
    add_fixture.insert_policy(&add_policy)?;
    let add = add_fixture.manager.plan_user(
        GateMutationAction::AddUser,
        add_fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
    add_fixture.password.fail_hash.store(true, Ordering::SeqCst);
    assert!(
        add_fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: add_fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &add.plan_hash,
                mutation: GateUserMutation::Add(material("long-enough-password")?),
            })
            .await
            .is_err()
    );
    add_fixture.repository.fail_next("insert_user")?;
    assert!(
        add_fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: add_fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &add.plan_hash,
                mutation: GateUserMutation::Add(material("long-enough-password")?),
            })
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn login_fails_closed_and_replacement_rotates_the_session_identifier()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let debug_request = login_request(policy.id, "debug-token", None)?;
    let debug = format!("{debug_request:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("correct-password"));
    assert!(!debug.contains("debug-token"));
    assert!(
        fixture
            .manager
            .login(GateLoginRequest {
                policy_id: Uuid::new_v4(),
                username: "alice".to_owned(),
                password: material("correct-password")?,
                cap_token: material("missing-policy")?,
                replace_session_id: None,
            })
            .await
            .is_err()
    );
    assert!(
        fixture
            .manager
            .login(login_request(policy.id, "denied", None)?)
            .await
            .is_err()
    );
    fixture.sessions.fail_seal.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .login(login_request(policy.id, "seal", None)?)
            .await
            .is_err()
    );
    fixture.repository.fail_next("insert_session")?;
    assert!(
        fixture
            .manager
            .login(login_request(policy.id, "insert", None)?)
            .await
            .is_err()
    );

    let first = fixture
        .manager
        .login(login_request(policy.id, "first", None)?)
        .await?;
    assert!(format!("{first:?}").contains("[REDACTED]"));
    assert!(!format!("{first:?}").contains(first.cookie_value.as_str()));
    let replacement = fixture
        .manager
        .login(login_request(
            policy.id,
            "replacement",
            Some(first.session.id),
        )?)
        .await?;
    assert_ne!(replacement.session.id, first.session.id);
    assert!(
        fixture
            .repository
            .session(first.session.id)?
            .ok_or_else(authentication_failed)?
            .revoked_at_unix_ms
            .is_some()
    );
    assert!(
        fixture
            .manager
            .login(login_request(
                policy.id,
                "stale-replacement",
                Some(first.session.id),
            )?)
            .await
            .is_err()
    );

    let mut missing_key = policy.clone();
    missing_key.session_keys.current =
        SecretReference::new(StorageProviderId::new("age")?, "missing-session-key")?;
    *fixture
        .repository
        .state()?
        .policies
        .iter_mut()
        .find(|stored| stored.id == policy.id)
        .ok_or_else(authentication_failed)? = missing_key;
    assert!(
        fixture
            .manager
            .login(login_request(policy.id, "missing-key", None)?)
            .await
            .is_err()
    );
    Ok(())
}

#[test]
fn gate_helpers_reject_invalid_reviews_and_route_rendering()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut invalid_plan = fixture
        .manager
        .plan_add_policy(&add_policy_request(fixture.project_id, "internal")?)?;
    invalid_plan.plan_hash = "tampered".to_owned();
    assert!(
        invalid_plan.validate().is_err(),
        "a plan whose hash was changed is no reviewed plan"
    );

    let target_id = TargetId(Uuid::new_v4());
    let mut claim = fixture.route(target_id);
    claim.target_id = TargetId(Uuid::new_v4());
    assert!(route_config_digest(target_id, &[claim], &BTreeMap::new()).is_err());
    Ok(())
}

#[tokio::test]
async fn login_rejects_non_authorizing_driver_results_and_invalid_principals()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let mut user = fixture.user(&policy, "alice")?;
    user.metadata.enabled = false;
    fixture.insert_user(&user)?;
    let id = GateDriverId::new(PASSWORD_CAP_DRIVER)?;

    for result in [
        GateDriverResult::Denied,
        GateDriverResult::Unavailable,
        GateDriverResult::Challenge(mm::domain::gate::GateChallenge {
            driver: id.clone(),
            kind: mm::domain::gate::GateChallengeKind::Header {
                name: "X-Test".to_owned(),
            },
        }),
    ] {
        let mut dependencies = fixture.dependencies.clone();
        dependencies.drivers = GateDriverRegistry::new(vec![Arc::new(FakeDriver {
            id: id.clone(),
            result,
        })])?;
        assert!(
            GateManager::new(dependencies)
                .login(GateLoginRequest {
                    policy_id: policy.id,
                    username: "alice".to_owned(),
                    password: material("correct-password")?,
                    cap_token: material("token")?,
                    replace_session_id: None,
                })
                .await
                .is_err()
        );
    }

    let principal = GatePrincipal {
        user_id: user.metadata.id,
        normalized_username: "alice".to_owned(),
        display_username: "alice".to_owned(),
        authenticated_drivers: vec![id.clone()],
    };
    let mut dependencies = fixture.dependencies.clone();
    dependencies.drivers = GateDriverRegistry::new(vec![Arc::new(FakeDriver {
        id,
        result: GateDriverResult::Authenticated(principal),
    })])?;
    assert!(
        GateManager::new(dependencies)
            .login(GateLoginRequest {
                policy_id: policy.id,
                username: "alice".to_owned(),
                password: material("correct-password")?,
                cap_token: material("token")?,
                replace_session_id: None,
            })
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn session_validation_rejects_malformed_stale_and_mismatched_state()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let session = fixture.session(&policy, &user, 1);
    fixture.repository.insert_session(&session)?;
    let claims = session_claims(&policy, &user, &session)?;
    let cookie = FakeSessionCipher::cookie(&claims, 1)?;
    assert!(
        !fixture
            .manager
            .validate_session(policy.id, &cookie)
            .await?
            .requires_rotation
    );
    assert!(
        fixture
            .manager
            .validate_session(Uuid::new_v4(), &cookie)
            .await
            .is_err()
    );
    assert!(
        fixture
            .manager
            .validate_session(policy.id, "malformed")
            .await
            .is_err()
    );
    assert!(
        fixture
            .manager
            .validate_session(policy.id, "gate.not-a-version.value")
            .await
            .is_err()
    );
    assert!(
        fixture
            .manager
            .validate_session(policy.id, "gate.2.value")
            .await
            .is_err()
    );
    fixture.sessions.fail_open.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .validate_session(policy.id, &cookie)
            .await
            .is_err()
    );

    let mut invalid_claims = Vec::new();
    let mut value = claims.clone();
    value.policy_id = Uuid::new_v4();
    invalid_claims.push(value);
    let mut value = claims.clone();
    value.issued_at_unix_ms = fixture.clock.now_unix_ms() + 61_000;
    invalid_claims.push(value);
    let mut value = claims.clone();
    value.expires_at_unix_ms = fixture.clock.now_unix_ms() - 61_000;
    invalid_claims.push(value);
    let mut value = claims.clone();
    value.authenticated_drivers.clear();
    invalid_claims.push(value);
    let mut value = claims.clone();
    value
        .authenticated_drivers
        .push(GateDriverId::new(PASSWORD_CAP_DRIVER)?);
    invalid_claims.push(value);
    let mut value = claims.clone();
    value.authenticated_drivers = vec![GateDriverId::new("other")?];
    invalid_claims.push(value);
    for invalid in invalid_claims {
        let invalid_cookie = FakeSessionCipher::cookie(&invalid, 1)?;
        assert!(
            fixture
                .manager
                .validate_session(policy.id, &invalid_cookie)
                .await
                .is_err()
        );
    }

    let mut all_policy = policy.clone();
    all_policy.mode = GatePolicyMode::All;
    all_policy.drivers = vec![
        GateDriverId::new("other")?,
        GateDriverId::new(PASSWORD_CAP_DRIVER)?,
    ];
    all_policy.drivers.sort();
    *fixture
        .repository
        .state()?
        .policies
        .iter_mut()
        .find(|stored| stored.id == policy.id)
        .ok_or_else(authentication_failed)? = all_policy;
    assert!(
        fixture
            .manager
            .validate_session(policy.id, &cookie)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn session_validation_checks_repository_and_user_state_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    enum Mutation {
        MissingSession,
        SessionPolicy,
        SessionUser,
        SessionVersion,
        SessionRevoked,
        SessionExpiry,
        MissingUser,
        UserDisabled,
        PasswordVersion,
    }

    for mutation in [
        Mutation::MissingSession,
        Mutation::SessionPolicy,
        Mutation::SessionUser,
        Mutation::SessionVersion,
        Mutation::SessionRevoked,
        Mutation::SessionExpiry,
        Mutation::MissingUser,
        Mutation::UserDisabled,
        Mutation::PasswordVersion,
    ] {
        let fixture = Fixture::new()?;
        let policy = fixture.policy("internal")?;
        fixture.insert_policy(&policy)?;
        let user = fixture.user(&policy, "alice")?;
        fixture.insert_user(&user)?;
        let session = fixture.session(&policy, &user, 1);
        fixture.repository.insert_session(&session)?;
        let claims = session_claims(&policy, &user, &session)?;
        let cookie = FakeSessionCipher::cookie(&claims, 1)?;
        {
            let mut state = fixture.repository.state()?;
            match mutation {
                Mutation::MissingSession => state.sessions.clear(),
                Mutation::SessionPolicy => state.sessions[0].policy_id = Uuid::new_v4(),
                Mutation::SessionUser => state.sessions[0].user_id = Uuid::new_v4(),
                Mutation::SessionVersion => state.sessions[0].key_version = 2,
                Mutation::SessionRevoked => {
                    state.sessions[0].revoked_at_unix_ms = Some(fixture.clock.now_unix_ms());
                }
                Mutation::SessionExpiry => state.sessions[0].expires_at_unix_ms += 1,
                Mutation::MissingUser => state.users.clear(),
                Mutation::UserDisabled => state.users[0].metadata.enabled = false,
                Mutation::PasswordVersion => state.users[0].metadata.password_version += 1,
            }
        }
        assert!(
            fixture
                .manager
                .validate_session(policy.id, &cookie)
                .await
                .is_err()
        );
    }
    Ok(())
}

#[tokio::test]
async fn previous_key_sessions_validate_then_rotate_once_with_preserved_expiry()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut policy = fixture.policy("internal")?;
    let old_reference = policy.session_keys.current.clone();
    policy.session_keys.current =
        SecretReference::new(StorageProviderId::new("age")?, "current-key-2")?;
    policy.session_keys.current_version = 2;
    policy.session_keys.previous = Some(PreviousSecretReference {
        version: 1,
        reference: old_reference.clone(),
        accept_until_unix_ms: fixture.clock.now_unix_ms() + 60_000,
    });
    fixture.insert_policy(&policy)?;
    fixture.secrets.put(&old_reference, "old-key")?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let old_session = fixture.session(&policy, &user, 1);
    fixture.repository.insert_session(&old_session)?;
    let claims = session_claims(&policy, &user, &old_session)?;
    let old_cookie = FakeSessionCipher::cookie(&claims, 1)?;
    let validation = fixture
        .manager
        .validate_session(policy.id, &old_cookie)
        .await?;
    assert!(validation.requires_rotation);

    fixture.sessions.fail_seal.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .rotate_validated_session(&validation)
            .await
            .is_err()
    );
    let rotation = fixture
        .manager
        .rotate_validated_session(&validation)
        .await?;
    assert_eq!(rotation.session.key_version, 2);
    assert_eq!(
        rotation.session.expires_at_unix_ms,
        old_session.expires_at_unix_ms
    );
    assert!(format!("{rotation:?}").contains("[REDACTED]"));
    assert!(
        fixture
            .manager
            .validate_session(policy.id, &old_cookie)
            .await
            .is_err()
    );
    let current = fixture
        .manager
        .validate_session(policy.id, rotation.cookie_value.as_str())
        .await?;
    assert!(!current.requires_rotation);
    assert!(
        fixture
            .manager
            .rotate_validated_session(&current)
            .await
            .is_err()
    );

    let mut expired_previous = policy.clone();
    expired_previous
        .session_keys
        .previous
        .as_mut()
        .ok_or_else(authentication_failed)?
        .accept_until_unix_ms = fixture.clock.now_unix_ms() - 1;
    *fixture
        .repository
        .state()?
        .policies
        .iter_mut()
        .find(|stored| stored.id == policy.id)
        .ok_or_else(authentication_failed)? = expired_previous;
    assert!(
        fixture
            .manager
            .validate_session(policy.id, &old_cookie)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn validated_session_rotation_rechecks_policy_user_and_expiry()
-> Result<(), Box<dyn std::error::Error>> {
    enum Mutation {
        MissingPolicy,
        MissingUser,
        DisabledUser,
        Expired,
        MissingKey,
        StaleReplacement,
    }
    for mutation in [
        Mutation::MissingPolicy,
        Mutation::MissingUser,
        Mutation::DisabledUser,
        Mutation::Expired,
        Mutation::MissingKey,
        Mutation::StaleReplacement,
    ] {
        let fixture = Fixture::new()?;
        let mut policy = fixture.policy("internal")?;
        policy.session_keys.current_version = 2;
        fixture.insert_policy(&policy)?;
        let user = fixture.user(&policy, "alice")?;
        fixture.insert_user(&user)?;
        let session = fixture.session(&policy, &user, 1);
        fixture.repository.insert_session(&session)?;
        let validation = GateSessionValidation {
            principal: GatePrincipal {
                user_id: user.metadata.id,
                normalized_username: "alice".to_owned(),
                display_username: "alice".to_owned(),
                authenticated_drivers: policy.drivers.clone(),
            },
            session: session.clone(),
            csrf_binding: "csrf-binding".to_owned(),
            requires_rotation: true,
        };
        match mutation {
            Mutation::MissingPolicy => fixture.repository.state()?.policies.clear(),
            Mutation::MissingUser => fixture.repository.state()?.users.clear(),
            Mutation::DisabledUser => fixture.repository.state()?.users[0].metadata.enabled = false,
            Mutation::Expired => {
                fixture
                    .clock
                    .0
                    .store(validation.session.expires_at_unix_ms, Ordering::SeqCst);
            }
            Mutation::MissingKey => {
                fixture
                    .secrets
                    .values
                    .lock()
                    .map_err(|_| test_error("secrets lock"))?
                    .remove(&policy.session_keys.current.canonical());
            }
            Mutation::StaleReplacement => {
                fixture
                    .repository
                    .revoke_session(session.id, fixture.clock.now_unix_ms(), None)?;
            }
        }
        assert!(
            fixture
                .manager
                .rotate_validated_session(&validation)
                .await
                .is_err()
        );
    }
    Ok(())
}

#[test]
fn session_listing_and_revocation_cover_ownership_stale_and_direct_logout()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let other_policy = fixture.policy("other")?;
    fixture.insert_policy(&other_policy)?;
    let user = fixture.user(&policy, "alice")?;
    fixture.insert_user(&user)?;
    let session = fixture.session(&policy, &user, 1);
    fixture.repository.insert_session(&session)?;
    assert_eq!(
        fixture
            .manager
            .list_sessions(fixture.project_id, "development", "internal")?,
        vec![session.clone()]
    );
    assert!(
        fixture
            .manager
            .plan_revoke_session(
                fixture.project_id,
                "development",
                "internal",
                Uuid::new_v4(),
            )
            .is_err()
    );
    assert!(matches!(
        fixture
            .manager
            .plan_revoke_session(fixture.project_id, "development", "other", session.id,),
        Err(MmError::Security { .. })
    ));
    let plan = fixture.manager.plan_revoke_session(
        fixture.project_id,
        "development",
        "internal",
        session.id,
    )?;
    fixture.repository.no_effect_next("revoke_session")?;
    assert!(
        fixture
            .manager
            .revoke_session_reviewed(
                fixture.project_id,
                "development",
                "internal",
                session.id,
                &plan.plan_hash,
            )
            .is_err()
    );
    let plan = fixture.manager.plan_revoke_session(
        fixture.project_id,
        "development",
        "internal",
        session.id,
    )?;
    assert!(fixture.manager.revoke_session_reviewed(
        fixture.project_id,
        "development",
        "internal",
        session.id,
        &plan.plan_hash,
    )?);
    assert!(
        fixture
            .manager
            .plan_revoke_session(fixture.project_id, "development", "internal", session.id,)
            .is_err()
    );

    let second = fixture.session(&policy, &user, 1);
    fixture.repository.insert_session(&second)?;
    assert!(fixture.manager.revoke_authenticated_session(second.id)?);
    assert!(!fixture.manager.revoke_authenticated_session(second.id)?);
    Ok(())
}

#[tokio::test]
async fn session_key_rotation_handles_overlap_no_effect_stale_and_rollback()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.policy("internal")?;
    fixture.insert_policy(&policy)?;
    let plan =
        fixture
            .manager
            .plan_rotate_session_key(fixture.project_id, "development", "internal")?;
    assert!(
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                "wrong-review",
            )
            .await
            .is_err()
    );

    fixture
        .repository
        .policy_lookups_before_bump
        .store(1, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );

    let plan =
        fixture
            .manager
            .plan_rotate_session_key(fixture.project_id, "development", "internal")?;
    fixture.secrets.queue_create(Ok(false))?;
    assert!(
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );

    let plan =
        fixture
            .manager
            .plan_rotate_session_key(fixture.project_id, "development", "internal")?;
    fixture.repository.fail_next("replace_policy")?;
    fixture.secrets.queue_unset_failure(false)?;
    assert!(
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await
            .is_err()
    );

    let plan =
        fixture
            .manager
            .plan_rotate_session_key(fixture.project_id, "development", "internal")?;
    fixture.repository.fail_next("replace_policy")?;
    fixture.secrets.queue_unset_failure(true)?;
    assert!(matches!(
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await,
        Err(MmError::OperationFailure { .. })
    ));

    let plan =
        fixture
            .manager
            .plan_rotate_session_key(fixture.project_id, "development", "internal")?;
    let rotated = fixture
        .manager
        .rotate_session_key(
            fixture.project_id,
            "development",
            "internal",
            &plan.plan_hash,
        )
        .await?;
    assert_eq!(rotated.session_keys.current_version, 2);
    assert!(
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                &fixture
                    .manager
                    .plan_rotate_session_key(fixture.project_id, "development", "internal",)?
                    .plan_hash,
            )
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn session_key_rotation_cleans_expired_previous_keys_best_effort()
-> Result<(), Box<dyn std::error::Error>> {
    for cleanup in ["success", "inspect-failure", "unset-failure"] {
        let fixture = Fixture::new()?;
        let mut policy = fixture.policy("internal")?;
        let expired = SecretReference::new(StorageProviderId::new("age")?, "expired-key")?;
        policy.session_keys.previous = Some(PreviousSecretReference {
            version: 0,
            reference: expired.clone(),
            accept_until_unix_ms: fixture.clock.now_unix_ms() - 1,
        });
        fixture.insert_policy(&policy)?;
        fixture.secrets.put(&expired, "expired")?;
        if cleanup == "inspect-failure" {
            fixture.secrets.queue_inspect_failure(true)?;
        } else if cleanup == "unset-failure" {
            fixture.secrets.queue_unset_failure(true)?;
        }
        let plan = fixture.manager.plan_rotate_session_key(
            fixture.project_id,
            "development",
            "internal",
        )?;
        fixture
            .manager
            .rotate_session_key(
                fixture.project_id,
                "development",
                "internal",
                &plan.plan_hash,
            )
            .await?;
        assert_eq!(fixture.secrets.contains(&expired)?, cleanup != "success");
    }
    Ok(())
}

#[tokio::test]
async fn route_planning_enforces_existence_ownership_tls_and_attachment_state()
-> Result<(), Box<dyn std::error::Error>> {
    let (fixture, policy, claim) = route_fixture()?;
    fixture.routes.fail_route_set.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "internal",
                claim.target_id,
                &claim.router_name,
                true,
            )
            .is_err()
    );
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "internal",
                claim.target_id,
                "missing-router",
                true,
            )
            .is_err()
    );
    let mut foreign = claim.clone();
    foreign.project_id = ProjectId(Uuid::new_v4());
    fixture.insert_route(foreign)?;
    assert!(matches!(
        fixture.manager.plan_attach(
            fixture.project_id,
            "development",
            "internal",
            claim.target_id,
            &claim.router_name,
            true,
        ),
        Err(MmError::Security { .. })
    ));
    let mut insecure = claim.clone();
    insecure.tls = false;
    fixture.insert_route(insecure)?;
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "internal",
                claim.target_id,
                &claim.router_name,
                true,
            )
            .is_err()
    );
    fixture.insert_route(claim.clone())?;
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "internal",
                claim.target_id,
                &claim.router_name,
                false,
            )
            .is_err()
    );

    let attachment = attach_route(&fixture, &claim).await?;
    assert_eq!(attachment.policy_id, policy.id);
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "internal",
                claim.target_id,
                &claim.router_name,
                true,
            )
            .is_err()
    );
    let mut other = policy.clone();
    other.id = Uuid::new_v4();
    other.name = GatePolicyName::new("other")?;
    fixture.insert_policy(&other)?;
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "other",
                claim.target_id,
                &claim.router_name,
                false,
            )
            .is_err()
    );

    let detach = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        claim.target_id,
        &claim.router_name,
        false,
    )?;
    fixture
        .manager
        .apply_attach_reviewed(route_request(&fixture, &claim, false, &detach.plan_hash))
        .await?;
    let routes = fixture.routes.route_set(claim.target_id)?;
    assert!(routes.middlewares.is_empty());
    assert!(routes.claims[0].middlewares.is_empty());
    assert!(
        fixture
            .repository
            .attachment(claim.target_id, &claim.router_name)?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn route_apply_rejects_policy_and_claims_removed_during_review()
-> Result<(), Box<dyn std::error::Error>> {
    let (fixture, _policy, claim) = route_fixture()?;
    let plan = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        claim.target_id,
        &claim.router_name,
        true,
    )?;
    fixture
        .routes
        .clear_claims_after_route_set
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(route_request(&fixture, &claim, true, &plan.plan_hash))
            .await
            .is_err()
    );

    let (fixture, _policy, claim) = route_fixture()?;
    fixture.repository.state()?.policies.clear();
    assert!(
        fixture
            .manager
            .plan_attach(
                fixture.project_id,
                "development",
                "internal",
                claim.target_id,
                &claim.router_name,
                true,
            )
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn route_apply_rejects_review_lock_and_generation_changes_before_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let (fixture, _policy, claim) = route_fixture()?;
    let plan = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        claim.target_id,
        &claim.router_name,
        true,
    )?;
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(route_request(&fixture, &claim, true, "wrong-review"))
            .await
            .is_err()
    );
    fixture.target_locks.fail.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(route_request(&fixture, &claim, true, &plan.plan_hash))
            .await
            .is_err()
    );
    fixture
        .routes
        .bump_after_route_set
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(route_request(&fixture, &claim, true, &plan.plan_hash))
            .await
            .is_err()
    );
    assert!(lock(&fixture.ingress.publications, "publications")?.is_empty());

    Ok(())
}

#[tokio::test]
async fn attach_publication_failures_restore_previous_ingress_or_report_unknown_state()
-> Result<(), Box<dyn std::error::Error>> {
    for (first, rollback, operation_failure) in [
        (PublishResult::Failure, PublishResult::Valid, false),
        (PublishResult::Failure, PublishResult::Failure, true),
        (PublishResult::WrongDigest, PublishResult::Valid, false),
        (PublishResult::WrongDigest, PublishResult::Failure, true),
    ] {
        let (fixture, _policy, claim) = route_fixture()?;
        fixture.ingress.queue(first)?;
        fixture.ingress.queue(rollback)?;
        let error = attach_route(&fixture, &claim).await;
        assert!(error.is_err());
        assert_eq!(
            matches!(error, Err(MmError::OperationFailure { .. })),
            operation_failure
        );
        assert!(
            fixture
                .repository
                .attachment(claim.target_id, &claim.router_name)?
                .is_none()
        );
        assert!(
            fixture
                .routes
                .route_set(claim.target_id)?
                .middlewares
                .is_empty()
        );
    }
    Ok(())
}

#[tokio::test]
async fn attach_route_and_metadata_failures_execute_compensating_rollbacks()
-> Result<(), Box<dyn std::error::Error>> {
    for rollback_fails in [false, true] {
        let (fixture, _policy, claim) = route_fixture()?;
        fixture.routes.queue_replace_failure(true)?;
        if rollback_fails {
            fixture.ingress.queue(PublishResult::Valid)?;
            fixture.ingress.queue(PublishResult::Failure)?;
        }
        let error = attach_route(&fixture, &claim).await;
        assert!(error.is_err());
        assert_eq!(
            error
                .as_ref()
                .err()
                .is_some_and(|error| error.to_string().contains("rollback failed")),
            rollback_fails
        );
    }

    for failure in ["metadata", "route-rollback", "publish-rollback"] {
        let (fixture, _policy, claim) = route_fixture()?;
        fixture.repository.fail_next("insert_attachment")?;
        if failure == "route-rollback" {
            fixture.routes.queue_replace_failure(false)?;
            fixture.routes.queue_replace_failure(true)?;
        }
        if failure == "publish-rollback" {
            fixture.ingress.queue(PublishResult::Valid)?;
            fixture.ingress.queue(PublishResult::Failure)?;
        }
        let error = attach_route(&fixture, &claim).await;
        assert!(error.is_err());
        assert_eq!(
            error
                .as_ref()
                .err()
                .is_some_and(|error| error.to_string().contains("rollback failed")),
            failure != "metadata"
        );
    }
    Ok(())
}

#[tokio::test]
async fn detach_publication_failures_restore_metadata_routes_and_ingress()
-> Result<(), Box<dyn std::error::Error>> {
    for result in [PublishResult::Failure, PublishResult::WrongDigest] {
        let (fixture, _policy, claim) = route_fixture()?;
        let attachment = attach_route(&fixture, &claim).await?;
        fixture.ingress.queue(result)?;
        fixture.ingress.queue(PublishResult::Valid)?;
        let plan = fixture.manager.plan_attach(
            fixture.project_id,
            "development",
            "internal",
            claim.target_id,
            &claim.router_name,
            false,
        )?;
        assert!(
            fixture
                .manager
                .apply_attach_reviewed(route_request(&fixture, &claim, false, &plan.plan_hash))
                .await
                .is_err()
        );
        assert_eq!(
            fixture
                .repository
                .attachment(claim.target_id, &claim.router_name)?,
            Some(attachment)
        );
        assert!(
            !fixture
                .routes
                .route_set(claim.target_id)?
                .middlewares
                .is_empty()
        );
    }

    for incomplete in ["metadata", "route", "publish"] {
        let (fixture, _policy, claim) = route_fixture()?;
        attach_route(&fixture, &claim).await?;
        fixture.ingress.queue(PublishResult::Failure)?;
        if incomplete == "metadata" {
            fixture.repository.fail_next("insert_attachment")?;
        } else if incomplete == "route" {
            fixture.routes.queue_replace_failure(false)?;
            fixture.routes.queue_replace_failure(true)?;
        } else {
            fixture.ingress.queue(PublishResult::Failure)?;
        }
        let plan = fixture.manager.plan_attach(
            fixture.project_id,
            "development",
            "internal",
            claim.target_id,
            &claim.router_name,
            false,
        )?;
        let error = fixture
            .manager
            .apply_attach_reviewed(route_request(&fixture, &claim, false, &plan.plan_hash))
            .await;
        assert!(
            error
                .as_ref()
                .err()
                .is_some_and(|error| error.to_string().contains("rollback was incomplete"))
        );
    }
    Ok(())
}
