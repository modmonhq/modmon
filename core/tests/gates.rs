use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::gate::{Argon2idPasswordVerifier, XChaChaSessionCipher};
use mm::adapters::state::SqliteStateStore;
use mm::app::ports::{
    CapSiteProvisioner, CapTokenVerifier, Clock, GateIngressPublisher, GatePolicySource,
    GateRepository, IngressRouteRegistry, ProvisionedCapSite, SecretCreator,
    SecretMaterialResolver, SecretMetadataInspector, SecretRemover, TargetOperationLock,
    TargetOperationLockGuard,
};
use mm::app::{
    AddGatePolicyRequest, ApplyGateRouteRequest, ApplyGateUserRequest, GateDriverRegistry,
    GateLoginRequest, GateManager, GateManagerDependencies, GateUserMutation,
    PasswordCapGateDriver,
};
use mm::domain::error::MmError;
use mm::domain::gate::{
    DEFAULT_SESSION_TTL_SECONDS, GateMutationAction, GatePolicyMode, GatePolicyName,
    GateSessionMetadata,
};
use mm::domain::ingress::{
    CanonicalMiddleware, IngressCertificateReference, IngressRouteClaim, IngressRouteOwnership,
    IngressRouteProvenance,
};
use mm::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretResolution,
    SecretScope, SecretStatus, SecretWriteResult,
};
use mm::domain::target::TargetId;
use mm::domain::workspace::ProjectId;
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

struct Fixture {
    _root: TempDir,
    database: std::path::PathBuf,
    repository: Arc<SqliteStateStore>,
    secrets: Arc<MemorySecrets>,
    cap: Arc<FakeCap>,
    ingress: Arc<FakeGateIngress>,
    clock: Arc<TestClock>,
    manager: GateManager,
    project_id: ProjectId,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = temp_dirs::tempdir()?;
        let database = root.path().join("state.sqlite3");
        let repository = Arc::new(SqliteStateStore::open(&database)?);
        let secrets = Arc::new(MemorySecrets::default());
        let cap = Arc::new(FakeCap::default());
        let clock = Arc::new(TestClock::new(1_700_000_000_000));
        let ingress = Arc::new(FakeGateIngress::default());
        let password = Arc::new(Argon2idPasswordVerifier);
        let driver = Arc::new(PasswordCapGateDriver::new(
            repository.clone(),
            password.clone(),
            cap.clone(),
            secrets.clone(),
            clock.clone(),
        )?);
        let manager = GateManager::new(GateManagerDependencies {
            repository: repository.clone(),
            routes: repository.clone(),
            ingress: ingress.clone(),
            target_locks: Arc::new(TestTargetLock),
            password,
            sessions: Arc::new(XChaChaSessionCipher),
            drivers: GateDriverRegistry::new(vec![driver])?,
            cap_sites: cap.clone(),
            secret_inspector: secrets.clone(),
            secret_resolver: secrets.clone(),
            secret_creator: secrets.clone(),
            secret_remover: secrets.clone(),
            clock: clock.clone(),
        });
        Ok(Self {
            _root: root,
            database,
            repository,
            secrets,
            cap,
            ingress,
            clock,
            manager,
            project_id: ProjectId(Uuid::new_v4()),
        })
    }

    async fn add_policy(
        &self,
        name: &str,
        ttl: u64,
    ) -> Result<mm::domain::gate::GatePolicy, MmError> {
        let request = AddGatePolicyRequest {
            project_id: self.project_id,
            environment: "development".to_owned(),
            name: GatePolicyName::new(name).map_err(|message| MmError::Security { message })?,
            mode: GatePolicyMode::Any,
            session_ttl_seconds: ttl,
            secret_provider: "age".to_owned(),
        };
        let plan = self.manager.plan_add_policy(&request)?;
        self.manager
            .add_policy_reviewed(request, &plan.plan_hash)
            .await
    }

    async fn add_user(&self, policy: &str, username: &str, password: &str) -> Result<(), MmError> {
        let plan = self.manager.plan_user(
            GateMutationAction::AddUser,
            self.project_id,
            "development",
            policy,
            username,
        )?;
        self.manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: self.project_id,
                environment: "development",
                policy_name: policy,
                username,
                reviewed_plan_hash: &plan.plan_hash,
                mutation: GateUserMutation::Add(material(password)?),
            })
            .await?;
        Ok(())
    }

    async fn login(
        &self,
        policy_id: Uuid,
        username: &str,
        password: &str,
        cap_token: &str,
        replace_session_id: Option<Uuid>,
    ) -> Result<mm::app::GateLoginResult, MmError> {
        self.manager
            .login(GateLoginRequest {
                policy_id,
                username: username.to_owned(),
                password: material(password)?,
                cap_token: material(cap_token)?,
                replace_session_id,
            })
            .await
    }
}

#[tokio::test]
async fn repository_policy_user_and_session_lifecycles_cover_stale_and_list_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    assert!(!fixture.repository.gate_components_required()?);
    let policy = fixture
        .add_policy("repository", DEFAULT_SESSION_TTL_SECONDS)
        .await?;
    assert!(fixture.repository.gate_components_required()?);
    assert_eq!(
        fixture.repository.policy_by_id(policy.id)?,
        Some(policy.clone())
    );
    assert_eq!(
        fixture
            .repository
            .list_policies(fixture.project_id, "development")?
            .len(),
        1
    );
    assert!(
        fixture
            .repository
            .replace_policy(&policy, u64::MAX)
            .is_err()
    );

    fixture
        .add_user("repository", "Alice", "correct horse")
        .await?;
    let user = fixture
        .repository
        .user(policy.id, "alice")?
        .ok_or("user missing")?;
    assert_eq!(
        fixture.repository.user_by_id(user.metadata.id)?,
        Some(user.clone())
    );
    assert_eq!(fixture.repository.list_users(policy.id)?.len(), 1);
    assert!(fixture.repository.replace_user(&user, u64::MAX).is_err());
    fixture.repository.record_authentication_failure(
        user.metadata.id,
        user.window_started_at_unix_ms + 16 * 60 * 1000,
    )?;
    fixture
        .repository
        .record_authentication_failure(Uuid::new_v4(), 1)?;
    let reset = fixture
        .repository
        .user_by_id(user.metadata.id)?
        .ok_or("reset user missing")?;
    assert_eq!(reset.failed_attempts, 1);

    let first = GateSessionMetadata {
        id: Uuid::new_v4(),
        policy_id: policy.id,
        user_id: user.metadata.id,
        username: "alice".to_owned(),
        issued_at_unix_ms: 100,
        expires_at_unix_ms: 10_000,
        last_seen_at_unix_ms: 100,
        key_version: 1,
        revoked_at_unix_ms: None,
        replaced_by: None,
    };
    fixture.repository.insert_session(&first)?;
    assert_eq!(fixture.repository.session(first.id)?, Some(first.clone()));
    assert_eq!(fixture.repository.list_sessions(policy.id, 50)?.len(), 1);
    assert!(
        fixture
            .repository
            .list_sessions(policy.id, 20_000)?
            .is_empty()
    );
    let replacement = GateSessionMetadata {
        id: Uuid::new_v4(),
        issued_at_unix_ms: 200,
        expires_at_unix_ms: 20_000,
        last_seen_at_unix_ms: 200,
        ..first.clone()
    };
    fixture
        .repository
        .insert_session_replacing(&replacement, first.id, 200)?;
    assert_eq!(
        fixture
            .repository
            .session(first.id)?
            .and_then(|session| session.replaced_by),
        Some(replacement.id)
    );
    let stale = GateSessionMetadata {
        id: Uuid::new_v4(),
        ..replacement.clone()
    };
    assert!(
        fixture
            .repository
            .insert_session_replacing(&stale, Uuid::new_v4(), 300)
            .is_err()
    );
    assert!(fixture.repository.session(stale.id)?.is_none());

    assert!(
        !fixture
            .repository
            .remove_user(policy.id, "alice", u64::MAX)?
    );
    assert!(
        fixture
            .repository
            .remove_user(policy.id, "alice", reset.metadata.password_version,)?
    );
    assert!(!fixture.repository.remove_policy(policy.id, u64::MAX)?);
    assert!(
        fixture
            .repository
            .remove_policy(policy.id, policy.revision)?
    );
    assert!(!fixture.repository.gate_components_required()?);
    Ok(())
}

#[derive(Default)]
struct FakeGateIngress {
    publications: Mutex<Vec<String>>,
    fail_next_publish: AtomicBool,
}

#[async_trait]
impl GateIngressPublisher for FakeGateIngress {
    async fn publish(
        &self,
        target_id: TargetId,
        claims: &[IngressRouteClaim],
        middlewares: &BTreeMap<String, CanonicalMiddleware>,
    ) -> Result<String, MmError> {
        if self.fail_next_publish.swap(false, Ordering::SeqCst) {
            return Err(MmError::Target {
                message: "injected gate ingress publication failure".to_owned(),
            });
        }
        let local_hosts = claims
            .iter()
            .filter(|claim| claim.tls && mm::domain::ca::is_local_ca_host(&claim.host))
            .map(|claim| claim.host.clone())
            .collect();
        let rendered =
            mm::app::render_traefik_dynamic_routes(target_id, claims, middlewares, &local_hosts)
                .map_err(|error| MmError::Target {
                    message: error.to_string(),
                })?;
        self.publications
            .lock()
            .map_err(|_| MmError::State {
                message: "gate ingress publication lock poisoned".to_owned(),
            })?
            .push(rendered.manifest.digest.clone());
        Ok(rendered.manifest.digest)
    }
}

struct TestTargetLock;
struct TestTargetLockGuard;

impl TargetOperationLockGuard for TestTargetLockGuard {}

impl TargetOperationLock for TestTargetLock {
    fn acquire(&self, _target_id: TargetId) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(TestTargetLockGuard))
    }
}

#[tokio::test]
async fn credentials_cap_lockout_sessions_rotation_and_audit_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture
        .add_policy("internal", DEFAULT_SESSION_TTL_SECONDS)
        .await?;
    fixture
        .add_user("internal", " Alice ", "correct horse")
        .await?;

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
    let stale_disable = fixture.manager.plan_user(
        GateMutationAction::DisableUser,
        fixture.project_id,
        "development",
        "internal",
        "alice",
    )?;
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
            mutation: GateUserMutation::ResetPassword(material("correct horse")?),
        })
        .await?;
    assert!(
        fixture
            .manager
            .apply_user_reviewed(ApplyGateUserRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                username: "alice",
                reviewed_plan_hash: &stale_disable.plan_hash,
                mutation: GateUserMutation::Disable,
            })
            .await
            .is_err()
    );
    assert!(
        fixture
            .login(policy.id, "alice", "correct horse", "invalid-cap", None)
            .await
            .is_err()
    );
    assert!(
        fixture
            .login(policy.id, "alice", "correct horse", "cap-expired", None)
            .await
            .is_err()
    );
    let missing = match fixture
        .login(policy.id, "missing", "incorrect", "cap-missing", None)
        .await
    {
        Ok(_) => return Err("missing user authenticated".into()),
        Err(error) => error,
    };
    let incorrect = match fixture
        .login(policy.id, "alice", "incorrect", "cap-incorrect", None)
        .await
    {
        Ok(_) => return Err("incorrect password authenticated".into()),
        Err(error) => error,
    };
    assert_eq!(missing.to_string(), incorrect.to_string());
    assert_eq!(missing.to_string(), "authentication failed");

    for attempt in 0..4 {
        let token = format!("cap-lockout-{attempt}");
        assert!(
            fixture
                .login(policy.id, "alice", "incorrect", &token, None)
                .await
                .is_err()
        );
    }
    assert!(
        fixture
            .login(policy.id, "alice", "correct horse", "cap-locked", None)
            .await
            .is_err()
    );
    fixture.clock.advance(61_000);

    let login = fixture
        .login(policy.id, "alice", "correct horse", "cap-success", None)
        .await?;
    assert_eq!(
        login.session.expires_at_unix_ms - login.session.issued_at_unix_ms,
        7 * 24 * 60 * 60 * 1000
    );
    assert!(
        fixture
            .login(policy.id, "alice", "correct horse", "cap-success", None)
            .await
            .is_err()
    );
    let validation = fixture
        .manager
        .validate_session(policy.id, login.cookie_value.as_str())
        .await?;
    assert!(!validation.requires_rotation);

    let rotation_plan =
        fixture
            .manager
            .plan_rotate_session_key(fixture.project_id, "development", "internal")?;
    fixture
        .manager
        .rotate_session_key(
            fixture.project_id,
            "development",
            "internal",
            &rotation_plan.plan_hash,
        )
        .await?;
    let old_key = fixture
        .manager
        .validate_session(policy.id, login.cookie_value.as_str())
        .await?;
    assert!(old_key.requires_rotation);
    let rotated = fixture.manager.rotate_validated_session(&old_key).await?;
    assert!(
        fixture
            .manager
            .validate_session(policy.id, login.cookie_value.as_str())
            .await
            .is_err()
    );
    fixture
        .manager
        .validate_session(policy.id, rotated.cookie_value.as_str())
        .await?;

    let revoke = fixture.manager.plan_revoke_session(
        fixture.project_id,
        "development",
        "internal",
        rotated.session.id,
    )?;
    fixture.manager.revoke_session_reviewed(
        fixture.project_id,
        "development",
        "internal",
        rotated.session.id,
        &revoke.plan_hash,
    )?;
    assert!(
        fixture
            .manager
            .validate_session(policy.id, rotated.cookie_value.as_str())
            .await
            .is_err()
    );
    assert!(
        fixture
            .manager
            .revoke_session_reviewed(
                fixture.project_id,
                "development",
                "internal",
                rotated.session.id,
                &revoke.plan_hash,
            )
            .is_err()
    );

    fixture.cap.outage.store(true, Ordering::SeqCst);
    assert!(matches!(
        fixture
            .login(policy.id, "alice", "correct horse", "cap-outage", None)
            .await,
        Err(MmError::RuntimeResponse { .. })
    ));

    let connection = rusqlite::Connection::open(&fixture.database)?;
    let audit_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM gate_audit", [], |row| row.get(0))?;
    assert!(audit_count >= 4);
    let password_hash: String = connection.query_row(
        "SELECT password_hash FROM gate_users WHERE normalized_username='alice'",
        [],
        |row| row.get(0),
    )?;
    assert!(password_hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
    assert!(!password_hash.contains("correct horse"));
    assert!(!format!("{login:?}").contains(login.cookie_value.as_str()));
    assert!(!format!("{login:?}").contains(login.csrf_token.as_str()));
    assert!(fixture.secrets.count()? >= 4);
    Ok(())
}

#[tokio::test]
async fn route_attachment_is_owned_reviewed_stale_safe_and_reversible()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture
        .add_policy("internal", DEFAULT_SESSION_TTL_SECONDS)
        .await?;
    let target_id = TargetId(Uuid::new_v4());
    let claim = route_claim(target_id, fixture.project_id, "development");
    fixture.repository.replace_owner_claims(
        target_id,
        fixture.project_id,
        "development",
        std::slice::from_ref(&claim),
        &BTreeMap::new(),
        fixture.clock.now_unix_ms(),
    )?;

    let stale = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        target_id,
        &claim.router_name,
        true,
    )?;
    let repeated = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        target_id,
        &claim.router_name,
        true,
    )?;
    assert_eq!(repeated.plan_hash, stale.plan_hash);
    assert_eq!(repeated.effects, stale.effects);
    fixture.repository.replace_owner_claims(
        target_id,
        fixture.project_id,
        "development",
        std::slice::from_ref(&claim),
        &BTreeMap::new(),
        fixture.clock.now_unix_ms(),
    )?;
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(ApplyGateRouteRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                target_id,
                router_name: &claim.router_name,
                attach: true,
                reviewed_plan_hash: &stale.plan_hash,
            })
            .await
            .is_err()
    );

    let rollback = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        target_id,
        &claim.router_name,
        true,
    )?;
    let connection = rusqlite::Connection::open(&fixture.database)?;
    connection.execute_batch(
        "CREATE TRIGGER fail_gate_attachment BEFORE INSERT ON gate_route_attachments BEGIN SELECT RAISE(ABORT, 'injected attachment failure'); END;",
    )?;
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(ApplyGateRouteRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                target_id,
                router_name: &claim.router_name,
                attach: true,
                reviewed_plan_hash: &rollback.plan_hash,
            })
            .await
            .is_err()
    );
    let routes = fixture.repository.route_set(target_id)?;
    assert!(routes.middlewares.is_empty());
    assert!(routes.claims[0].middlewares.is_empty());
    assert!(
        fixture
            .repository
            .attachment(target_id, &claim.router_name)?
            .is_none()
    );
    connection.execute_batch("DROP TRIGGER fail_gate_attachment;")?;

    let attach = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        target_id,
        &claim.router_name,
        true,
    )?;
    let attachment = fixture
        .manager
        .apply_attach_reviewed(ApplyGateRouteRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            target_id,
            router_name: &claim.router_name,
            attach: true,
            reviewed_plan_hash: &attach.plan_hash,
        })
        .await?;
    assert_eq!(attachment.policy_id, policy.id);
    let routes = fixture.repository.route_set(target_id)?;
    assert!(routes.middlewares.contains_key(&attachment.middleware_name));
    assert!(
        routes.claims[0]
            .middlewares
            .contains(&attachment.middleware_name)
    );
    assert!(
        fixture
            .repository
            .attachment(target_id, &claim.router_name)?
            .is_some()
    );

    fixture.repository.replace_owner_claims(
        target_id,
        fixture.project_id,
        "development",
        std::slice::from_ref(&claim),
        &BTreeMap::new(),
        fixture.clock.now_unix_ms(),
    )?;
    let routes = fixture.repository.route_set(target_id)?;
    assert!(routes.middlewares.contains_key(&attachment.middleware_name));
    assert!(
        routes.claims[0]
            .middlewares
            .contains(&attachment.middleware_name)
    );
    let mut changed_claim = claim.clone();
    changed_claim.path_prefix = "/changed".to_owned();
    assert!(
        fixture
            .repository
            .replace_owner_claims(
                target_id,
                fixture.project_id,
                "development",
                &[changed_claim],
                &BTreeMap::new(),
                fixture.clock.now_unix_ms(),
            )
            .is_err()
    );

    let detach = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        target_id,
        &claim.router_name,
        false,
    )?;
    fixture
        .ingress
        .fail_next_publish
        .store(true, Ordering::SeqCst);
    assert!(
        fixture
            .manager
            .apply_attach_reviewed(ApplyGateRouteRequest {
                project_id: fixture.project_id,
                environment: "development",
                policy_name: "internal",
                target_id,
                router_name: &claim.router_name,
                attach: false,
                reviewed_plan_hash: &detach.plan_hash,
            })
            .await
            .is_err()
    );
    let routes = fixture.repository.route_set(target_id)?;
    assert!(routes.middlewares.contains_key(&attachment.middleware_name));
    assert!(
        fixture
            .repository
            .attachment(target_id, &claim.router_name)?
            .is_some()
    );
    let detach = fixture.manager.plan_attach(
        fixture.project_id,
        "development",
        "internal",
        target_id,
        &claim.router_name,
        false,
    )?;
    fixture
        .manager
        .apply_attach_reviewed(ApplyGateRouteRequest {
            project_id: fixture.project_id,
            environment: "development",
            policy_name: "internal",
            target_id,
            router_name: &claim.router_name,
            attach: false,
            reviewed_plan_hash: &detach.plan_hash,
        })
        .await?;
    let routes = fixture.repository.route_set(target_id)?;
    assert!(routes.middlewares.is_empty());
    assert!(routes.claims[0].middlewares.is_empty());
    let publication_count = fixture
        .ingress
        .publications
        .lock()
        .map_err(|_| std::io::Error::other("gate ingress publication lock poisoned"))?
        .len();
    assert_eq!(publication_count, 5);
    assert!(
        fixture
            .repository
            .attachment(target_id, &claim.router_name)?
            .is_none()
    );

    let foreign_target = TargetId(Uuid::new_v4());
    let foreign = route_claim(foreign_target, ProjectId(Uuid::new_v4()), "development");
    fixture.repository.replace_owner_claims(
        foreign_target,
        foreign.project_id,
        "development",
        std::slice::from_ref(&foreign),
        &BTreeMap::new(),
        fixture.clock.now_unix_ms(),
    )?;
    assert!(matches!(
        fixture.manager.plan_attach(
            fixture.project_id,
            "development",
            "internal",
            foreign_target,
            &foreign.router_name,
            true,
        ),
        Err(MmError::Security { .. })
    ));

    let insecure_target = TargetId(Uuid::new_v4());
    let mut insecure = route_claim(insecure_target, fixture.project_id, "development");
    insecure.tls = false;
    fixture.repository.replace_owner_claims(
        insecure_target,
        fixture.project_id,
        "development",
        std::slice::from_ref(&insecure),
        &BTreeMap::new(),
        fixture.clock.now_unix_ms(),
    )?;
    assert!(matches!(
        fixture.manager.plan_attach(
            fixture.project_id,
            "development",
            "internal",
            insecure_target,
            &insecure.router_name,
            true,
        ),
        Err(MmError::Security { .. })
    ));
    Ok(())
}

fn route_claim(target_id: TargetId, project_id: ProjectId, environment: &str) -> IngressRouteClaim {
    IngressRouteClaim {
        target: "local".to_owned(),
        target_id,
        project_id,
        project_name: "example".to_owned(),
        environment: environment.to_owned(),
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
            project_id,
            environment: environment.to_owned(),
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

#[tokio::test]
async fn session_override_fixation_and_expiry_are_enforced()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture.add_policy("short", 24 * 60 * 60).await?;
    fixture
        .add_user("short", "user", "long-enough-password")
        .await?;
    let first = fixture
        .login(policy.id, "user", "long-enough-password", "cap-first", None)
        .await?;
    let replacement = fixture
        .login(
            policy.id,
            "user",
            "long-enough-password",
            "cap-replacement",
            Some(first.session.id),
        )
        .await?;
    assert_eq!(
        replacement.session.expires_at_unix_ms - replacement.session.issued_at_unix_ms,
        24 * 60 * 60 * 1000
    );
    assert!(
        fixture
            .manager
            .validate_session(policy.id, first.cookie_value.as_str())
            .await
            .is_err()
    );
    fixture.clock.advance(24 * 60 * 60 * 1000 + 61_000);
    assert!(
        fixture
            .manager
            .validate_session(policy.id, replacement.cookie_value.as_str())
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn weak_new_passwords_are_rejected_before_hash_storage()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    fixture
        .add_policy("internal", DEFAULT_SESSION_TTL_SECONDS)
        .await?;
    let error = fixture.add_user("internal", "user", "too-short").await;
    assert!(matches!(error, Err(MmError::Security { .. })));
    assert!(
        fixture
            .manager
            .list_users(fixture.project_id, "development", "internal")?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn concurrent_policy_creation_publishes_one_complete_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let request = AddGatePolicyRequest {
        project_id: fixture.project_id,
        environment: "development".to_owned(),
        name: GatePolicyName::new("internal").map_err(|message| MmError::Security { message })?,
        mode: GatePolicyMode::Any,
        session_ttl_seconds: DEFAULT_SESSION_TTL_SECONDS,
        secret_provider: "age".to_owned(),
    };
    let plan = fixture.manager.plan_add_policy(&request)?;
    let first = fixture
        .manager
        .add_policy_reviewed(request.clone(), &plan.plan_hash);
    let second = fixture
        .manager
        .add_policy_reviewed(request, &plan.plan_hash);
    let (first, second) = tokio::join!(first, second);

    assert_ne!(first.is_ok(), second.is_ok());
    assert_eq!(
        fixture
            .manager
            .show(fixture.project_id, "development", None)?
            .len(),
        1
    );
    assert_eq!(fixture.secrets.count()?, 3);
    Ok(())
}

#[tokio::test]
async fn policy_lookup_and_reviewed_removal_clean_owned_resources()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let policy = fixture
        .add_policy("internal", DEFAULT_SESSION_TTL_SECONDS)
        .await?;
    assert!(
        fixture
            .add_policy("internal", DEFAULT_SESSION_TTL_SECONDS)
            .await
            .is_err()
    );
    assert_eq!(fixture.secrets.count()?, 3);

    let named = fixture
        .manager
        .show(fixture.project_id, "development", Some("internal"))?;
    assert_eq!(named, vec![policy.clone()]);
    assert_eq!(fixture.manager.policy_by_id(policy.id)?, policy);
    assert!(fixture.manager.policy_by_id(Uuid::new_v4()).is_err());

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
                "stale-review",
            )
            .await
            .is_err()
    );
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
    assert!(
        fixture
            .manager
            .show(fixture.project_id, "development", None)?
            .is_empty()
    );
    assert_eq!(fixture.secrets.count()?, 0);
    Ok(())
}

fn material(value: &str) -> Result<SecretMaterial, MmError> {
    SecretMaterial::new(value.to_owned()).map_err(|message| MmError::Secret { message })
}

struct TestClock(AtomicI64);

impl TestClock {
    fn new(now: i64) -> Self {
        Self(AtomicI64::new(now))
    }

    fn advance(&self, milliseconds: i64) {
        self.0.fetch_add(milliseconds, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakeCap {
    outage: AtomicBool,
}

#[async_trait]
impl CapTokenVerifier for FakeCap {
    async fn verify(
        &self,
        _site_key: &str,
        _site_secret: &SecretMaterial,
        token: &SecretMaterial,
    ) -> Result<bool, MmError> {
        if self.outage.load(Ordering::SeqCst) {
            return Err(MmError::RuntimeResponse {
                message: "Cap test outage".to_owned(),
            });
        }
        Ok(token.expose(|value| value.starts_with("cap-") && value != "cap-expired"))
    }
}

#[async_trait]
impl CapSiteProvisioner for FakeCap {
    async fn create(&self, name: &str) -> Result<ProvisionedCapSite, MmError> {
        tokio::task::yield_now().await;
        Ok(ProvisionedCapSite {
            site_key: format!("site-{}", &blake3::hash(name.as_bytes()).to_hex()[..12]),
            site_secret: material("cap-site-secret")?,
        })
    }

    async fn remove(&self, _site_key: &str) -> Result<(), MmError> {
        Ok(())
    }
}

#[derive(Default)]
struct MemorySecrets {
    values: Mutex<BTreeMap<String, String>>,
}

impl MemorySecrets {
    fn values(&self) -> Result<std::sync::MutexGuard<'_, BTreeMap<String, String>>, MmError> {
        self.values.lock().map_err(|_| MmError::State {
            message: "test secret lock poisoned".to_owned(),
        })
    }

    fn count(&self) -> Result<usize, MmError> {
        self.values().map(|values| values.len())
    }
}

#[async_trait]
impl SecretCreator for MemorySecrets {
    async fn create(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
        material: SecretMaterial,
        _now_unix_ms: i64,
        _set_by: Option<&str>,
    ) -> Result<SecretWriteResult, MmError> {
        let value = material.expose(str::to_owned);
        if self
            .values()?
            .insert(reference.canonical(), value)
            .is_some()
        {
            return Err(MmError::State {
                message: "test secret already exists".to_owned(),
            });
        }
        Ok(SecretWriteResult {
            revision: "revision-1".to_owned(),
            changed: true,
        })
    }
}

#[async_trait]
impl SecretMaterialResolver for MemorySecrets {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        let material = self
            .values()?
            .get(&reference.canonical())
            .cloned()
            .map(|value| material(&value))
            .transpose()?;
        Ok(SecretResolution {
            metadata: metadata(material.is_some()),
            material,
        })
    }
}

#[async_trait]
impl SecretMetadataInspector for MemorySecrets {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Ok(metadata(
            self.values()?.contains_key(&reference.canonical()),
        ))
    }
}

#[async_trait]
impl SecretRemover for MemorySecrets {
    async fn unset(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
        _expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        Ok(self.values()?.remove(&reference.canonical()).is_some())
    }
}

fn metadata(present: bool) -> SecretMetadata {
    SecretMetadata {
        status: if present {
            SecretStatus::Set
        } else {
            SecretStatus::Unset
        },
        revision: present.then(|| "revision-1".to_owned()),
        created_at_unix_ms: present.then_some(1_700_000_000_000),
        candidate_revision: None,
        set_by: None,
    }
}
