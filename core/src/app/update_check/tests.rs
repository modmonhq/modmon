use async_trait::async_trait;
use std::collections::BTreeMap;

use super::*;
use crate::domain::release::{ArchiveFormat, ReleaseArtifact, ReleaseFile, ReleaseManifest};

struct UnavailableClient;

struct StaticClient(ReleaseManifest);

#[async_trait]
impl ReleaseManifestClient for UnavailableClient {
    async fn fetch_stable(&self) -> Result<ReleaseManifest, String> {
        Err("stable manifest fixture is unreachable".to_owned())
    }
}

#[async_trait]
impl ReleaseManifestClient for StaticClient {
    async fn fetch_stable(&self) -> Result<ReleaseManifest, String> {
        Ok(self.0.clone())
    }
}

fn file(path: &str) -> ReleaseFile {
    ReleaseFile {
        path: path.to_owned(),
        sha256: "a".repeat(64),
        size: 1,
    }
}

fn manifest() -> Result<ReleaseManifest, &'static str> {
    let target = installed_target().ok_or("unsupported test platform")?;
    Ok(ReleaseManifest {
        schema_version: 1,
        product: "mm".to_owned(),
        channel: "stable".to_owned(),
        version: "1.2.3".to_owned(),
        tag: "v1.2.3".to_owned(),
        commit: "1".repeat(40),
        published_at: "2026-08-24T00:00:00Z".to_owned(),
        source_date_epoch: 1_787_529_600,
        minimum_installer_schema: 1,
        artifacts: BTreeMap::from([(
            target.to_owned(),
            ReleaseArtifact {
                path: "releases/v1.2.3/mm-v1.2.3.tar.gz".to_owned(),
                format: ArchiveFormat::TarGz,
                sha256: "b".repeat(64),
                size: 2,
                binaries: vec!["mm".to_owned()],
            },
        )]),
        source: Some(file("releases/v1.2.3/source.tar.gz")),
        sbom: Some(file("releases/v1.2.3/sbom.json")),
        release_certificate: Some(file("releases/v1.2.3/certificate.json")),
        provenance: vec![file("releases/v1.2.3/provenance.json")],
    })
}

#[tokio::test]
async fn explicit_service_reports_network_failure_without_downloading_an_artifact() {
    let service = UpdateCheckService::new(Arc::new(UnavailableClient));
    let report = service.check("0.1.0").await;

    assert_eq!(report.status, UpdateStatus::Unavailable);
    assert_eq!(report.current, "0.1.0");
    assert!(report.latest.is_none());
    assert!(report.message.contains("unreachable"));
}

#[tokio::test]
async fn service_reports_comparison_success_and_manifest_validation_failure()
-> Result<(), &'static str> {
    let service = UpdateCheckService::new(Arc::new(StaticClient(manifest()?)));
    let report = service.check("1.2.2").await;
    assert_eq!(report.status, UpdateStatus::UpdateAvailable);
    assert_eq!(report.latest.as_deref(), Some("1.2.3"));
    assert_eq!(report.target.as_deref(), installed_target());

    let mut malformed = manifest()?;
    malformed.version = "not-semver".to_owned();
    let service = UpdateCheckService::new(Arc::new(StaticClient(malformed)));
    let report = service.check("1.2.2").await;
    assert_eq!(report.status, UpdateStatus::Unavailable);
    assert_eq!(report.latest.as_deref(), Some("not-semver"));
    assert!(report.message.contains("semantic version"));
    Ok(())
}
