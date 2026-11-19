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
        source: Some(file("releases/v1.2.3/mm-v1.2.3-source.tar.gz")),
        sbom: Some(file("releases/v1.2.3/sbom.spdx.json")),
        release_certificate: Some(file("releases/v1.2.3/release-certificate.json")),
        provenance: vec![file("releases/v1.2.3/provenance/build.intoto.jsonl")],
    }
}

#[test]
fn schema_round_trip_is_deterministic_and_ignores_unknown_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let expected = manifest();
    let first = serde_json::to_vec(&expected)?;
    let parsed = ReleaseManifest::parse(&first).map_err(std::io::Error::other)?;
    let second = serde_json::to_vec(&parsed)?;
    assert_eq!(first, second);
    let mut unknown = serde_json::to_value(expected.clone())?;
    unknown["unexpected"] = serde_json::json!(true);
    unknown["artifacts"]["x86_64-unknown-linux-gnu"]["signature"] = serde_json::json!("sig");
    let parsed =
        ReleaseManifest::parse(unknown.to_string().as_bytes()).map_err(std::io::Error::other)?;
    assert_eq!(parsed, expected);
    Ok(())
}

#[test]
fn rejects_unsafe_paths_formats_checksums_and_components() -> Result<(), Box<dyn std::error::Error>>
{
    for path in [
        "/releases/v1.2.3/a.tar.gz",
        "releases/v1.2.3/../a.tar.gz",
        "releases/v1.2.3/nested/a.tar.gz",
        "releases/v1.2.4/a.tar.gz",
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
        assert!(
            candidate.artifact("x86_64-unknown-linux-gnu").is_err(),
            "accepted {path}"
        );
    }
    let mut candidate = manifest();
    let artifact = candidate
        .artifacts
        .values_mut()
        .next()
        .ok_or_else(|| std::io::Error::other("test artifact is missing"))?;
    artifact.sha256 = "A".repeat(64);
    assert!(candidate.artifact("x86_64-unknown-linux-gnu").is_err());
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
fn manifest_identity_fails_closed_and_later_schemas_are_read() {
    let mutations: [fn(&mut ReleaseManifest); 7] = [
        |value| value.minimum_installer_schema = 2,
        |value| value.minimum_installer_schema = 0,
        |value| value.schema_version = 0,
        |value| value.product = "other".to_owned(),
        |value| value.channel = "nightly".to_owned(),
        |value| value.version = "1.2.3-alpha.1".to_owned(),
        |value| value.tag = "v1.2.4".to_owned(),
    ];
    for mutate in mutations {
        let mut candidate = manifest();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
    let mut later = manifest();
    later.schema_version = 7;
    later.provenance.clear();
    later.sbom = None;
    assert!(later.validate().is_ok());
    let mut newer_reader = manifest();
    newer_reader.minimum_installer_schema = 2;
    let refused = newer_reader.validate().err().unwrap_or_default();
    assert!(refused.contains("needs a newer mm"), "{refused}");
    assert!(refused.contains(INSTALLER_COMMAND), "{refused}");
}

#[test]
fn the_archive_this_mm_uses_is_checked_and_other_binaries_are_skipped()
-> Result<(), Box<dyn std::error::Error>> {
    let target = "x86_64-unknown-linux-gnu";
    let mut linux = manifest();
    linux
        .artifacts
        .get_mut(target)
        .ok_or("fixture artifact is missing")?
        .binaries = ["mm", "mmd", "mm-ingress-helper", "mm-future-tool"]
        .map(str::to_owned)
        .to_vec();
    let artifact = linux.artifact(target)?.ok_or("artifact is missing")?;
    assert_eq!(
        artifact.known_binaries(),
        ["mm", "mm-ingress-helper", "mmd"]
    );
    assert_eq!(
        artifact.file_name(),
        "mm-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
    );
    assert_eq!(linux.artifact("aarch64-apple-darwin")?, None);

    for format in [ArchiveFormat::Zip, ArchiveFormat::Unknown] {
        let mut other = manifest();
        let artifact = other
            .artifacts
            .get_mut(target)
            .ok_or("fixture artifact is missing")?;
        artifact.format = format;
        let refused = other.artifact(target).err().unwrap_or_default();
        assert!(refused.contains("cannot unpack"), "{refused}");
    }
    for binaries in [
        Vec::new(),
        vec!["mmd".to_owned()],
        vec!["mm".to_owned(), "mm".to_owned()],
    ] {
        let mut candidate = manifest();
        candidate
            .artifacts
            .get_mut(target)
            .ok_or("fixture artifact is missing")?
            .binaries = binaries;
        assert!(candidate.artifact(target).is_err());
    }
    let mut wrong_extension = manifest();
    wrong_extension
        .artifacts
        .get_mut(target)
        .ok_or("fixture artifact is missing")?
        .path = "releases/v1.2.3/mm-v1.2.3-x86_64-unknown-linux-gnu.zip".to_owned();
    assert!(wrong_extension.artifact(target).is_err());
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
