use std::sync::Mutex;

use async_trait::async_trait;

use super::*;

#[derive(Default)]
struct AdminTransport {
    requests: Mutex<Vec<String>>,
}

fn response(status: u16, body: serde_json::Value) -> Result<VaultHttpResponse, MmError> {
    Ok(VaultHttpResponse {
        status,
        headers: BTreeMap::new(),
        body: Zeroizing::new(serde_json::to_vec(&body).map_err(|error| MmError::State {
            message: error.to_string(),
        })?),
    })
}

#[async_trait]
impl VaultHttpTransport for AdminTransport {
    async fn request(
        &self,
        _connection: &VaultConnection,
        method: Method,
        path: &str,
        token: Option<&str>,
        _body: Option<&serde_json::Value>,
    ) -> Result<VaultHttpResponse, MmError> {
        self.requests
            .lock()
            .map_err(|_| MmError::State {
                message: "Vault admin request lock was poisoned".to_owned(),
            })?
            .push(format!(
                "{} {path} token={}",
                method.as_str(),
                token.is_some()
            ));
        if path == "sys/health" {
            return response(
                200,
                serde_json::json!({
                    "initialized": true,
                    "sealed": false,
                    "standby": false,
                    "version": "1.19.5"
                }),
            );
        }
        if path == "sys/mounts" || path == "sys/auth" {
            return response(200, serde_json::json!({"data": {}}));
        }
        if path.ends_with("/role-id") {
            return response(200, serde_json::json!({"data": {"role_id": "role-id"}}));
        }
        if path.ends_with("/secret-id") {
            return response(200, serde_json::json!({"data": {"secret_id": "secret-id"}}));
        }
        if path.ends_with("/login") {
            return response(
                200,
                serde_json::json!({
                    "auth": {
                        "client_token": "ephemeral",
                        "renewable": true,
                        "lease_duration": 900
                    }
                }),
            );
        }
        if path.contains("/metadata/") {
            return response(404, serde_json::json!({"errors": []}));
        }
        response(204, serde_json::json!({}))
    }

    async fn request_bytes(
        &self,
        _connection: &VaultConnection,
        _method: Method,
        _path: &str,
        _token: &str,
        _body: Option<&[u8]>,
    ) -> Result<VaultHttpResponse, MmError> {
        response(204, serde_json::json!({}))
    }
}

fn setup_request(project_id: ProjectId, environment: &str) -> VaultSetupRequest {
    let ca_pem = "test-ca".to_owned();
    VaultSetupRequest {
        name: "external".to_owned(),
        mode: VaultMode::External,
        address: "https://vault.example.test".to_owned(),
        namespace: Some("private".to_owned()),
        mount: "kv".to_owned(),
        auth_mount: "approle".to_owned(),
        expected_ca_sha256: ca_sha256(&ca_pem),
        ca_pem,
        project_id,
        environment: environment.to_owned(),
        locations: vec![
            "zeta/key".to_owned(),
            "app/key".to_owned(),
            "app/key".to_owned(),
        ],
        developer_token: Zeroizing::new("developer-token".to_owned()),
        allow_mount_bootstrap: true,
        managed_installation: None,
    }
}

#[tokio::test]
async fn setup_bootstraps_scoped_access_persists_and_rejects_rebinding()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let connections = Arc::new(VaultConnectionStore::new(
        temp.path().join("identity.key"),
        temp.path().join("connections.age"),
    ));
    let transport = Arc::new(AdminTransport::default());
    let administrator = VaultConnectionAdministrator::new(connections.clone(), transport.clone());
    let project_id = ProjectId(uuid::Uuid::from_u128(61));
    assert!(format!("{administrator:?}").contains("VaultConnectionAdministrator"));

    let debug = format!("{:?}", setup_request(project_id, "development"));
    assert!(!debug.contains("developer-token"));
    assert!(!debug.contains("vault.example.test"));
    assert!(!debug.contains("test-ca"));

    let status = administrator
        .setup(setup_request(project_id, "development"))
        .await?;
    assert!(status.health.healthy);
    assert!(status.authenticated);
    assert!(status.isolated_path.contains("development"));

    administrator
        .setup(setup_request(project_id, "staging"))
        .await?;
    let stored = connections.connection("external")?;
    assert_eq!(stored.credentials.len(), 2);
    assert_eq!(
        stored.allowed_locations["development"],
        ["app/key", "zeta/key"]
    );

    let mut rebound = setup_request(project_id, "production");
    rebound.address = "https://other.example.test".to_owned();
    assert!(administrator.setup(rebound).await.is_err());

    let requests = transport.requests.lock().map_err(|_| "request lock")?;
    assert!(
        requests
            .iter()
            .any(|request| request.contains("sys/mounts/kv"))
    );
    assert!(
        requests
            .iter()
            .any(|request| request.contains("sys/auth/approle"))
    );
    assert!(
        requests
            .iter()
            .all(|request| !request.contains("developer-token"))
    );
    Ok(())
}

#[tokio::test]
async fn setup_validation_and_admin_helpers_fail_at_exact_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let administrator = VaultConnectionAdministrator::new(
        Arc::new(VaultConnectionStore::new(
            temp.path().join("identity.key"),
            temp.path().join("connections.age"),
        )),
        Arc::new(AdminTransport::default()),
    );
    let project_id = ProjectId(uuid::Uuid::from_u128(62));
    let mut empty = setup_request(project_id, "development");
    empty.developer_token = Zeroizing::new(String::new());
    assert!(administrator.setup(empty).await.is_err());
    let mut wrong_pin = setup_request(project_id, "development");
    wrong_pin.expected_ca_sha256 = "0".repeat(64);
    assert!(administrator.setup(wrong_pin).await.is_err());
    let mut invalid = setup_request(project_id, "bad/environment");
    invalid.locations.clear();
    assert!(administrator.setup(invalid).await.is_err());

    assert_eq!(
        normalized_locations(vec!["b".to_owned(), "a".to_owned(), "a".to_owned()])?,
        ["a", "b"]
    );
    assert!(normalized_locations(Vec::new()).is_err());
    assert!(normalized_locations(vec!["../escape".to_owned()]).is_err());

    let connection = VaultConnection {
        mode: VaultMode::External,
        address: "https://example.test".to_owned(),
        namespace: None,
        mount: "kv".to_owned(),
        auth_mount: "approle".to_owned(),
        ca_pem: "ca".to_owned(),
        ca_sha256: ca_sha256("ca"),
        project_id,
        credentials: BTreeMap::new(),
        allowed_locations: BTreeMap::new(),
        managed_installation: None,
    };
    let policy = environment_policy(
        &connection,
        project_id,
        "development",
        &["app/key".to_owned()],
    );
    assert!(policy.contains("/data/mm/"));
    assert!(policy.contains("/metadata/mm/"));
    assert!(policy.contains("/delete/mm/"));
    assert!(ensure_same_connection(&connection, &connection).is_ok());
    let mut changed = connection.clone();
    changed.mount = "other".to_owned();
    assert!(ensure_same_connection(&connection, &changed).is_err());

    assert!(require_success(&response(204, serde_json::json!({}))?, "test").is_ok());
    assert!(matches!(
        require_success(&response(403, serde_json::json!({}))?, "test"),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        require_success(&response(500, serde_json::json!({}))?, "test"),
        Err(MmError::Secret { .. })
    ));
    let malformed = VaultHttpResponse {
        status: 200,
        headers: BTreeMap::new(),
        body: Zeroizing::new(b"not-json".to_vec()),
    };
    assert!(parse::<serde_json::Value>(&malformed, "test response").is_err());
    Ok(())
}
