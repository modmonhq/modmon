use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::*;

fn response(status: u16, body: serde_json::Value) -> OpenBaoHttpResponse {
    OpenBaoHttpResponse {
        status,
        headers: BTreeMap::new(),
        body: Zeroizing::new(body.to_string().into_bytes()),
    }
}

struct QueueTransport(Mutex<VecDeque<OpenBaoHttpResponse>>);

#[async_trait]
impl OpenBaoHttpTransport for QueueTransport {
    async fn request(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        _path: &str,
        _token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        self.0
            .lock()
            .map_err(|_| MmError::State {
                message: "OpenBao response queue lock poisoned".to_owned(),
            })?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "OpenBao response queue exhausted".to_owned(),
            })
    }

    async fn request_bytes(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        Err(MmError::State {
            message: "byte requests are unused by provider tests".to_owned(),
        })
    }
}

fn health(sealed: bool) -> OpenBaoHttpResponse {
    response(
        if sealed { 503 } else { 200 },
        serde_json::json!({
            "initialized": true, "sealed": sealed, "standby": false, "version": "2.4.0",
        }),
    )
}

fn login() -> OpenBaoHttpResponse {
    response(
        200,
        serde_json::json!({
            "auth": {"client_token": "token", "renewable": true, "lease_duration": 300}
        }),
    )
}

fn stored_metadata(status: u16, destroyed: bool) -> OpenBaoHttpResponse {
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

fn read_value(status: u16, value: &str) -> OpenBaoHttpResponse {
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
    responses: Vec<OpenBaoHttpResponse>,
) -> Result<
    (
        tempfile::TempDir,
        OpenBaoSecretStore,
        SecretScope,
        SecretReference,
    ),
    Box<dyn std::error::Error>,
> {
    store_over(Arc::new(QueueTransport(Mutex::new(responses.into()))))
}

fn store_over(
    transport: Arc<dyn OpenBaoHttpTransport>,
) -> Result<
    (
        tempfile::TempDir,
        OpenBaoSecretStore,
        SecretScope,
        SecretReference,
    ),
    Box<dyn std::error::Error>,
> {
    let temporary = crate::test_support::tempdir()?;
    let project_id = ProjectId(uuid::Uuid::from_u128(801));
    let instance =
        crate::domain::secret_backend::ProviderInstanceName::new("openbao-test".to_owned())?;
    let connections = Arc::new(OpenBaoConnectionStore::new(
        temporary.path().join("identity.key"),
        temporary.path().join("connections.age"),
    ));
    let ca_pem = "test trust bundle".to_owned();
    connections.save_connection(
        instance.as_str(),
        OpenBaoConnection {
            mode: OpenBaoMode::External,
            address: "https://127.0.0.1:8200".to_owned(),
            namespace: Some("team".to_owned()),
            mount: "secret".to_owned(),
            auth_mount: "approle".to_owned(),
            ca_sha256: super::super::connection_store::ca_sha256(&ca_pem),
            ca_pem,
            project_id,
            credentials: BTreeMap::from([(
                "local".to_owned(),
                super::super::connection_store::AppRoleCredential {
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
    let store = OpenBaoSecretStore::new(
        instance,
        OpenBaoMode::External,
        project_id,
        connections,
        transport,
    );
    Ok((temporary, store, SecretScope::new("local"), reference))
}

#[test]
fn health_metadata_http_revision_and_value_helpers_cover_all_response_classes()
-> Result<(), Box<dyn std::error::Error>> {
    for (status, version, healthy) in [
        (200, "2.3.1", true),
        (429, "v2.4.0", false),
        (472, "2.3.9", false),
        (473, "2.4.1", false),
        (501, "2.3.0", false),
        (503, "2.4.0", false),
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
    for version in ["invalid", "1.9.0", "2.2.0", "2.6.0"] {
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
    assert!(parse_health(response(200, serde_json::json!({"version": "2.3.0"}))).is_err());

    assert_eq!(
        metadata_from_response(response(404, serde_json::json!({})))?.status,
        SecretStatus::Unset
    );
    assert_eq!(
        metadata_from_response(response(403, serde_json::json!({})))?.status,
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
    let set = metadata_from_response(metadata(false, "", "2026-08-26T12:00:00Z"))?;
    assert_eq!(set.status, SecretStatus::Set);
    assert!(set.created_at_unix_ms.is_some());
    assert_eq!(
        metadata_from_response(metadata(true, "", "bad"))?.status,
        SecretStatus::Unset
    );
    assert_eq!(
        metadata_from_response(metadata(false, "2026-08-26T12:00:00Z", "bad"))?.status,
        SecretStatus::Unset
    );
    assert!(metadata_from_response(response(500, serde_json::json!({}))).is_err());
    assert!(metadata_from_response(response(200, serde_json::json!({"bad": true}))).is_err());

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
    Ok(())
}

#[test]
fn encoding_timestamp_material_unavailable_and_descriptor_helpers_are_bounded()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(encode_segment("safe-name_1.~"), "safe-name_1.~");
    assert_eq!(encode_segment("space value/next"), "space%20value%2Fnext");
    assert!(parse_timestamp("2026-08-26T12:00:00Z").is_some());
    assert!(parse_timestamp("not-a-time").is_none());
    assert!(validate_material(&SecretMaterial::new("safe".to_owned())?).is_ok());
    assert!(validate_material(&SecretMaterial::new("unsafe\nvalue".to_owned())?).is_err());
    assert!(unavailable(&MmError::Secret {
        message: "provider unavailable".to_owned()
    }));
    assert!(unavailable(&MmError::Secret {
        message: "provider sealed".to_owned()
    }));
    assert!(!unavailable(&MmError::Security {
        message: "denied".to_owned()
    }));
    assert_eq!(
        unavailable_metadata().status,
        SecretStatus::ProviderUnavailable
    );
    assert_eq!(unset_metadata().status, SecretStatus::Unset);
    assert_eq!(access_denied_metadata().status, SecretStatus::AccessDenied);
    assert!(format!("{}", lifecycle_conflict()).contains("lifecycle"));
    let instance =
        crate::domain::secret_backend::ProviderInstanceName::new("openbao-test".to_owned())?;
    let descriptor = openbao_storage_descriptor(&instance)?;
    assert_eq!(descriptor.id.as_str(), "openbao-test");
    assert_eq!(descriptor.operations.len(), 5);
    Ok(())
}

#[tokio::test]
async fn authenticated_provider_lifecycle_covers_remote_states_and_cas_results()
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
    let created = store
        .create(
            &scope,
            &reference,
            SecretMaterial::new("created".to_owned())?,
            1,
            None,
        )
        .await?;
    assert!(created.changed);

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

struct RecordingTransport {
    responses: Mutex<VecDeque<OpenBaoHttpResponse>>,
    sent: Mutex<Vec<(Method, String, Option<serde_json::Value>)>>,
}

#[async_trait]
impl OpenBaoHttpTransport for RecordingTransport {
    async fn request(
        &self,
        _connection: &OpenBaoConnection,
        method: Method,
        path: &str,
        _token: Option<&str>,
        body: Option<&serde_json::Value>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        let poisoned = || MmError::State {
            message: "recording transport lock poisoned".to_owned(),
        };
        self.sent
            .lock()
            .map_err(|_| poisoned())?
            .push((method, path.to_owned(), body.cloned()));
        self.responses
            .lock()
            .map_err(|_| poisoned())?
            .pop_front()
            .ok_or_else(|| MmError::State {
                message: "OpenBao response queue exhausted".to_owned(),
            })
    }

    async fn request_bytes(
        &self,
        _connection: &OpenBaoConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<OpenBaoHttpResponse, MmError> {
        Err(MmError::State {
            message: "byte requests are unused by provider tests".to_owned(),
        })
    }
}

fn authored_metadata(recorded: u64) -> OpenBaoHttpResponse {
    response(
        200,
        serde_json::json!({
            "data": {
                "current_version": 2,
                "versions": {"2": {
                    "created_time": "2026-08-26T12:00:00Z",
                    "deletion_time": "", "destroyed": false,
                }},
                "custom_metadata": {"mm_set_by": "alex", "mm_set_version": recorded.to_string()}
            }
        }),
    )
}

#[tokio::test]
async fn a_write_records_its_writer_for_that_version_only() -> Result<(), Box<dyn std::error::Error>>
{
    let transport = Arc::new(RecordingTransport {
        responses: Mutex::new(VecDeque::from([
            health(false),
            login(),
            response(200, serde_json::json!({"data": {"version": 4}})),
            response(204, serde_json::json!({})),
        ])),
        sent: Mutex::new(Vec::new()),
    });
    let (_temporary, store, scope, reference) = store_over(transport.clone())?;
    store
        .replace(
            &scope,
            &reference,
            SecretMaterial::new("replacement-value".to_owned())?,
            SecretExpectedState {
                active_revision: Some("3".to_owned()),
                candidate_revision: None,
            },
            2,
            Some("alex"),
        )
        .await?;
    let (method, path, body) = transport
        .sent
        .lock()
        .map_err(|_| "poisoned")?
        .last()
        .cloned()
        .ok_or("no request")?;
    assert_eq!(method, Method::POST);
    assert!(path.contains("/metadata/mm/"), "{path}");
    let body = body.as_ref().ok_or("no body")?;
    assert_eq!(
        body,
        &serde_json::json!({"custom_metadata": {"mm_set_by": "alex", "mm_set_version": "4"}})
    );
    assert!(!body.to_string().contains("replacement-value"));

    let (_temporary, store, scope, reference) = configured_store(vec![
        health(false),
        login(),
        response(200, serde_json::json!({"data": {"version": 5}})),
        response(403, serde_json::json!({"errors": ["permission denied"]})),
    ])?;
    let written = store
        .create(
            &scope,
            &reference,
            SecretMaterial::new("created".to_owned())?,
            1,
            Some("alex"),
        )
        .await?;
    assert_eq!(
        written.revision, "5",
        "a refused metadata write keeps the version"
    );

    for (recorded, expected) in [(2, Some("alex")), (1, None)] {
        let (_temporary, store, scope, reference) =
            configured_store(vec![health(false), login(), authored_metadata(recorded)])?;
        assert_eq!(
            store.inspect(&scope, &reference).await?.set_by.as_deref(),
            expected
        );
    }
    let (_temporary, store, scope, reference) =
        configured_store(vec![health(false), login(), stored_metadata(200, false)])?;
    assert_eq!(store.inspect(&scope, &reference).await?.set_by, None);
    Ok(())
}
