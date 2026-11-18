use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::domain::gate::{GateDriverId, PASSWORD_CAP_DRIVER};

fn material(value: &str) -> Result<SecretMaterial, String> {
    SecretMaterial::new(value.to_owned())
}

#[test]
fn argon2id_parameters_pepper_and_upgrade_are_enforced() -> Result<(), Box<dyn std::error::Error>> {
    let verifier = Argon2idPasswordVerifier;
    let password = material("correct horse")?;
    let record = verifier.hash(&password, &material("pepper-one")?)?;
    assert!(
        record
            .encoded
            .starts_with("$argon2id$v=19$m=19456,t=2,p=1$")
    );
    assert!(verifier.verify(
        &material("correct horse")?,
        &material("pepper-one")?,
        &record
    )?);
    assert!(!verifier.verify(
        &material("correct horse")?,
        &material("pepper-two")?,
        &record
    )?);
    assert!(!verifier.requires_upgrade(&record));
    let mut old = record;
    old.parameter_version = 0;
    assert!(verifier.requires_upgrade(&old));
    Ok(())
}

#[test]
fn sessions_are_authenticated_versioned_and_randomized() -> Result<(), Box<dyn std::error::Error>> {
    let cipher = XChaChaSessionCipher;
    let key = material("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")?;
    let claims = GateSessionClaims {
        session_id: uuid::Uuid::new_v4(),
        policy_id: uuid::Uuid::new_v4(),
        user_id: uuid::Uuid::new_v4(),
        password_version: 1,
        authenticated_drivers: vec![GateDriverId::new(PASSWORD_CAP_DRIVER)?],
        issued_at_unix_ms: 10,
        expires_at_unix_ms: 20,
        csrf_binding: "binding".to_owned(),
    };
    let first = cipher.seal(&claims, 1, &key)?;
    let second = cipher.seal(&claims, 1, &key)?;
    assert_ne!(first, second);
    assert_eq!(cipher.open(&first, 1, &key)?, claims);
    assert!(cipher.open(&first, 2, &key).is_err());
    Ok(())
}

#[test]
fn password_and_session_decoders_reject_malformed_unversioned_and_tampered_values()
-> Result<(), Box<dyn std::error::Error>> {
    let verifier = Argon2idPasswordVerifier;
    let invalid = PasswordHashRecord {
        encoded: "not-a-hash".to_owned(),
        parameter_version: ARGON2_VERSION,
    };
    assert!(
        verifier
            .verify(&material("password")?, &material("pepper")?, &invalid)
            .is_err()
    );
    assert!(verifier.requires_upgrade(&invalid));

    let cipher = XChaChaSessionCipher;
    let short_key = material("c2hvcnQ")?;
    let valid_key = material("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")?;
    assert!(super::cipher(&material("not-base64!")?).is_err());
    assert!(super::cipher(&short_key).is_err());
    for value in [
        "",
        "v2.1.aaa.bbb",
        "v1.1.only-three",
        "v1.1.@@@.bbb",
        "v1.1.YQ.bbb",
        "v1.1.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.invalid",
        "v1.1.a.b.extra",
    ] {
        assert!(cipher.open(value, 1, &valid_key).is_err(), "{value}");
    }
    assert!(format!("{}", cap_not_prepared()).contains("Cap"));
    assert!(format!("{}", cap_admin_unavailable()).contains("Cap"));
    assert!(format!("{}", unavailable()).contains("Cap"));
    assert!(safe_site_key("site-key_1"));
    assert!(!safe_site_key("../site"));
    Ok(())
}

#[tokio::test]
async fn cap_token_verifier_handles_success_false_malformed_status_and_transport_failures()
-> Result<(), Box<dyn std::error::Error>> {
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{Json, Router};

    let app = Router::new()
        .route(
            "/accepted/siteverify",
            post(|| async { Json(serde_json::json!({"success": true})) }),
        )
        .route(
            "/rejected/siteverify",
            post(|| async { Json(serde_json::json!({"success": false})) }),
        )
        .route("/malformed/siteverify", post(|| async { "not-json" }))
        .route(
            "/failed/siteverify",
            post(|| async { StatusCode::BAD_GATEWAY }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let verifier =
        ReqwestCapTokenVerifier::new(format!("http://{address}"))?.with_host_header("cap.mm.test");
    let secret = material("secret")?;
    let token = material("token")?;
    assert!(verifier.verify("accepted", &secret, &token).await?);
    assert!(!verifier.verify("rejected", &secret, &token).await?);
    assert!(verifier.verify("malformed", &secret, &token).await.is_err());
    assert!(verifier.verify("failed", &secret, &token).await.is_err());
    assert!(verifier.verify("../unsafe", &secret, &token).await.is_err());
    assert!(ReqwestCapTokenVerifier::new("http://0.0.0.0:3000").is_err());
    assert!(ReqwestCapTokenVerifier::new("http://cap:3000").is_ok());
    assert!(ReqwestCapTokenVerifier::new("https://cap.example.test").is_ok());
    server.abort();

    let unavailable = ReqwestCapTokenVerifier::new("http://127.0.0.1:1")?;
    assert!(
        unavailable
            .verify("accepted", &secret, &token)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn cap_sites_are_reconciled_through_loopback_without_exposing_the_admin_key()
-> Result<(), Box<dyn std::error::Error>> {
    use axum::routing::{delete, get, post};
    use axum::{Json, Router};

    let deletions = Arc::new(AtomicUsize::new(0));
    let delete_count = Arc::clone(&deletions);
    let app = Router::new()
        .route(
            "/auth/login",
            post(|| async {
                Json(serde_json::json!({
                    "success": true,
                    "session_token": "session-token",
                    "hashed_token": "session-hash"
                }))
            }),
        )
        .route(
            "/server/keys",
            get(|| async {
                Json(serde_json::json!([{
                    "siteKey": "old-site",
                    "name": "mm-owned-site"
                }]))
            })
            .post(|| async {
                Json(serde_json::json!({
                    "siteKey": "new-site",
                    "secretKey": "sk-new-secret"
                }))
            }),
        )
        .route(
            "/server/keys/{site_key}",
            delete(move || {
                let delete_count = Arc::clone(&delete_count);
                async move {
                    delete_count.fetch_add(1, Ordering::SeqCst);
                    Json(serde_json::json!({ "success": true }))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });

    let directory = crate::test_support::tempdir()?;
    let key_file = directory.path().join("cap-admin.key");
    std::fs::write(&key_file, "a".repeat(64))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600))?;
    }
    let provisioner =
        ReqwestCapSiteProvisioner::new(format!("http://{address}"), "cap.mm.test", key_file)?;
    let site = provisioner.create("mm-owned-site").await?;
    assert_eq!(site.site_key, "new-site");
    assert!(site.site_secret.expose(|value| value == "sk-new-secret"));
    provisioner.remove(&site.site_key).await?;
    assert_eq!(deletions.load(Ordering::SeqCst), 2);
    assert!(
        ReqwestCapSiteProvisioner::new(
            "http://0.0.0.0:3000",
            "cap.mm.test",
            directory.path().join("missing")
        )
        .is_err()
    );
    server.abort();
    Ok(())
}

#[tokio::test]
async fn cap_site_validation_and_failed_admin_responses_remain_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    use axum::http::StatusCode;
    use axum::routing::{delete, get, post};
    use axum::{Json, Router};

    let directory = crate::test_support::tempdir()?;
    let invalid_key = directory.path().join("invalid.key");
    std::fs::write(&invalid_key, "a".repeat(63))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&invalid_key, std::fs::Permissions::from_mode(0o600))?;
    }
    assert!(
        ReqwestCapSiteProvisioner::new("http://127.0.0.1:3000", "", invalid_key.clone()).is_err()
    );
    let invalid_admin =
        ReqwestCapSiteProvisioner::new("http://127.0.0.1:3000", "cap.mm.test", invalid_key)?;
    assert!(format!("{invalid_admin:?}").contains("ReqwestCapSiteProvisioner"));
    assert!(invalid_admin.create("site").await.is_err());

    let app = Router::new()
        .route(
            "/auth/login",
            post(|| async {
                Json(serde_json::json!({
                    "success": true,
                    "session_token": "session-token",
                    "hashed_token": "session-hash"
                }))
            }),
        )
        .route("/server/keys", get(|| async { StatusCode::BAD_GATEWAY }))
        .route(
            "/server/keys/{site_key}",
            delete(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let key_file = directory.path().join("valid.key");
    std::fs::write(&key_file, "b".repeat(64))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600))?;
    }
    let provisioner =
        ReqwestCapSiteProvisioner::new(format!("http://{address}"), "cap.mm.test", key_file)?;
    assert!(provisioner.remove("../unsafe").await.is_err());
    assert!(provisioner.remove("safe-site").await.is_err());
    assert!(provisioner.create("site").await.is_err());
    server.abort();
    Ok(())
}
