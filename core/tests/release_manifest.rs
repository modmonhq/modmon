//! The release manifest as an older `mm` reads a newer release's: what a
//! release adds is skipped, and what this `mm` downloads is still checked.

use mm::domain::release::{
    INSTALLER_COMMAND, ReleaseManifest, UpdateStatus, compare_installed, installed_target,
};
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const MANIFEST: &str = include_str!("fixtures/release-manifest.json");

fn fixture() -> Result<Value, serde_json::Error> {
    serde_json::from_str(MANIFEST)
}

fn later_release() -> Result<Value, Box<dyn std::error::Error>> {
    let mut manifest = fixture()?;
    manifest["schema_version"] = json!(3);
    manifest["signatures"] =
        json!({"x86_64-unknown-linux-gnu": "a signature this mm does not read"});
    manifest["supported_targets"] =
        json!(["x86_64-unknown-linux-gnu", "riscv64gc-unknown-linux-gnu"]);
    manifest["artifacts"]["riscv64gc-unknown-linux-gnu"] = json!({
        "files": ["mm-v0.1.0-riscv64gc-unknown-linux-gnu.tar.zst"],
        "format": "tar.zst",
    });
    manifest["artifacts"]["x86_64-pc-windows-msvc"] = json!({
        "path": "releases/v0.1.0/mm-v0.1.0-x86_64-pc-windows-msvc.zip",
        "format": "zip",
        "sha256": "0".repeat(64),
        "size": 1,
        "binaries": ["mm.exe"],
    });
    for target in [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
    ] {
        let artifact = &mut manifest["artifacts"][target];
        artifact["signature"] = json!("detached");
        if let Some(binaries) = artifact["binaries"].as_array_mut() {
            binaries.push(json!("mm-future-tool"));
        }
    }
    manifest
        .as_object_mut()
        .ok_or("the fixture is not an object")?
        .remove("sbom");
    Ok(manifest)
}

#[test]
fn a_later_release_with_more_fields_targets_and_binaries_is_read() -> TestResult {
    let manifest = ReleaseManifest::parse(later_release()?.to_string().as_bytes())?;
    assert_eq!(manifest.version, "0.1.0");
    assert_eq!(manifest.schema_version, 3);
    assert_eq!(
        manifest.artifacts.keys().collect::<Vec<_>>(),
        [
            "aarch64-apple-darwin",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-gnu",
        ]
    );
    let linux = manifest
        .artifact("x86_64-unknown-linux-gnu")?
        .ok_or("the Linux archive is missing")?;
    assert_eq!(linux.known_binaries(), ["mm", "mm-ingress-helper", "mmd"]);
    assert_eq!(
        linux.file_name(),
        "mm-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    );
    if let Some(target) = installed_target() {
        let report = compare_installed("0.0.9", Some(target), &manifest)?;
        assert_eq!(
            report.status,
            UpdateStatus::UpdateAvailable,
            "{}",
            report.message
        );
    }
    Ok(())
}

#[test]
fn a_release_that_needs_a_newer_reader_says_to_run_the_installer() -> TestResult {
    let mut manifest = later_release()?;
    manifest["minimum_installer_schema"] = json!(2);
    manifest["artifacts"] = json!([{"a": "shape this mm has never seen"}]);
    let refused = ReleaseManifest::parse(manifest.to_string().as_bytes())
        .err()
        .ok_or("a manifest for newer readers was accepted")?;
    assert!(refused.contains("needs a newer mm"), "{refused}");
    assert!(refused.contains("v0.1.0"), "{refused}");
    assert!(refused.contains(INSTALLER_COMMAND), "{refused}");
    Ok(())
}

#[test]
fn what_this_mm_downloads_is_still_checked() -> TestResult {
    for (field, value) in [
        ("path", json!("releases/v0.1.0/../mm.tar.gz")),
        (
            "path",
            json!("releases/v0.0.9/mm-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"),
        ),
        ("sha256", json!("not-a-digest")),
        ("size", json!(0)),
        ("format", json!("tar.zst")),
        ("binaries", json!(["mmd"])),
    ] {
        let mut manifest = later_release()?;
        manifest["artifacts"]["x86_64-unknown-linux-gnu"][field] = value.clone();
        let manifest = ReleaseManifest::parse(manifest.to_string().as_bytes())?;
        assert!(
            manifest.artifact("x86_64-unknown-linux-gnu").is_err(),
            "{field} = {value} was accepted"
        );
        assert!(manifest.artifact("aarch64-unknown-linux-gnu")?.is_some());
    }
    for (field, value) in [
        ("tag", json!("v0.1.1")),
        ("version", json!("0.1.0-rc.1")),
        ("product", json!("other")),
        ("channel", json!("nightly")),
    ] {
        let mut manifest = later_release()?;
        manifest[field] = value.clone();
        assert!(
            ReleaseManifest::parse(manifest.to_string().as_bytes()).is_err(),
            "{field} = {value} was accepted"
        );
    }
    Ok(())
}

#[test]
fn a_target_listed_twice_is_refused_and_a_target_named_elsewhere_is_not() -> TestResult {
    let encoded = later_release()?.to_string();
    let duplicate = encoded.replacen(
        "\"artifacts\":{",
        "\"artifacts\":{\"x86_64-unknown-linux-gnu\":{\"path\":\"releases/v0.1.0/x.tar.gz\",\"format\":\"tar.gz\",\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"size\":1,\"binaries\":[\"mm\"]},",
        1,
    );
    assert_ne!(duplicate, encoded);
    let refused = ReleaseManifest::parse(duplicate.as_bytes())
        .err()
        .ok_or("a duplicate target was accepted")?;
    assert!(refused.contains("duplicate release target"), "{refused}");
    assert!(encoded.matches("\"x86_64-unknown-linux-gnu\"").count() > 1);
    ReleaseManifest::parse(encoded.as_bytes())?;
    Ok(())
}
