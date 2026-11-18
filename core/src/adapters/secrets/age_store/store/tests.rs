use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use super::*;

fn store(temp: &TempDir, name: &str) -> AgeSecretStore {
    AgeSecretStore::new(
        temp.path().join("identities/project.agekey"),
        temp.path().join(format!("secrets/{name}.age")),
    )
}

#[test]
fn initialization_reuses_private_identity_rejects_duplicates_and_handles_empty_batches()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let first = store(&temp, "first");
    let initialized = first.initialize()?;
    assert!(first.initialized());
    assert!(initialized.recipient.starts_with("age1"));
    assert!(first.initialize().is_err());

    let second = store(&temp, "second");
    let second_initialized = second.initialize()?;
    assert_eq!(second_initialized.recipient, initialized.recipient);
    assert!(second.read_document()?.entries.is_empty());
    assert!(second.replace_many(Vec::new(), 1)?.is_empty());
    Ok(())
}

#[test]
fn identity_and_document_readers_reject_encoding_key_ciphertext_json_and_version_corruption()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(&temp, "corrupt");
    store.initialize()?;
    let identity = store.read_identity()?;

    fs::write(&store.identity_path, [0xff, 0xfe])?;
    fs::set_permissions(&store.identity_path, fs::Permissions::from_mode(0o600))?;
    assert!(store.read_identity().is_err());
    fs::write(&store.identity_path, b"not-an-age-key")?;
    assert!(store.read_identity().is_err());
    fs::write(&store.identity_path, identity.to_string().expose_secret().as_bytes())?;

    fs::write(&store.store_path, b"not-age-ciphertext")?;
    assert!(store.read_document().is_err());
    for plaintext in [
        b"not-json".as_slice(),
        br#"{"version":999,"entries":{}}"#.as_slice(),
    ] {
        let ciphertext = age::encrypt(&identity.to_public(), plaintext)?;
        fs::write(&store.store_path, ciphertext)?;
        fs::set_permissions(&store.store_path, fs::Permissions::from_mode(0o600))?;
        assert!(store.read_document().is_err());
    }
    Ok(())
}

#[test]
fn initialization_rejects_public_or_missing_identity_material()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(&temp, "unsafe");
    store.initialize()?;
    fs::set_permissions(&store.identity_path, fs::Permissions::from_mode(0o644))?;
    assert!(store.read_identity().is_err());
    fs::remove_file(&store.identity_path)?;
    assert!(store.read_identity().is_err());
    Ok(())
}
