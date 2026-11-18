use super::*;

fn file(path: &str) -> ReleaseFile {
    ReleaseFile {
        path: path.to_owned(),
        sha256: "a".repeat(64),
        size: 1,
    }
}

fn manifest() -> ReleaseManifest {
    ReleaseManifest {
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
            "x86_64-unknown-linux-gnu".to_owned(),
            ReleaseArtifact {
                path: "releases/v1.2.3/mm-v1.2.3-x86_64-unknown-linux-gnu.tar.gz".to_owned(),
                format: ArchiveFormat::TarGz,
                sha256: "b".repeat(64),
                size: 2,
                binaries: vec!["mm".to_owned(), "mmd".to_owned()],
            },
        )]),
        source: file("releases/v1.2.3/mm-v1.2.3-source.tar.gz"),
        sbom: file("releases/v1.2.3/sbom.spdx.json"),
        release_certificate: file("releases/v1.2.3/release-certificate.json"),
        provenance: vec![file("releases/v1.2.3/provenance/build.intoto.jsonl")],
    }
}

#[test]
fn schema_round_trip_is_deterministic_and_strict() -> Result<(), Box<dyn std::error::Error>> {
    let expected = manifest();
    let first = serde_json::to_vec(&expected)?;
    let parsed = ReleaseManifest::parse(&first).map_err(std::io::Error::other)?;
    let second = serde_json::to_vec(&parsed)?;
    assert_eq!(first, second);
    let mut unknown = serde_json::to_value(expected)?;
    unknown["unexpected"] = serde_json::json!(true);
    assert!(ReleaseManifest::parse(unknown.to_string().as_bytes()).is_err());
    Ok(())
}

#[test]
fn rejects_unsafe_paths_formats_checksums_and_components() -> Result<(), Box<dyn std::error::Error>>
{
    for path in [
        "/releases/v1.2.3/a.tar.gz",
        "releases/v1.2.3/../a.tar.gz",
        "https://example.invalid/a.tar.gz",
        "releases/v1.2.3/a\\b.tar.gz",
    ] {
        let mut candidate = manifest();
        let artifact = candidate
            .artifacts
            .values_mut()
            .next()
            .ok_or_else(|| std::io::Error::other("test artifact is missing"))?;
        artifact.path = path.to_owned();
        assert!(candidate.validate().is_err(), "accepted {path}");
    }
    let mut candidate = manifest();
    let artifact = candidate
        .artifacts
        .values_mut()
        .next()
        .ok_or_else(|| std::io::Error::other("test artifact is missing"))?;
    artifact.sha256 = "A".repeat(64);
    artifact.size = 0;
    artifact.binaries = vec!["../mm".to_owned()];
    assert!(candidate.validate().is_err());
    Ok(())
}

#[test]
fn semantic_comparison_never_reports_false_upgrade_or_downgrade()
-> Result<(), Box<dyn std::error::Error>> {
    let manifest = manifest();
    assert_eq!(
        compare_installed("1.2.3", Some("x86_64-unknown-linux-gnu"), &manifest)
            .map_err(std::io::Error::other)?
            .status,
        UpdateStatus::UpToDate
    );
    assert_eq!(
        compare_installed("1.2.2", Some("x86_64-unknown-linux-gnu"), &manifest)
            .map_err(std::io::Error::other)?
            .status,
        UpdateStatus::UpdateAvailable
    );
    assert_eq!(
        compare_installed("2.0.0", Some("x86_64-unknown-linux-gnu"), &manifest)
            .map_err(std::io::Error::other)?
            .status,
        UpdateStatus::InstalledVersionNewer
    );
    assert_eq!(
        compare_installed("1.2.3", Some("unknown-target"), &manifest)
            .map_err(std::io::Error::other)?
            .status,
        UpdateStatus::UnsupportedPlatform
    );
    Ok(())
}

#[test]
fn manifest_identity_and_inventory_fail_closed() {
    let mutations: [fn(&mut ReleaseManifest); 12] = [
        |value| value.schema_version = 2,
        |value| value.minimum_installer_schema = 2,
        |value| value.product = "other".to_owned(),
        |value| value.channel = "nightly".to_owned(),
        |value| value.version = "1.2.3-alpha.1".to_owned(),
        |value| value.tag = "v1.2.4".to_owned(),
        |value| value.commit = "A".repeat(40),
        |value| value.published_at = "not-a-time".to_owned(),
        |value| value.source_date_epoch = 0,
        |value| value.artifacts.clear(),
        |value| value.provenance.clear(),
        |value| {
            if let Some(artifact) = value.artifacts.values().next().cloned() {
                value.artifacts.insert("unknown".to_owned(), artifact);
            }
        },
    ];
    for mutate in mutations {
        let mut candidate = manifest();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
}

#[test]
fn artifact_formats_and_binary_inventories_are_exact() -> Result<(), Box<dyn std::error::Error>> {
    let mut zip = manifest();
    let artifact = zip
        .artifacts
        .values_mut()
        .next()
        .ok_or("fixture artifact is missing")?;
    artifact.format = ArchiveFormat::Zip;
    artifact.path = "releases/v1.2.3/mm-v1.2.3-x86_64-pc-windows-msvc.zip".to_owned();
    artifact.binaries = vec!["mm".to_owned()];
    assert!(zip.validate().is_ok());

    let mut linux = manifest();
    let artifact = linux
        .artifacts
        .values_mut()
        .next()
        .ok_or("fixture artifact is missing")?;
    artifact.binaries = ["mm", "mmd", "mm-ingress-helper"]
        .map(str::to_owned)
        .to_vec();
    assert!(linux.validate().is_ok());
    let mut macos = manifest();
    let mut artifact = macos
        .artifacts
        .remove("x86_64-unknown-linux-gnu")
        .ok_or("fixture artifact is missing")?;
    artifact.path = "releases/v1.2.3/mm-v1.2.3-aarch64-apple-darwin.tar.gz".to_owned();
    macos
        .artifacts
        .insert("aarch64-apple-darwin".to_owned(), artifact.clone());
    assert!(macos.validate().is_ok());
    let mut windows = macos.clone();
    let moved = windows
        .artifacts
        .remove("aarch64-apple-darwin")
        .ok_or("fixture artifact is missing")?;
    windows
        .artifacts
        .insert("x86_64-pc-windows-msvc".to_owned(), moved);
    assert!(
        windows.validate().is_err(),
        "Windows is not a release target"
    );
    artifact.binaries = ["mm", "mm-ingress-helper"].map(str::to_owned).to_vec();
    macos
        .artifacts
        .insert("aarch64-apple-darwin".to_owned(), artifact);
    assert!(
        macos.validate().is_err(),
        "the ingress helper ships with Linux archives only"
    );

    for binaries in [
        Vec::new(),
        vec!["mmd".to_owned()],
        vec!["mm-ingress-helper".to_owned()],
        vec!["mm".to_owned(), "mm".to_owned()],
        vec!["mm".to_owned(), "other".to_owned()],
    ] {
        let mut candidate = manifest();
        let artifact = candidate
            .artifacts
            .values_mut()
            .next()
            .ok_or("fixture artifact is missing")?;
        artifact.binaries = binaries;
        assert!(candidate.validate().is_err());
    }
    let mut wrong_extension = manifest();
    let artifact = wrong_extension
        .artifacts
        .values_mut()
        .next()
        .ok_or("fixture artifact is missing")?;
    artifact.format = ArchiveFormat::Zip;
    assert!(wrong_extension.validate().is_err());
    Ok(())
}

#[test]
fn parsing_rejects_duplicate_targets_and_versions_are_canonical()
-> Result<(), Box<dyn std::error::Error>> {
    let encoded = serde_json::to_string(&manifest())?;
    let fixture = manifest();
    let artifact = fixture
        .artifacts
        .values()
        .next()
        .ok_or("fixture artifact is missing")?;
    let duplicate = encoded.replacen(
        "\"artifacts\":{",
        &format!(
            "\"artifacts\":{{\"x86_64-unknown-linux-gnu\":{},",
            serde_json::to_string(artifact)?
        ),
        1,
    );
    assert!(ReleaseManifest::parse(duplicate.as_bytes()).is_err());
    assert!(ReleaseManifest::parse(b"not-json").is_err());
    for version in ["v1.2.3", "1.2", "1.2.3+build", "01.2.3"] {
        assert!(
            compare_installed(version, None, &manifest()).is_err(),
            "{version}"
        );
    }
    let unsupported =
        compare_installed("1.2.3", None, &manifest()).map_err(std::io::Error::other)?;
    assert_eq!(unsupported.status, UpdateStatus::UnsupportedPlatform);
    assert!(unsupported.message.contains("this platform"));
    assert!(installed_target().is_some());
    Ok(())
}
