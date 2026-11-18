use std::collections::BTreeMap;

use reqwest::Method;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::domain::workspace::ProjectId;

use super::super::connection_store::{VaultConnection, VaultMode, ca_sha256};
use super::{ReqwestVaultTransport, VaultHttpResponse, VaultHttpTransport as _};

fn valid_ca() -> Result<String, Box<dyn std::error::Error>> {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    let mut parameters = rcgen::CertificateParams::new(Vec::<String>::new())?;
    parameters.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    Ok(parameters.self_signed(&key)?.pem())
}

fn connection(ca_pem: &str) -> VaultConnection {
    VaultConnection {
        mode: VaultMode::External,
        address: "https://127.0.0.1:1/base/ignored".to_owned(),
        namespace: Some("private".to_owned()),
        mount: "kv".to_owned(),
        auth_mount: "approle".to_owned(),
        ca_pem: ca_pem.to_owned(),
        ca_sha256: ca_sha256(ca_pem),
        project_id: ProjectId(Uuid::nil()),
        credentials: BTreeMap::new(),
        allowed_locations: BTreeMap::new(),
        managed_installation: None,
    }
}

#[tokio::test]
async fn transport_rejects_tampered_and_invalid_trust_before_network_io() {
    let transport = ReqwestVaultTransport;
    let mut tampered = connection("not a certificate");
    tampered.ca_sha256 = "0".repeat(64);
    assert!(
        transport
            .request(&tampered, Method::GET, "sys/health", None, None)
            .await
            .is_err()
    );

    let invalid = connection("not a certificate");
    assert!(
        transport
            .request(&invalid, Method::GET, "sys/health", None, None)
            .await
            .is_err()
    );
    assert!(
        transport
            .request_bytes(
                &invalid,
                Method::POST,
                "sys/restore",
                "secret",
                Some(b"blob"),
            )
            .await
            .is_err()
    );
}

#[test]
fn urls_are_rooted_and_response_debug_redacts_body() -> Result<(), Box<dyn std::error::Error>> {
    let configured = connection("invalid");
    let url = ReqwestVaultTransport::url(&configured, "/secret/data/example")?;
    assert_eq!(url.as_str(), "https://127.0.0.1:1/v1/secret/data/example");

    let response = VaultHttpResponse {
        status: 200,
        headers: BTreeMap::from([("x-vault-index".to_owned(), "1".to_owned())]),
        body: Zeroizing::new(b"must-not-leak".to_vec()),
    };
    let debug = format!("{response:?}");
    assert!(debug.contains("x-vault-index"));
    assert!(!debug.contains("must-not-leak"));

    let mut malformed = connection("invalid");
    malformed.address = "not a URL".to_owned();
    assert!(ReqwestVaultTransport::url(&malformed, "sys/health").is_err());
    Ok(())
}

#[tokio::test]
async fn valid_tls_configuration_builds_every_request_body_before_connect_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let transport = ReqwestVaultTransport;
    let configured = connection(&valid_ca()?);
    assert!(
        transport
            .request(&configured, Method::GET, "sys/health", None, None)
            .await
            .is_err()
    );
    assert!(
        transport
            .request(
                &configured,
                Method::POST,
                "secret/data/example",
                Some("token"),
                Some(&serde_json::json!({"value": "secret"})),
            )
            .await
            .is_err()
    );
    assert!(
        transport
            .request_bytes(
                &configured,
                Method::POST,
                "sys/storage/raft/snapshot-force",
                "token",
                Some(b"snapshot"),
            )
            .await
            .is_err()
    );
    Ok(())
}
