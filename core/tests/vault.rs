#![cfg(unix)]

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::secrets::{
    VaultAppRoleCredential, VaultConnection, VaultConnectionStore, VaultHttpResponse,
    VaultHttpTransport, VaultMode, VaultSecretStore, vault_ca_sha256, vault_environment_policy,
    vault_storage_descriptor,
};
use mm::app::ports::{
    SecretCreator, SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
};
use mm::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretReference, SecretScope, SecretStatus,
};
use mm::domain::secret_contract::{
    AuthenticationKind, RevisionSemantics, StorageOperation, StorageProviderId,
};
use mm::domain::workspace::ProjectId;
use reqwest::Method;
use uuid::Uuid;
use zeroize::Zeroizing;

mod temp_dirs;

const SENTINEL: &str = "mm-vault-secret-sentinel-931ef0";
const PROJECT_A: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";
const PROJECT_B: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de4";

#[derive(Default)]
struct FakeTransport {
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    values: BTreeMap<String, (u64, Zeroizing<String>)>,
    deleted: BTreeMap<String, u64>,
    requests: Vec<String>,
    forced: VecDeque<VaultHttpResponse>,
}

#[async_trait]
impl VaultHttpTransport for FakeTransport {
    async fn request(
        &self,
        _connection: &VaultConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, mm::MmError> {
        let mut state = self.state.lock().map_err(|_| mm::MmError::State {
            message: "fake Vault lock poisoned".to_owned(),
        })?;
        state.requests.push(format!(
            "{} {path} authenticated={}",
            method.as_str(),
            token.is_some()
        ));
        if let Some(response) = state.forced.pop_front() {
            return Ok(response);
        }
        if path == "sys/health" {
            return response(
                200,
                serde_json::json!({"initialized": true, "sealed": false, "standby": false, "version": "1.19.5"}),
            );
        }
        if path.ends_with("/login") {
            return response(
                200,
                serde_json::json!({"auth": {"client_token": "short-lived-token", "renewable": true, "lease_duration": 900}}),
            );
        }
        if path.contains("/metadata/") {
            let data_path = path.replacen("/metadata/", "/data/", 1);
            if let Some(version) = state.deleted.get(&data_path) {
                return response(
                    200,
                    serde_json::json!({"data": {"current_version": version, "versions": {version.to_string(): {"created_time": "2026-08-23T12:00:00Z", "deletion_time": "2026-08-23T12:01:00Z", "destroyed": false}}}}),
                );
            }
            return state.values.get(&data_path).map_or_else(|| response(404, serde_json::json!({"errors": []})), |(version, _)| response(200, serde_json::json!({"data": {"current_version": version, "versions": {version.to_string(): {"created_time": "2026-08-23T12:00:00Z", "deletion_time": "", "destroyed": false}}}})));
        }
        if path.contains("/delete/") && method == Method::POST {
            let data_path = path.replacen("/delete/", "/data/", 1);
            if let Some((version, _)) = state.values.remove(&data_path) {
                state.deleted.insert(data_path, version);
            }
            return response(204, serde_json::json!({}));
        }
        if path.contains("/data/") && method == Method::GET {
            return state.values.get(path).map_or_else(|| response(404, serde_json::json!({"errors": []})), |(version, value)| response(200, serde_json::json!({"data": {"data": {"value": value.as_str()}, "metadata": {"created_time": "2026-08-23T12:00:00Z", "version": version}}})));
        }
        if path.contains("/data/") && method == Method::POST {
            let body = body.ok_or_else(|| mm::MmError::State {
                message: "fake Vault write body missing".to_owned(),
            })?;
            let cas = body
                .pointer("/options/cas")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| mm::MmError::State {
                    message: "fake Vault CAS missing".to_owned(),
                })?;
            let current = state
                .values
                .get(path)
                .map(|(version, _)| *version)
                .or_else(|| state.deleted.get(path).copied());
            if (cas == 0 && current.is_some()) || (cas > 0 && current != Some(cas)) {
                return response(
                    400,
                    serde_json::json!({"errors": ["check-and-set parameter did not match"]}),
                );
            }
            let value = body
                .pointer("/data/value")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| mm::MmError::State {
                    message: "fake Vault value missing".to_owned(),
                })?;
            let version = current.unwrap_or(0) + 1;
            state
                .values
                .insert(path.to_owned(), (version, Zeroizing::new(value.to_owned())));
            state.deleted.remove(path);
            return response(200, serde_json::json!({"data": {"version": version}}));
        }
        response(404, serde_json::json!({"errors": []}))
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, mm::MmError> {
        response(200, serde_json::json!({}))
    }
}

#[tokio::test]
async fn vault_contract_has_independent_cas_path_and_redaction()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let project = project(PROJECT_A)?;
    let connections = Arc::new(VaultConnectionStore::new(
        home.path().join("vault.key"),
        home.path().join("vault.age"),
    ));
    connections.save_connection("external", connection(project, &["integration", "uat"]))?;
    let transport = Arc::new(FakeTransport::default());
    let instance = mm::domain::secret_backend::ProviderInstanceName::new("team-secrets")?;
    let store = VaultSecretStore::new(
        instance.clone(),
        VaultMode::External,
        project,
        Arc::clone(&connections),
        transport.clone(),
    );
    let reference = SecretReference::with_connection(
        StorageProviderId::new(instance.as_str())?,
        "external",
        "database/password",
    )?;
    let integration = SecretScope::new("integration");
    let uat = SecretScope::new("uat");
    let created = store
        .create(
            &integration,
            &reference,
            SecretMaterial::new(SENTINEL.to_owned())?,
            1,
        )
        .await?;
    assert_eq!(created.revision, "1");
    assert_eq!(
        store.inspect(&uat, &reference).await?.status,
        SecretStatus::Unset
    );
    assert!(
        store
            .resolve(&integration, &reference)
            .await?
            .material
            .is_some_and(|value| value.expose(|value| value == SENTINEL))
    );
    let expected = SecretExpectedState {
        active_revision: Some("1".to_owned()),
        candidate_revision: None,
    };
    let (left, right) = tokio::join!(
        store.replace(
            &integration,
            &reference,
            SecretMaterial::new("left".to_owned())?,
            expected.clone(),
            2
        ),
        store.replace(
            &integration,
            &reference,
            SecretMaterial::new("right".to_owned())?,
            expected,
            2
        )
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let revision = left.or(right)?;
    assert_eq!(revision, "2");
    assert!(
        store
            .unset(
                &integration,
                &reference,
                SecretExpectedState {
                    active_revision: Some(revision),
                    candidate_revision: None
                }
            )
            .await?
    );
    let recreated = store
        .create(
            &integration,
            &reference,
            SecretMaterial::new("recreated".to_owned())?,
            3,
        )
        .await?;
    assert!(recreated.changed);
    assert_eq!(recreated.revision, "3");
    let requests = transport.state.lock().map_err(|_| "fake lock")?;
    assert!(requests.requests.iter().any(|request| {
        request.contains(&format!("mm/{}/integration/database/password", project.0))
    }));
    assert!(
        requests
            .requests
            .iter()
            .all(|request| !request.contains(SENTINEL))
    );
    drop(requests);
    assert!(!format!("{:?}", connections.connection("external")?).contains(SENTINEL));
    for entry in fs::read_dir(home.path())? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let bytes = fs::read(entry.path())?;
            assert!(
                !bytes
                    .windows(SENTINEL.len())
                    .any(|part| part == SENTINEL.as_bytes())
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn vault_version_ownership_environment_and_tls_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let owner = project(PROJECT_A)?;
    let connections = Arc::new(VaultConnectionStore::new(
        home.path().join("vault.key"),
        home.path().join("vault.age"),
    ));
    connections.save_connection("external", connection(owner, &["integration"]))?;
    let transport = Arc::new(FakeTransport::default());
    let instance = mm::domain::secret_backend::ProviderInstanceName::new("team-secrets")?;
    let reference = SecretReference::with_connection(
        StorageProviderId::new(instance.as_str())?,
        "external",
        "value",
    )?;
    let wrong = VaultSecretStore::new(
        instance.clone(),
        VaultMode::External,
        project(PROJECT_B)?,
        Arc::clone(&connections),
        transport.clone(),
    );
    assert!(
        wrong
            .inspect(&SecretScope::new("integration"), &reference)
            .await
            .is_err()
    );
    let store = VaultSecretStore::new(
        instance,
        VaultMode::External,
        owner,
        Arc::clone(&connections),
        transport.clone(),
    );
    assert!(
        store
            .inspect(&SecretScope::new("production"), &reference)
            .await
            .is_err()
    );
    for version in ["1.18.5", "1.19.5"] {
        transport.state.lock().map_err(|_| "fake lock")?.forced.push_back(response(200, serde_json::json!({"initialized": true, "sealed": false, "standby": false, "version": version}))?);
        transport.state.lock().map_err(|_| "fake lock")?.forced.push_back(response(200, serde_json::json!({"auth": {"client_token": "token", "renewable": true, "lease_duration": 900}}))?);
        transport
            .state
            .lock()
            .map_err(|_| "fake lock")?
            .forced
            .push_back(response(404, serde_json::json!({"errors": []}))?);
        assert_eq!(
            store
                .inspect(&SecretScope::new("integration"), &reference)
                .await?
                .status,
            SecretStatus::Unset
        );
    }
    transport.state.lock().map_err(|_| "fake lock")?.forced.push_back(response(200, serde_json::json!({"initialized": true, "sealed": false, "standby": false, "version": "1.20.0"}))?);
    assert!(
        store
            .inspect(&SecretScope::new("integration"), &reference)
            .await
            .err()
            .is_some_and(|error| error.to_string().contains("tested Community server lines"))
    );
    let mut bad = connection(owner, &["integration"]);
    bad.ca_sha256 = format!("sha256:{}", "f".repeat(64));
    assert!(connections.save_connection("bad-pin", bad).is_err());
    Ok(())
}

#[test]
fn vault_capabilities_policy_and_private_backup_are_truthful()
-> Result<(), Box<dyn std::error::Error>> {
    for name in ["managed-secrets", "external-secrets"] {
        let instance = mm::domain::secret_backend::ProviderInstanceName::new(name)?;
        let descriptor = vault_storage_descriptor(&instance)?;
        assert_eq!(
            descriptor.revision_semantics,
            RevisionSemantics::CompareAndSwap
        );
        assert_eq!(
            descriptor.authentication,
            AuthenticationKind::AppRoleEphemeralToken
        );
        assert!(descriptor.supports(StorageOperation::Create));
        assert!(descriptor.supports(StorageOperation::Replace));
        assert!(descriptor.supports(StorageOperation::Delete));
        assert!(!descriptor.supports(StorageOperation::ReplaceBatch));
        assert!(!descriptor.supports(StorageOperation::StageCandidate));
    }
    let project = project(PROJECT_A)?;
    let connection = connection(project, &["integration"]);
    let policy = vault_environment_policy(
        &connection,
        project,
        "integration",
        &["application/database-password".to_owned()],
    );
    assert!(policy.contains(&format!(
        "mm/{}/integration/application/database-password\"",
        project.0
    )));
    assert!(!policy.contains("integration/*"));
    assert!(!policy.contains("uat"));
    assert!(!policy.contains("list"));
    assert!(!policy.contains("sudo"));
    let home = temp_dirs::tempdir()?;
    let store =
        VaultConnectionStore::new(home.path().join("vault.key"), home.path().join("vault.age"));
    let encrypted = store.encrypt_integrity_checked(SENTINEL.as_bytes())?;
    assert!(
        !encrypted
            .windows(SENTINEL.len())
            .any(|part| part == SENTINEL.as_bytes())
    );
    assert_eq!(
        store.decrypt_integrity_checked(&encrypted)?.as_slice(),
        SENTINEL.as_bytes()
    );
    let mut tampered = encrypted;
    let index = tampered.len() / 2;
    tampered[index] ^= 1;
    assert!(store.decrypt_integrity_checked(&tampered).is_err());
    Ok(())
}

#[test]
fn vault_connection_and_recovery_registry_lifecycle_rejects_malformed_state()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store =
        VaultConnectionStore::new(home.path().join("vault.key"), home.path().join("vault.age"));
    assert!(!store.has_connection("external")?);
    assert!(store.connection("external").is_err());
    let configured = connection(project(PROJECT_A)?, &["integration"]);
    store.save_connection("external", configured.clone())?;
    assert!(store.has_connection("external")?);
    assert_eq!(store.connection("external")?.mode, VaultMode::External);
    assert!(store.remove_connection("external")?);
    assert!(!store.remove_connection("external")?);

    assert!(!store.has_recovery("managed")?);
    assert!(store.recovery("managed").is_err());
    assert!(!store.remove_recovery("managed")?);

    assert!(store.has_connection("../escape").is_err());
    let mut invalid = configured;
    invalid.address = "not-a-url".to_owned();
    assert!(store.save_connection("invalid", invalid).is_err());
    assert_eq!(
        VaultMode::from(mm::domain::secret_backend::ProviderMode::Managed),
        VaultMode::Managed
    );

    let malformed_home = temp_dirs::tempdir()?;
    let identity = malformed_home.path().join("identity.key");
    fs::write(&identity, b"not-an-age-identity")?;
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&identity, fs::Permissions::from_mode(0o600))?;
    let malformed = VaultConnectionStore::new(identity, malformed_home.path().join("store.age"));
    assert!(
        malformed
            .decrypt_integrity_checked(b"not-an-envelope")
            .is_err()
    );
    Ok(())
}

fn connection(project_id: ProjectId, environments: &[&str]) -> VaultConnection {
    let ca_pem = "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n".to_owned();
    VaultConnection {
        mode: VaultMode::External,
        address: "https://vault.example.test:8200".to_owned(),
        namespace: None,
        mount: "mm-kv".to_owned(),
        auth_mount: "mm-approle".to_owned(),
        ca_sha256: vault_ca_sha256(&ca_pem),
        ca_pem,
        project_id,
        credentials: environments
            .iter()
            .map(|environment| {
                (
                    (*environment).to_owned(),
                    VaultAppRoleCredential {
                        role_id: Zeroizing::new(format!("role-{environment}")),
                        secret_id: Zeroizing::new(format!("secret-{environment}")),
                    },
                )
            })
            .collect(),
        allowed_locations: environments
            .iter()
            .map(|environment| {
                (
                    (*environment).to_owned(),
                    vec!["application/database-password".to_owned()],
                )
            })
            .collect(),
        managed_installation: None,
    }
}
fn project(value: &str) -> Result<ProjectId, uuid::Error> {
    Uuid::parse_str(value).map(ProjectId)
}
fn response(status: u16, body: serde_json::Value) -> Result<VaultHttpResponse, mm::MmError> {
    Ok(VaultHttpResponse {
        status,
        headers: BTreeMap::new(),
        body: Zeroizing::new(
            serde_json::to_vec(&body).map_err(|error| mm::MmError::State {
                message: format!("fake response serialization failed: {error}"),
            })?,
        ),
    })
}
