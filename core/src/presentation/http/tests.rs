use super::{
    Assets, CSP, GateHttpState, GateLoginForm, GateLoginPageQuery, GateLogoutForm,
    future_mutation_guard, gate_csrf_cookie, gate_forward, gate_login, gate_login_page,
    gate_logout, gate_session_cookie, request_scope, require_exact_origin, require_forwarded_https,
    require_ingress, require_safe_origin, secured, valid_return_path,
};
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header::SET_COOKIE};
use axum::response::IntoResponse;
use uuid::Uuid;

use crate::adapters::gate::{Argon2idPasswordVerifier, XChaChaSessionCipher};
use crate::adapters::state::SqliteStateStore;
use crate::app::ports::{
    CapSiteProvisioner, CapTokenVerifier, Clock, GateIngressPublisher, GateRepository,
    PasswordVerifier, ProvisionedCapSite, SecretCreator, SecretMaterialResolver,
    SecretMetadataInspector, SecretRemover, TargetOperationLock, TargetOperationLockGuard,
};
use crate::app::{GateDriverRegistry, GateManager, GateManagerDependencies, PasswordCapGateDriver};
use crate::domain::error::MmError;
use crate::domain::gate::{
    DEFAULT_SESSION_TTL_SECONDS, GateDriverId, GatePolicy, GatePolicyMode, GatePolicyName,
    GateUserMetadata, GateUserRecord, PASSWORD_CAP_DRIVER, VersionedSecretReferences,
};
use crate::domain::ingress::{CanonicalMiddleware, IngressRouteClaim};
use crate::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretMetadata, SecretReference, SecretResolution,
    SecretScope, SecretStatus, SecretWriteResult,
};
use crate::domain::secret_contract::StorageProviderId;
use crate::domain::workspace::ProjectId;

struct UnusedGateIngress;

#[async_trait]
impl GateIngressPublisher for UnusedGateIngress {
    async fn publish(
        &self,
        _target_id: crate::domain::target::TargetId,
        _claims: &[IngressRouteClaim],
        _middlewares: &BTreeMap<String, CanonicalMiddleware>,
    ) -> Result<String, MmError> {
        Err(MmError::State {
            message: "gate ingress publisher is unused in HTTP tests".to_owned(),
        })
    }
}

struct TestTargetLock;
struct TestTargetLockGuard;

impl TargetOperationLockGuard for TestTargetLockGuard {}

impl TargetOperationLock for TestTargetLock {
    fn acquire(
        &self,
        _target_id: crate::domain::target::TargetId,
    ) -> Result<Box<dyn TargetOperationLockGuard>, MmError> {
        Ok(Box::new(TestTargetLockGuard))
    }
}

#[test]
fn global_project_and_malformed_hosts_are_resolved_strictly() {
    let headers = |host: &'static str| {
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static(host));
        headers
    };
    assert!(matches!(
        request_scope(&headers("mm.test")),
        Ok(crate::domain::dashboard::DashboardScope::Installation)
    ));
    assert!(matches!(
        request_scope(&headers("mm.example.test:443")),
        Ok(crate::domain::dashboard::DashboardScope::Project(slug)) if slug == "example"
    ));
    for host in [
        "example.test",
        "mm.other.example.test",
        "mm.Example.test",
        "mm..test",
        "mm.example.test@attacker.test",
        "127.0.0.1",
    ] {
        assert!(request_scope(&headers(host)).is_err(), "accepted {host}");
    }
}

#[test]
fn direct_requests_require_the_private_ingress_credential() {
    let token = "a".repeat(64);
    let expected = blake3::hash(token.as_bytes());
    let mut headers = HeaderMap::new();
    assert_eq!(
        require_ingress(&headers, &expected),
        Err(StatusCode::FORBIDDEN)
    );
    headers.insert(
        "x-mm-ingress-auth",
        HeaderValue::from_static(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ),
    );
    assert_eq!(
        require_ingress(&headers, &expected),
        Err(StatusCode::FORBIDDEN)
    );
    let Ok(token) = HeaderValue::from_str(&token) else {
        return;
    };
    headers.insert("x-mm-ingress-auth", token);
    assert_eq!(require_ingress(&headers, &expected), Ok(()));
}

#[test]
fn spoofed_forwarded_gate_headers_do_not_bypass_the_private_ingress_credential() {
    let expected = blake3::hash("a".repeat(64).as_bytes());
    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("app.example.test"));
    headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    headers.insert(
        "x-forwarded-host",
        HeaderValue::from_static("app.example.test"),
    );
    headers.insert("x-forwarded-uri", HeaderValue::from_static("/private"));

    assert!(require_forwarded_https(&headers).is_ok());
    assert_eq!(
        require_ingress(&headers, &expected),
        Err(StatusCode::FORBIDDEN)
    );
}

#[test]
fn gate_cookie_origin_csrf_and_return_path_primitives_are_strict() {
    let session = gate_session_cookie("__Host-mm_gate_test", "opaque", 604_800);
    assert_eq!(
        session,
        "__Host-mm_gate_test=opaque; Path=/; Max-Age=604800; Secure; HttpOnly; SameSite=Lax"
    );
    let csrf = gate_csrf_cookie("__Host-mm_gate_csrf_test", "token", 600);
    assert_eq!(
        csrf,
        "__Host-mm_gate_csrf_test=token; Path=/; Max-Age=600; Secure; SameSite=Strict"
    );
    for invalid in [
        "//attacker.test",
        "https://attacker.test",
        "/safe\r\nlocation: bad",
    ] {
        assert!(!valid_return_path(invalid));
    }
    assert!(valid_return_path("/private?tab=1"));

    let mut headers = HeaderMap::new();
    headers.insert("host", HeaderValue::from_static("app.example.test"));
    headers.insert("origin", HeaderValue::from_static("https://attacker.test"));
    assert!(require_safe_origin(&headers).is_err());
    headers.insert(
        "origin",
        HeaderValue::from_static("https://app.example.test"),
    );
    assert!(require_safe_origin(&headers).is_ok());
    assert!(require_exact_origin(&headers).is_ok());
    headers.remove("origin");
    assert!(require_exact_origin(&headers).is_err());
}

#[test]
fn future_mutations_require_origin_csrf_scope_and_current_reviewed_hash() {
    let hash = "a".repeat(64);
    let stale = "b".repeat(64);
    let request = Request::builder()
        .method(Method::POST)
        .header("host", "mm.example.test")
        .header("origin", "https://mm.example.test")
        .header("x-mm-csrf-token", "single-use-token")
        .body(Body::empty());
    let Ok(request) = request else {
        return;
    };
    assert!(future_mutation_guard(&request, &hash, &hash, true));
    assert!(!future_mutation_guard(&request, &stale, &hash, true));
    assert!(!future_mutation_guard(&request, &hash, &hash, false));
}

#[test]
fn embedded_assets_have_strict_csp_no_inline_code_and_immutable_names() {
    assert!(CSP.contains("default-src 'none'"));
    assert!(CSP.contains("frame-ancestors 'none'"));
    let Some(index) = Assets::get("index.html") else {
        return;
    };
    let html = String::from_utf8_lossy(index.data.as_ref());
    assert!(!html.contains("https://"));
    assert!(!html.contains("<script>"));
    assert!(!html.contains("style="));
    assert!(Assets::get("gate/widget.js").is_some());
    assert!(Assets::get("gate/cap_wasm_bg.wasm").is_some());
    assert!(Assets::get("gate/pako.min.js").is_some());
    assert!(!Assets::iter().any(|path| path.ends_with(".map")));
    let response = secured(StatusCode::OK.into_response(), true);
    assert_eq!(
        response
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=31536000, immutable")
    );
}

#[tokio::test]
async fn trusted_gate_browser_flow_enforces_credentials_cookies_csrf_and_logout()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = TestGateFixture::new()?;
    let mut headers = fixture.headers();
    let page = gate_login_page(
        State(fixture.state.clone()),
        Path(fixture.policy_id),
        Query(GateLoginPageQuery {
            return_to: Some("/private".to_owned()),
        }),
        headers.clone(),
    )
    .await;
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(
        page.headers()
            .get("referrer-policy")
            .and_then(|value| value.to_str().ok()),
        Some("same-origin")
    );
    let csrf_name = crate::domain::gate::gate_csrf_cookie_name(fixture.policy_id);
    let page_csrf_cookie = cookie_pair(&response_cookie(&page, &csrf_name)?)?;
    let page_csrf = cookie_value_from_pair(&page_csrf_cookie)?;
    let page_body = String::from_utf8(to_bytes(page.into_body(), 128 * 1024).await?.to_vec())?;
    assert!(page_body.contains("<cap-widget"));
    assert!(page_body.contains("type=password"));
    assert!(page_body.contains(&format!("value=\"{page_csrf}\"")));

    let missing_origin = gate_login(
        State(fixture.state.clone()),
        Path(fixture.policy_id),
        headers.clone(),
        Form(login_form(page_csrf, "valid-cap-missing-origin")),
    )
    .await;
    assert_eq!(missing_origin.status(), StatusCode::FORBIDDEN);

    headers.insert(
        "origin",
        HeaderValue::from_static("https://app.example.test"),
    );
    headers.insert("cookie", HeaderValue::from_str(&page_csrf_cookie)?);
    let login = gate_login(
        State(fixture.state.clone()),
        Path(fixture.policy_id),
        headers.clone(),
        Form(login_form(page_csrf, "valid-cap-login")),
    )
    .await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        login
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok()),
        Some("/private")
    );
    let session_name = crate::domain::gate::gate_session_cookie_name(fixture.policy_id);
    let session_cookie = response_cookie(&login, &session_name)?;
    let csrf_cookie = response_cookie(&login, &csrf_name)?;
    assert!(session_cookie.contains("Max-Age=604800"));
    assert!(session_cookie.contains("Secure; HttpOnly; SameSite=Lax"));
    assert!(csrf_cookie.contains("Secure; SameSite=Strict"));

    let mut authenticated = fixture.headers();
    authenticated.insert(
        "cookie",
        HeaderValue::from_str(&format!(
            "{}; {}",
            cookie_pair(&session_cookie)?,
            cookie_pair(&csrf_cookie)?
        ))?,
    );
    let forward = gate_forward(
        State(fixture.state.clone()),
        Path(fixture.policy_id),
        authenticated.clone(),
    )
    .await;
    assert_eq!(forward.status(), StatusCode::OK);
    assert_eq!(
        forward
            .headers()
            .get("x-mm-gate-principal")
            .and_then(|value| value.to_str().ok()),
        Some("alice")
    );

    let csrf_pair = cookie_pair(&csrf_cookie)?;
    let post_csrf = cookie_value_from_pair(&csrf_pair)?;
    authenticated.insert(
        "origin",
        HeaderValue::from_static("https://app.example.test"),
    );
    let rejected_logout = gate_logout(
        State(fixture.state.clone()),
        Path(fixture.policy_id),
        authenticated.clone(),
        Form(GateLogoutForm {
            csrf_token: zeroize::Zeroizing::new("wrong-token".to_owned()),
        }),
    )
    .await;
    assert_eq!(rejected_logout.status(), StatusCode::FORBIDDEN);
    let logout = gate_logout(
        State(fixture.state.clone()),
        Path(fixture.policy_id),
        authenticated.clone(),
        Form(GateLogoutForm {
            csrf_token: zeroize::Zeroizing::new(post_csrf.to_owned()),
        }),
    )
    .await;
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);
    assert!(response_cookie(&logout, &session_name)?.contains("Max-Age=0"));

    authenticated.remove("origin");
    let revoked = gate_forward(State(fixture.state), Path(fixture.policy_id), authenticated).await;
    assert_eq!(revoked.status(), StatusCode::SEE_OTHER);
    Ok(())
}

fn login_form(csrf_token: &str, cap_token: &str) -> GateLoginForm {
    GateLoginForm {
        username: "Alice".to_owned(),
        password: zeroize::Zeroizing::new("correct horse battery".to_owned()),
        cap_token: zeroize::Zeroizing::new(cap_token.to_owned()),
        csrf_token: zeroize::Zeroizing::new(csrf_token.to_owned()),
        return_to: "/private".to_owned(),
    }
}

fn response_cookie(response: &axum::response::Response, name: &str) -> Result<String, String> {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .map(str::to_owned)
        .ok_or_else(|| format!("response did not contain cookie {name}"))
}

fn cookie_pair(set_cookie: &str) -> Result<String, String> {
    set_cookie
        .split(';')
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "set-cookie value is malformed".to_owned())
}

fn cookie_value_from_pair(pair: &str) -> Result<&str, String> {
    pair.split_once('=')
        .map(|(_, value)| value)
        .ok_or_else(|| "cookie pair is malformed".to_owned())
}

struct TestGateFixture {
    _directory: tempfile::TempDir,
    state: GateHttpState,
    policy_id: Uuid,
    ingress_token: String,
}

impl TestGateFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let repository = Arc::new(SqliteStateStore::open(
            &directory.path().join("state.sqlite3"),
        )?);
        let external = Arc::new(TestGateExternal);
        let clock = Arc::new(TestGateClock);
        let password = Arc::new(Argon2idPasswordVerifier);
        let provider = StorageProviderId::new("age")?;
        let reference = |name| SecretReference::new(provider.clone(), name);
        let policy = GatePolicy {
            id: Uuid::new_v4(),
            name: GatePolicyName::new("internal")?,
            project_id: ProjectId(Uuid::new_v4()),
            environment: "development".to_owned(),
            mode: GatePolicyMode::Any,
            drivers: vec![GateDriverId::new(PASSWORD_CAP_DRIVER)?],
            session_ttl_seconds: DEFAULT_SESSION_TTL_SECONDS,
            secret_provider: "age".to_owned(),
            password_pepper: reference("gate/test/password-pepper")?,
            session_keys: VersionedSecretReferences {
                current_version: 1,
                current: reference("gate/test/session-key/1")?,
                previous: None,
            },
            cap_site_key: "test-site".to_owned(),
            cap_site_secret: reference("gate/test/cap-site-secret")?,
            revision: 1,
            created_at_unix_ms: TestGateClock.now_unix_ms(),
            updated_at_unix_ms: TestGateClock.now_unix_ms(),
        };
        repository.insert_policy(&policy)?;
        let pepper = material("test-pepper")?;
        let user = GateUserRecord {
            metadata: GateUserMetadata {
                id: Uuid::new_v4(),
                policy_id: policy.id,
                normalized_username: "alice".to_owned(),
                display_username: "Alice".to_owned(),
                enabled: true,
                password_version: 1,
                created_at_unix_ms: TestGateClock.now_unix_ms(),
                updated_at_unix_ms: TestGateClock.now_unix_ms(),
            },
            password_hash: password.hash(&material("correct horse battery")?, &pepper)?,
            failed_attempts: 0,
            window_started_at_unix_ms: TestGateClock.now_unix_ms(),
            locked_until_unix_ms: None,
        };
        repository.insert_user(&user)?;
        let repository_port: Arc<dyn GateRepository> = repository.clone();
        let resolver: Arc<dyn SecretMaterialResolver> = external.clone();
        let cap: Arc<dyn CapTokenVerifier> = external.clone();
        let driver = Arc::new(PasswordCapGateDriver::new(
            repository_port.clone(),
            password.clone(),
            cap,
            resolver.clone(),
            clock.clone(),
        )?);
        let manager = GateManager::new(GateManagerDependencies {
            repository: repository_port,
            routes: repository,
            ingress: Arc::new(UnusedGateIngress),
            target_locks: Arc::new(TestTargetLock),
            password,
            sessions: Arc::new(XChaChaSessionCipher),
            drivers: GateDriverRegistry::new(vec![driver])?,
            cap_sites: external.clone(),
            secret_inspector: external.clone(),
            secret_resolver: resolver,
            secret_creator: external.clone(),
            secret_remover: external,
            clock,
        });
        let ingress_token = "a".repeat(64);
        Ok(Self {
            _directory: directory,
            state: GateHttpState {
                ingress_token_hash: blake3::hash(ingress_token.as_bytes()),
                gates: manager,
            },
            policy_id: policy.id,
            ingress_token,
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Ok(value) = HeaderValue::from_str(&self.ingress_token) {
            headers.insert("x-mm-ingress-auth", value);
        }
        headers.insert("host", HeaderValue::from_static("app.example.test"));
        headers.insert(
            "x-forwarded-host",
            HeaderValue::from_static("app.example.test"),
        );
        headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        headers.insert("x-forwarded-uri", HeaderValue::from_static("/private"));
        headers
    }
}

struct TestGateClock;

impl Clock for TestGateClock {
    fn now_unix_ms(&self) -> i64 {
        1_700_000_000_000
    }
}

struct TestGateExternal;

#[async_trait]
impl CapTokenVerifier for TestGateExternal {
    async fn verify(
        &self,
        _site_key: &str,
        _site_secret: &SecretMaterial,
        token: &SecretMaterial,
    ) -> Result<bool, MmError> {
        Ok(token.expose(|value| value.starts_with("valid-cap-")))
    }
}

#[async_trait]
impl CapSiteProvisioner for TestGateExternal {
    async fn create(&self, _name: &str) -> Result<ProvisionedCapSite, MmError> {
        Ok(ProvisionedCapSite {
            site_key: "test-site".to_owned(),
            site_secret: material("test-cap-secret")?,
        })
    }

    async fn remove(&self, _site_key: &str) -> Result<(), MmError> {
        Ok(())
    }
}

#[async_trait]
impl SecretMaterialResolver for TestGateExternal {
    async fn resolve(
        &self,
        _scope: &SecretScope,
        reference: &SecretReference,
    ) -> Result<SecretResolution, MmError> {
        let canonical = reference.canonical();
        let value = if canonical.ends_with("session-key/1") {
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        } else if canonical.ends_with("password-pepper") {
            "test-pepper"
        } else {
            "test-cap-secret"
        };
        Ok(SecretResolution {
            metadata: test_secret_metadata(),
            material: Some(material(value)?),
        })
    }
}

#[async_trait]
impl SecretMetadataInspector for TestGateExternal {
    async fn inspect(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
    ) -> Result<SecretMetadata, MmError> {
        Ok(test_secret_metadata())
    }
}

#[async_trait]
impl SecretCreator for TestGateExternal {
    async fn create(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
        _material: SecretMaterial,
        _now_unix_ms: i64,
    ) -> Result<SecretWriteResult, MmError> {
        Ok(SecretWriteResult {
            revision: "test-revision".to_owned(),
            changed: true,
        })
    }
}

#[async_trait]
impl SecretRemover for TestGateExternal {
    async fn unset(
        &self,
        _scope: &SecretScope,
        _reference: &SecretReference,
        _expected: SecretExpectedState,
    ) -> Result<bool, MmError> {
        Ok(true)
    }
}

fn material(value: &str) -> Result<SecretMaterial, MmError> {
    SecretMaterial::new(value.to_owned()).map_err(|message| MmError::Secret { message })
}

fn test_secret_metadata() -> SecretMetadata {
    SecretMetadata {
        status: SecretStatus::Set,
        revision: Some("test-revision".to_owned()),
        created_at_unix_ms: Some(1_700_000_000_000),
        candidate_revision: None,
    }
}
