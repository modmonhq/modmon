#![cfg(unix)]

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mm::adapters::secrets::{
    AppRoleCredential, OpenBaoConnection, OpenBaoConnectionStore, OpenBaoHttpResponse,
    OpenBaoHttpTransport, OpenBaoMode, OpenBaoSecretStore, ca_sha256, environment_policy,
    openbao_storage_descriptor,
};
use mm::app::ports::{
    SecretCreator, SecretMaterialResolver, SecretMetadataInspector, SecretRemover, SecretReplacer,
};
use mm::domain::secret::{
    SecretExpectedState, SecretMaterial, SecretReference, SecretScope, SecretStatus,
};
use mm::domain::secret_contract::{
    AuthenticationKind, RevisionSemantics, RotationCapability, StorageOperation, StorageProviderId,
};
use mm::domain::workspace::ProjectId;
use reqwest::Method;
use uuid::Uuid;
use zeroize::Zeroizing;

mod temp_dirs;

const SENTINEL: &str = "mm-openbao-secret-sentinel-7a64c1";
const PROJECT_A: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de3";
const PROJECT_B: &str = "019c527b-6242-7b43-bfd4-bd60e27c1de4";

#[derive(Default)]
struct FakeTransport {
    state: Mutex<FakeState>,
}

#[derive(Default)]
struct FakeState {
    values: BTreeMap<String, (u64, Zeroizing<String>)>,
    requests: Vec<(Method, String, bool)>,
    forced: VecDeque<OpenBaoHttpResponse>,
}

#[async_trait]
impl OpenBaoHttpTransport for FakeTransport {
    async fn request(
        &self,
        _connection: &OpenBaoConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<OpenBaoHttpResponse, mm::MmError> {
        let mut state = self.state.lock().map_err(|_| mm::MmError::State {
            message: "fake provider lock poisoned".to_owned(),
        })?;
        state
            .requests
            .push((method.clone(), path.to_owned(), token.is_some()));
        if let Some(response) = state.forced.pop_front() {
            return Ok(response);
        }
        if path == "sys/health" {
            return response(
                200,
                serde_json::json!({
                    "initialized": true,
                    "sealed": false,
                    "standby": false,
                    "version": "2.4.4"
                }),
            );
        }
        if path.ends_with("/login") {
            return response(
                200,
                serde_json::json!({
                    "auth": {
                        "client_token": "ephemeral-token",
                        "renewable": true,
                        "lease_duration": 900
                    }
                }),
            );
        }
        if path.contains("/metadata/") {
            let data_path = path.replacen("/metadata/", "/data/", 1);
            return state.values.get(&data_path).map_or_else(
                || response(404, serde_json::json!({"errors": []})),
                |(version, _)| {
                    response(
                        200,
                        serde_json::json!({
                            "data": {
                                "current_version": version,
                                "versions": {
                                    version.to_string(): {
                                        "created_time": "2026-08-23T12:00:00Z",
                                        "deletion_time": "",
                                        "destroyed": false
                                    }
                                }
                            }
                        }),
                    )
                },
            );
        }
        if path.contains("/delete/") && method == Method::POST {
            let data_path = path.replacen("/delete/", "/data/", 1);
            state.values.remove(&data_path);
            return response(204, serde_json::json!({}));
        }
        if path.contains("/data/") && method == Method::GET {
            return state.values.get(path).map_or_else(
                || response(404, serde_json::json!({"errors": []})),
                |(version, value)| {
                    response(
                        200,
                        serde_json::json!({
                            "data": {
                                "data": {"value": value.as_str()},
                                "metadata": {
                                    "created_time": "2026-08-23T12:00:00Z",
                                    "version": version
                                }
                            }
                        }),
                    )
                },
            );
        }
        if path.contains("/data/") && method == Method::POST {
            let body = body.ok_or_else(|| mm::MmError::State {
                message: "fake write had no body".to_owned(),
            })?;
            let cas = body
                .pointer("/options/cas")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| mm::MmError::State {
                    message: "fake write had no CAS".to_owned(),
                })?;
            let current = state.values.get(path).map(|(version, _)| *version);
            if (cas == 0 && current.is_some()) || (cas > 0 && current != Some(cas)) {
                return response(400, serde_json::json!({"errors": ["check-and-set"]}));
            }
            let value = body
                .pointer("/data/value")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| mm::MmError::State {
                    message: "fake write had no value".to_owned(),
                })?;
            let version = current.unwrap_or(0) + 1;
            state
                .values
                .insert(path.to_owned(), (version, Zeroizing::new(value.to_owned())));
            return response(200, serde_json::json!({"data": {"version": version}}));
        }
        response(404, serde_json::json!({"errors": []}))
    }

    async fn request_bytes(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<OpenBaoHttpResponse, mm::MmError> {
        response(200, serde_json::json!({}))
    }
}

#[tokio::test]
async fn provider_contract_is_scoped_versioned_cas_and_redacted()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let project = project(PROJECT_A)?;
    let connections = Arc::new(OpenBaoConnectionStore::new(
        home.path().join("identity.key"),
        home.path().join("connections.age"),
    ));
    connections.save_connection("external", connection(project, &["integration", "uat"]))?;
    let transport = Arc::new(FakeTransport::default());
    let instance = mm::domain::secret_backend::ProviderInstanceName::new("team-secrets")?;
    let provider = OpenBaoSecretStore::new(
        instance.clone(),
        OpenBaoMode::External,
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
    assert_eq!(
        provider.inspect(&integration, &reference).await?.status,
        SecretStatus::Unset
    );
    let created = provider
        .create(
            &integration,
            &reference,
            SecretMaterial::new(SENTINEL.to_owned())?,
            1,
            None,
        )
        .await?;
    assert_eq!(created.revision, "1");
    assert!(created.changed);
    assert_eq!(
        provider.inspect(&uat, &reference).await?.status,
        SecretStatus::Unset
    );
    let resolved = provider.resolve(&integration, &reference).await?;
    assert!(
        resolved
            .material
            .is_some_and(|material| material.expose(|value| value == SENTINEL))
    );
    let stale = provider
        .replace(
            &integration,
            &reference,
            SecretMaterial::new("replacement".to_owned())?,
            SecretExpectedState {
                active_revision: Some("9".to_owned()),
                candidate_revision: None,
            },
            2,
            None,
        )
        .await;
    assert!(stale.is_err());
    let next = provider
        .replace(
            &integration,
            &reference,
            SecretMaterial::new("replacement".to_owned())?,
            SecretExpectedState {
                active_revision: Some("1".to_owned()),
                candidate_revision: None,
            },
            2,
            None,
        )
        .await?;
    assert_eq!(next, "2");
    let left = provider.clone();
    let right = provider.clone();
    let (left_result, right_result) = tokio::join!(
        left.replace(
            &integration,
            &reference,
            SecretMaterial::new("concurrent-left".to_owned())?,
            SecretExpectedState {
                active_revision: Some(next.clone()),
                candidate_revision: None,
            },
            3,
            None,
        ),
        right.replace(
            &integration,
            &reference,
            SecretMaterial::new("concurrent-right".to_owned())?,
            SecretExpectedState {
                active_revision: Some(next),
                candidate_revision: None,
            },
            3,
            None,
        )
    );
    assert_ne!(left_result.is_ok(), right_result.is_ok());
    let concurrent_revision = left_result.or(right_result)?;
    assert_eq!(concurrent_revision, "3");
    assert!(
        provider
            .unset(
                &integration,
                &reference,
                SecretExpectedState {
                    active_revision: Some(concurrent_revision),
                    candidate_revision: None,
                }
            )
            .await?
    );
    assert_eq!(
        provider.inspect(&integration, &reference).await?.status,
        SecretStatus::Unset
    );

    let requests = transport.state.lock().map_err(|_| "fake lock")?;
    assert!(
        requests
            .requests
            .iter()
            .all(|(_, path, _)| !path.contains(SENTINEL))
    );
    assert!(requests.requests.iter().any(|(_, path, _)| {
        path.contains(&format!("mm/{}/integration/database/password", project.0))
    }));
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
async fn ownership_auth_version_and_tls_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let owner = project(PROJECT_A)?;
    let other = project(PROJECT_B)?;
    let connections = Arc::new(OpenBaoConnectionStore::new(
        home.path().join("identity.key"),
        home.path().join("connections.age"),
    ));
    connections.save_connection("external", connection(owner, &["integration"]))?;
    let transport = Arc::new(FakeTransport::default());
    let instance = mm::domain::secret_backend::ProviderInstanceName::new("team-secrets")?;
    let reference = SecretReference::with_connection(
        StorageProviderId::new(instance.as_str())?,
        "external",
        "value",
    )?;
    let wrong_project = OpenBaoSecretStore::new(
        instance.clone(),
        OpenBaoMode::External,
        other,
        Arc::clone(&connections),
        transport.clone(),
    );
    assert!(
        wrong_project
            .inspect(&SecretScope::new("integration"), &reference)
            .await
            .is_err()
    );
    let owner_provider = OpenBaoSecretStore::new(
        instance,
        OpenBaoMode::External,
        owner,
        Arc::clone(&connections),
        transport.clone(),
    );
    assert!(
        owner_provider
            .inspect(&SecretScope::new("production"), &reference)
            .await
            .is_err()
    );
    transport
        .state
        .lock()
        .map_err(|_| "fake lock")?
        .forced
        .push_back(response(
            200,
            serde_json::json!({
                "initialized": true,
                "sealed": false,
                "standby": false,
                "version": "2.6.0"
            }),
        )?);
    let version_error = owner_provider
        .inspect(&SecretScope::new("integration"), &reference)
        .await
        .err()
        .ok_or("unsupported version accepted")?;
    assert!(version_error.to_string().contains("tested server lines"));

    let mut bad = connection(owner, &["integration"]);
    bad.ca_sha256 =
        "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned();
    assert!(connections.save_connection("bad-pin", bad).is_err());
    Ok(())
}

#[test]
fn capabilities_and_policies_are_truthful_and_isolated() -> Result<(), Box<dyn std::error::Error>> {
    for name in ["managed-secrets", "external-secrets"] {
        let instance = mm::domain::secret_backend::ProviderInstanceName::new(name)?;
        let descriptor = openbao_storage_descriptor(&instance)?;
        assert_eq!(
            descriptor.revision_semantics,
            RevisionSemantics::CompareAndSwap
        );
        assert_eq!(descriptor.rotation, RotationCapability::Replace);
        assert_eq!(
            descriptor.authentication,
            AuthenticationKind::AppRoleEphemeralToken
        );
        assert!(descriptor.metadata_without_material_resolution);
        assert!(descriptor.supports(StorageOperation::Create));
        assert!(descriptor.supports(StorageOperation::Replace));
        assert!(descriptor.supports(StorageOperation::Delete));
        assert!(!descriptor.supports(StorageOperation::ReplaceBatch));
        assert!(!descriptor.supports(StorageOperation::StageCandidate));
    }
    let project = project(PROJECT_A)?;
    let connection = connection(project, &["integration"]);
    let locations = vec!["application/database-password".to_owned()];
    let integration = environment_policy(&connection, project, "integration", &locations);
    assert!(integration.contains(&format!(
        "mm/{}/integration/application/database-password\"",
        project.0
    )));
    assert!(!integration.contains("integration/*"));
    assert!(!integration.contains("undeclared"));
    assert!(!integration.contains("uat"));
    assert!(!integration.contains("list"));
    assert!(!format!("{connection:?}").contains("role-id"));
    assert!(!format!("{connection:?}").contains("secret-id"));
    Ok(())
}

#[test]
fn encrypted_backup_detects_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = OpenBaoConnectionStore::new(
        home.path().join("identity.key"),
        home.path().join("connections.age"),
    );
    store.save_connection(
        "external",
        connection(project(PROJECT_A)?, &["integration"]),
    )?;
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
fn openbao_connection_and_recovery_registry_lifecycle_rejects_malformed_state()
-> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = OpenBaoConnectionStore::new(
        home.path().join("identity.key"),
        home.path().join("connections.age"),
    );
    assert!(!store.has_connection("external")?);
    assert!(store.connection("external").is_err());
    let configured = connection(project(PROJECT_A)?, &["integration"]);
    store.save_connection("external", configured.clone())?;
    assert!(store.has_connection("external")?);
    assert_eq!(store.connection("external")?.mode, OpenBaoMode::External);
    assert!(store.remove_connection("external")?);
    assert!(!store.remove_connection("external")?);

    let recovery = mm::adapters::secrets::ManagedRecoveryMaterial {
        root_token: Zeroizing::new("root-token".to_owned()),
        unseal_key: Zeroizing::new("unseal-key".to_owned()),
    };
    assert!(format!("{recovery:?}").contains("REDACTED"));
    assert!(!store.has_recovery("managed")?);
    assert!(store.recovery("managed").is_err());
    store.save_recovery("managed", recovery)?;
    assert!(store.has_recovery("managed")?);
    assert!(format!("{:?}", store.recovery("managed")?).contains("REDACTED"));
    assert!(store.remove_recovery("managed")?);
    assert!(!store.remove_recovery("managed")?);

    assert!(store.has_connection("../escape").is_err());
    let mut invalid = configured;
    invalid.address = "not-a-url".to_owned();
    assert!(store.save_connection("invalid", invalid).is_err());
    assert_eq!(
        OpenBaoMode::from(mm::domain::secret_backend::ProviderMode::Managed),
        OpenBaoMode::Managed
    );

    let malformed_home = temp_dirs::tempdir()?;
    let identity = malformed_home.path().join("identity.key");
    fs::write(&identity, b"not-an-age-identity")?;
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(&identity, fs::Permissions::from_mode(0o600))?;
    let malformed =
        OpenBaoConnectionStore::new(identity, malformed_home.path().join("connections.age"));
    assert!(
        malformed
            .decrypt_integrity_checked(b"not-an-envelope")
            .is_err()
    );
    Ok(())
}

fn connection(project_id: ProjectId, environments: &[&str]) -> OpenBaoConnection {
    let ca_pem = "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n".to_owned();
    OpenBaoConnection {
        mode: OpenBaoMode::External,
        address: "https://bao.example.test:8200".to_owned(),
        namespace: None,
        mount: "mm-kv".to_owned(),
        auth_mount: "mm-approle".to_owned(),
        ca_sha256: ca_sha256(&ca_pem),
        ca_pem,
        project_id,
        credentials: environments
            .iter()
            .map(|environment| {
                (
                    (*environment).to_owned(),
                    AppRoleCredential {
                        role_id: Zeroizing::new(format!("role-id-{environment}")),
                        secret_id: Zeroizing::new(format!("secret-id-{environment}")),
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

fn response(status: u16, body: serde_json::Value) -> Result<OpenBaoHttpResponse, mm::MmError> {
    Ok(OpenBaoHttpResponse {
        status,
        headers: BTreeMap::new(),
        body: Zeroizing::new(
            serde_json::to_vec(&body).map_err(|error| mm::MmError::State {
                message: format!("could not serialize fake response: {error}"),
            })?,
        ),
    })
}
