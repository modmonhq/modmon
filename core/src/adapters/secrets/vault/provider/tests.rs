use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::*;

fn response(status: u16, body: serde_json::Value) -> VaultHttpResponse {
    VaultHttpResponse {
        status,
        headers: BTreeMap::new(),
        body: Zeroizing::new(body.to_string().into_bytes()),
    }
}

struct QueueTransport(Mutex<VecDeque<VaultHttpResponse>>);

#[async_trait]
impl VaultHttpTransport for QueueTransport {
    async fn request(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        self.0
            .lock()
            .map_err(|_| MmError::State {
                message: "Vault response queue lock poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "Vault response queue exhausted".to_owned(),
            })
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        Err(MmError::State {
            message: "byte requests are unused by provider tests".to_owned(),
        })
    }
}

fn health(sealed: bool) -> VaultHttpResponse {
    response(
        if sealed { 503 } else { 200 },
        serde_json::json!({
            "initialized": true, "sealed": sealed, "standby": false, "version": "1.19.0",
        }),
    )
}

fn login() -> VaultHttpResponse {
    response(
        200,
        serde_json::json!({
            "auth": {"client_token": "token", "renewable": true, "lease_duration": 300}
        }),
    )
}

fn stored_metadata(status: u16, destroyed: bool) -> VaultHttpResponse {
    response(
        status,
        serde_json::json!({
            "data": {"current_version": 2, "versions": {"2": {
                "created_time": "2026-08-26T12:00:00Z",
                "deletion_time": "", "destroyed": destroyed,
            }}}
        }),
    )
}

fn read_value(status: u16, value: &str) -> VaultHttpResponse {
    response(
        status,
        serde_json::json!({
            "data": {
                "data": {"value": value},
                "metadata": {"version": 2, "created_time": "2026-08-26T12:00:00Z"}
            }
        }),
    )
}

fn configured_store(
    responses: Vec<VaultHttpResponse>,
) -> Result<
    (
        tempfile::TempDir,
        VaultSecretStore,
        SecretScope,
        SecretReference,
    ),
    Box<dyn std::error::Error>,
> {
    let temporary = crate::test_support::tempdir()?;
    let project_id = ProjectId(uuid::Uuid::from_u128(811));
    let instance =
        crate::domain::secret_backend::ProviderInstanceName::new("vault-test".to_owned())?;
    let connections = Arc::new(VaultConnectionStore::new(
        temporary.path().join("identity.key"),
        temporary.path().join("connections.age"),
    ));
    let ca_pem = "test trust bundle".to_owned();
    connections.save_connection(
        instance.as_str(),
        VaultConnection {
            mode: VaultMode::External,
            address: "https://127.0.0.1:8200".to_owned(),
            namespace: Some("team".to_owned()),
            mount: "secret".to_owned(),
            auth_mount: "approle".to_owned(),
            ca_sha256: super::super::connection_store::ca_sha256(&ca_pem),
            ca_pem,
            project_id,
            credentials: BTreeMap::from([(
                "local".to_owned(),
                super::super::connection_store::VaultAppRoleCredential {
                    role_id: Zeroizing::new("role".to_owned()),
                    secret_id: Zeroizing::new("secret".to_owned()),
                },
            )]),
            allowed_locations: BTreeMap::from([(
                "local".to_owned(),
                vec!["app/password".to_owned()],
            )]),
            managed_installation: None,
        },
    )?;
    let reference =
        SecretReference::new(StorageProviderId::new(instance.as_str())?, "app/password")?;
    let store = VaultSecretStore::new(
        instance,
        VaultMode::External,
        project_id,
        connections,
        Arc::new(QueueTransport(Mutex::new(responses.into()))),
    );
    Ok((temporary, store, SecretScope::new("local"), reference))
}

#[test]
fn health_and_metadata_parsers_cover_supported_error_and_tombstone_states()
-> Result<(), Box<dyn std::error::Error>> {
    for (status, version, healthy) in [
        (200, "1.18.1", true),
        (429, "v1.19.0", false),
        (472, "1.18.9", false),
        (473, "1.19.1", false),
        (501, "1.18.0", false),
        (503, "1.19.0", false),
    ] {
        let health = parse_health(response(
            status,
            serde_json::json!({
                "initialized": true, "sealed": false, "standby": false, "version": version,
            }),
        ))?;
        assert_eq!(health.healthy, healthy);
    }
    assert!(parse_health(response(500, serde_json::json!({}))).is_err());
    for version in ["invalid", "0.19.0", "1.17.0", "1.20.0", "2.18.0"] {
        assert!(
            parse_health(response(
                200,
                serde_json::json!({
                    "initialized": true, "sealed": false, "standby": false, "version": version,
                })
            ))
            .is_err()
        );
    }
    assert!(parse_health(response(200, serde_json::json!({"version": "1.18.0"}))).is_err());

    assert_eq!(
        metadata_state_from_response(response(404, serde_json::json!({})))?
            .0
            .status,
        SecretStatus::Unset
    );
    assert_eq!(
        metadata_state_from_response(response(403, serde_json::json!({})))?
            .0
            .status,
        SecretStatus::AccessDenied
    );
    let metadata = |destroyed: bool, deletion: &str, created: &str| {
        response(
            200,
            serde_json::json!({
                "data": {"current_version": 2, "versions": {"2": {
                    "created_time": created, "deletion_time": deletion, "destroyed": destroyed,
                }}}
            }),
        )
    };
    let (set, version) = metadata_state_from_response(metadata(false, "", "2026-08-26T12:00:00Z"))?;
    assert_eq!(set.status, SecretStatus::Set);
    assert_eq!(version, Some(2));
    assert!(set.created_at_unix_ms.is_some());
    assert_eq!(
        metadata_state_from_response(metadata(true, "", "bad"))?
            .0
            .status,
        SecretStatus::Unset
    );
    assert_eq!(
        metadata_state_from_response(metadata(false, "2026-08-26T12:00:00Z", "bad"))?
            .0
            .status,
        SecretStatus::Unset
    );
    assert!(metadata_state_from_response(response(500, serde_json::json!({}))).is_err());
    assert!(metadata_state_from_response(response(200, serde_json::json!({"bad": true}))).is_err());
    Ok(())
}

#[test]
fn response_revision_material_encoding_timestamp_and_descriptor_helpers_are_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    require_success(&response(204, serde_json::json!({})), "test")?;
    assert!(matches!(
        require_success(&response(403, serde_json::json!({})), "test"),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        require_success(&response(500, serde_json::json!({})), "test"),
        Err(MmError::Secret { .. })
    ));
    for expected in [
        SecretExpectedState {
            active_revision: None,
            candidate_revision: None,
        },
        SecretExpectedState {
            active_revision: Some("0".to_owned()),
            candidate_revision: None,
        },
        SecretExpectedState {
            active_revision: Some("bad".to_owned()),
            candidate_revision: None,
        },
        SecretExpectedState {
            active_revision: Some("1".to_owned()),
            candidate_revision: Some("2".to_owned()),
        },
    ] {
        assert!(expected_version(&expected).is_err());
    }
    assert_eq!(
        expected_version(&SecretExpectedState {
            active_revision: Some("7".to_owned()),
            candidate_revision: None,
        })?,
        7
    );
    assert!(validate_material(&SecretMaterial::new("safe".to_owned())?).is_ok());
    assert!(validate_material(&SecretMaterial::new("unsafe\nvalue".to_owned())?).is_err());
    assert_eq!(encode_segment("safe-name_1.~"), "safe-name_1.~");
    assert_eq!(encode_segment("space value/next"), "space%20value%2Fnext");
    assert!(parse_timestamp("2026-08-26T12:00:00Z").is_some());
    assert!(parse_timestamp("bad").is_none());
    assert_eq!(
        metadata(SecretStatus::Invalid, Some("1".to_owned()), Some(2)).status,
        SecretStatus::Invalid
    );
    assert!(format!("{}", lifecycle_conflict()).contains("lifecycle"));
    let instance =
        crate::domain::secret_backend::ProviderInstanceName::new("vault-test".to_owned())?;
    let descriptor = vault_storage_descriptor(&instance)?;
    assert_eq!(descriptor.id.as_str(), "vault-test");
    assert_eq!(descriptor.operations.len(), 5);
    Ok(())
}

#[tokio::test]
async fn authenticated_vault_lifecycle_covers_reads_recreation_cas_and_deletion()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temporary, store, scope, reference) =
        configured_store(vec![health(false), login(), stored_metadata(200, false)])?;
    assert_eq!(
        store.inspect(&scope, &reference).await?.status,
        SecretStatus::Set
    );
    assert!(format!("{store:?}").contains("External"));

    for (status, value, expected) in [
        (404, "", SecretStatus::Unset),
        (403, "", SecretStatus::AccessDenied),
        (200, "safe-value", SecretStatus::Set),
        (200, "unsafe\nvalue", SecretStatus::Invalid),
    ] {
        let (_temporary, store, scope, reference) =
            configured_store(vec![health(false), login(), read_value(status, value)])?;
        assert_eq!(
            store.resolve(&scope, &reference).await?.metadata.status,
            expected
        );
    }

    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        response(200, serde_json::json!({"data": {"version": 1}})),
    ])?;
    assert!(
        store
            .create(
                &scope,
                &reference,
                SecretMaterial::new("created".to_owned())?,
                1,
                None
            )
            .await?
            .changed
    );

    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        response(409, serde_json::json!({})),
        stored_metadata(200, false),
    ])?;
    assert!(
        !store
            .create(
                &scope,
                &reference,
                SecretMaterial::new("existing".to_owned())?,
                1,
                None
            )
            .await?
            .changed
    );

    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        response(409, serde_json::json!({})),
        stored_metadata(200, true),
        response(200, serde_json::json!({"data": {"version": 3}})),
    ])?;
    let recreated = store
        .create(
            &scope,
            &reference,
            SecretMaterial::new("recreated".to_owned())?,
            1,
            None,
        )
        .await?;
    assert!(recreated.changed);
    assert_eq!(recreated.revision, "3");

    let expected = SecretExpectedState {
        active_revision: Some("2".to_owned()),
        candidate_revision: None,
    };
    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        response(409, serde_json::json!({})),
    ])?;
    assert!(
        store
            .replace(
                &scope,
                &reference,
                SecretMaterial::new("replacement".to_owned())?,
                expected.clone(),
                2,
                None,
            )
            .await
            .is_err()
    );

    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        response(200, serde_json::json!({"data": {"version": 3}})),
    ])?;
    assert_eq!(
        store
            .replace(
                &scope,
                &reference,
                SecretMaterial::new("replacement".to_owned())?,
                expected.clone(),
                2,
                None,
            )
            .await?,
        "3"
    );

    let (_temporary, store, scope, reference) =
        configured_store(vec![health(false), login(), stored_metadata(404, false)])?;
    assert!(!store.unset(&scope, &reference, expected.clone()).await?);

    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        stored_metadata(200, false),
        response(204, serde_json::json!({})),
        stored_metadata(200, true),
    ])?;
    assert!(store.unset(&scope, &reference, expected).await?);

    let (_temporary, store, scope, reference) = configured_store(vec![health(true)])?;
    assert_eq!(
        store.inspect(&scope, &reference).await?.status,
        SecretStatus::ProviderUnavailable
    );
    Ok(())
}
