#![cfg(unix)]

use std::collections::BTreeMap;
use std::sync::Arc;

use mm::adapters::clock::FixedClock;
use mm::adapters::dashboard::DashboardEventHub;
use mm::adapters::release::ReqwestReleaseManifestClient;
use mm::adapters::remote::SshRemoteTransport;
use mm::adapters::secrets::{
    OpenBaoConnection, OpenBaoMode, ReqwestOpenBaoTransport, ReqwestVaultTransport,
    VaultConnection, VaultMode, ca_sha256, openbao_provider_descriptor, vault_ca_sha256,
    vault_provider_descriptor,
};
use mm::app::ports::{DashboardSubscription, ReleaseManifestClient};
use mm::domain::dashboard::DashboardEvent;
use mm::domain::runtime::RuntimeKind;
use mm::domain::secret_backend::{ProviderMode, ProviderOperation};
use mm::domain::target::{
    NamedTarget, TargetBootstrap, TargetDefinition, TargetId, TargetTransport,
};
use mm::domain::workspace::ProjectId;
use reqwest::Method;
use uuid::Uuid;

#[tokio::test]
async fn dashboard_events_are_monotonic_and_periodic() {
    let hub = DashboardEventHub::start(Arc::new(FixedClock { unix_ms: 42 }));
    let mut receiver = hub.subscribe();
    assert!(matches!(
        hub.connected_event(),
        DashboardEvent::Connected { sequence: 1, .. }
    ));
    tokio::task::yield_now().await;
    assert_eq!(
        receiver.recv().await,
        Ok(DashboardEvent::Refresh {
            sequence: 2,
            occurred_at_unix_ms: 42,
        })
    );
    assert!(matches!(
        hub.connected_event(),
        DashboardEvent::Connected { sequence: 3, .. }
    ));
}

#[tokio::test]
async fn release_client_rejects_non_https_and_malformed_urls_without_disclosure()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(ReqwestReleaseManifestClient::stable().is_ok());
    for url in ["http://127.0.0.1/manifest.json", "not a URL"] {
        let client = ReqwestReleaseManifestClient::new(url).map_err(std::io::Error::other)?;
        assert!(format!("{client:?}").contains("ReqwestReleaseManifestClient"));
        assert!(client.fetch_stable().await.is_err());
    }
    Ok(())
}

#[tokio::test]
async fn provider_http_clients_reject_pin_and_pem_tampering_before_network_io() {
    use mm::adapters::secrets::{OpenBaoHttpTransport as _, VaultHttpTransport as _};

    let mut bao = openbao_connection("not a certificate");
    bao.ca_sha256 = "0".repeat(64);
    assert!(
        ReqwestOpenBaoTransport
            .request(&bao, Method::GET, "sys/health", None, None)
            .await
            .is_err()
    );
    bao.ca_sha256 = ca_sha256(&bao.ca_pem);
    assert!(
        ReqwestOpenBaoTransport
            .request_bytes(&bao, Method::POST, "sys/restore", "secret", Some(b"blob"))
            .await
            .is_err()
    );

    let mut vault = vault_connection("not a certificate");
    vault.ca_sha256 = "0".repeat(64);
    assert!(
        ReqwestVaultTransport
            .request(&vault, Method::GET, "sys/health", None, None)
            .await
            .is_err()
    );
    vault.ca_sha256 = vault_ca_sha256(&vault.ca_pem);
    assert!(
        ReqwestVaultTransport
            .request_bytes(&vault, Method::POST, "sys/restore", "secret", Some(b"blob"),)
            .await
            .is_err()
    );
}

#[test]
fn provider_descriptors_pin_modes_releases_and_operations() -> Result<(), Box<dyn std::error::Error>>
{
    let bao = openbao_provider_descriptor()?;
    assert_eq!(bao.driver.as_str(), "openbao");
    assert!(
        bao.require(ProviderMode::Managed, ProviderOperation::Restore)
            .is_ok()
    );
    assert!(
        bao.require(ProviderMode::External, ProviderOperation::Remove)
            .is_err()
    );
    let bao_release = bao
        .mode(ProviderMode::Managed)
        .and_then(|mode| {
            mode.configuration
                .iter()
                .find(|field| field.key == "release")
        })
        .ok_or("OpenBao release field missing")?;
    assert_eq!(bao_release.allowed_values, ["2.3", "2.4", "2.5"]);

    let vault = vault_provider_descriptor()?;
    assert_eq!(vault.driver.as_str(), "vault");
    assert!(
        vault
            .require(ProviderMode::Managed, ProviderOperation::Backup)
            .is_ok()
    );
    assert!(
        vault
            .require(ProviderMode::External, ProviderOperation::Install)
            .is_err()
    );
    let vault_release = vault
        .mode(ProviderMode::Managed)
        .and_then(|mode| {
            mode.configuration
                .iter()
                .find(|field| field.key == "release")
        })
        .ok_or("Vault release field missing")?;
    assert_eq!(vault_release.allowed_values, ["1.18", "1.19"]);
    Ok(())
}

#[test]
fn ssh_transport_requires_valid_enrollment_and_rejects_local_targets() {
    let local = NamedTarget::new(
        "local".to_owned(),
        TargetDefinition::local(TargetId(Uuid::nil())),
    );
    assert!(SshRemoteTransport::new(&local).is_err());

    let unenrolled = NamedTarget::new(
        "remote".to_owned(),
        TargetDefinition {
            id: TargetId(Uuid::from_u128(1)),
            transport: TargetTransport::Ssh {
                host: "example.test".to_owned(),
                port: 22,
                user: "relay".to_owned(),
                host_key_fingerprint: "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
                    .to_owned(),
                host_key: None,
                identity_file: None,
                remote_root: Some("/srv/project".into()),
                daemon_fingerprint: None,
            },
            bootstrap: TargetBootstrap::Ansible,
            ingress: None,
            default_runtime: Some(RuntimeKind::DockerCompose),
            available_runtimes: vec![RuntimeKind::DockerCompose],
            runtime: None,
            nodes: Vec::new(),
        },
    );
    assert!(SshRemoteTransport::new(&unenrolled).is_err());
}

fn openbao_connection(ca_pem: &str) -> OpenBaoConnection {
    OpenBaoConnection {
        mode: OpenBaoMode::External,
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

fn vault_connection(ca_pem: &str) -> VaultConnection {
    VaultConnection {
        mode: VaultMode::External,
        address: "https://127.0.0.1:1/base/ignored".to_owned(),
        namespace: Some("private".to_owned()),
        mount: "kv".to_owned(),
        auth_mount: "approle".to_owned(),
        ca_pem: ca_pem.to_owned(),
        ca_sha256: vault_ca_sha256(ca_pem),
        project_id: ProjectId(Uuid::nil()),
        credentials: BTreeMap::new(),
        allowed_locations: BTreeMap::new(),
        managed_installation: None,
    }
}
