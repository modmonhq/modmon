use std::fs;
use std::path::Path;

use super::{
    EnvironmentSecretStore, environment_storage_descriptor,
    load_or_create_environment_revision_key, validate,
};
use crate::app::ports::{SecretMaterialResolver as _, SecretMetadataInspector as _};
use crate::domain::secret::{SecretReference, SecretScope, SecretStatus};

#[test]
fn low_entropy_values_have_keyed_non_guessable_revision_stamps()
-> Result<(), Box<dyn std::error::Error>> {
    let reference: SecretReference = "env://MM_LOW_ENTROPY".parse()?;
    let first = EnvironmentSecretStore::new([7_u8; 32]);
    let second = EnvironmentSecretStore::new([9_u8; 32]);
    let revision = first.revision(&reference, "0");
    assert_eq!(revision, first.revision(&reference, "0"));
    assert_ne!(revision, first.revision(&reference, "1"));
    assert_ne!(revision, second.revision(&reference, "0"));
    assert!(!revision.contains("MM_LOW_ENTROPY"));
    assert!(!revision.contains(":0"));
    assert!(!format!("{first:?}").contains(&"07".repeat(32)));
    Ok(())
}

#[tokio::test]
async fn metadata_resolution_and_descriptor_cover_set_unset_and_rejected_references()
-> Result<(), Box<dyn std::error::Error>> {
    const CHILD: &str = "MM_ENVIRONMENT_STORE_TEST_CHILD";
    const VALUE: &str = "MM_ENVIRONMENT_STORE_TEST_VALUE";
    if std::env::var_os(CHILD).is_some() {
        let store = EnvironmentSecretStore::new([11; 32]);
        let scope = SecretScope::new("development");
        let reference: SecretReference = format!("env://{VALUE}").parse()?;
        let metadata = store.inspect(&scope, &reference).await?;
        assert_eq!(metadata.status, SecretStatus::Set);
        assert!(metadata.revision.is_some());
        let resolution = store.resolve(&scope, &reference).await?;
        assert_eq!(resolution.metadata.status, SecretStatus::Set);
        assert!(resolution.material.is_some());
        return Ok(());
    }

    let status = std::process::Command::new(std::env::current_exe()?)
        .args([
            "adapters::secrets::environment::tests::metadata_resolution_and_descriptor_cover_set_unset_and_rejected_references",
            "--exact",
        ])
        .env(CHILD, "1")
        .env(VALUE, "secret-value")
        .status()?;
    assert!(status.success());

    let store = EnvironmentSecretStore::new([11; 32]);
    let scope = SecretScope::new("development");
    let missing: SecretReference = "env://MM_ENVIRONMENT_STORE_DEFINITELY_MISSING".parse()?;
    assert_eq!(
        store.inspect(&scope, &missing).await?.status,
        SecretStatus::Unset
    );
    assert_eq!(
        store.resolve(&scope, &missing).await?.metadata.status,
        SecretStatus::Unset
    );
    assert_eq!(environment_storage_descriptor()?.id.as_str(), "env");
    let foreign: SecretReference = "age://entry".parse()?;
    assert!(validate(&foreign).is_err());
    let invalid: SecretReference = "env://bad-name".parse()?;
    assert!(validate(&invalid).is_err());
    let reserved: SecretReference = "env://PATH".parse()?;
    assert!(validate(&reserved).is_err());
    Ok(())
}

#[test]
fn revision_keys_are_private_stable_and_reject_invalid_lengths_and_paths()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("keys/revision.key");
    let first = load_or_create_environment_revision_key(&path)?;
    assert_eq!(load_or_create_environment_revision_key(&path)?, first);
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().ok_or("key parent missing")?)?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );

    let short = temp.path().join("short.key");
    fs::write(&short, [1])?;
    fs::set_permissions(&short, fs::Permissions::from_mode(0o600))?;
    assert!(load_or_create_environment_revision_key(&short).is_err());
    let long = temp.path().join("long.key");
    fs::write(&long, [2; 33])?;
    fs::set_permissions(&long, fs::Permissions::from_mode(0o600))?;
    assert!(load_or_create_environment_revision_key(&long).is_err());

    let blocked = temp.path().join("blocked");
    fs::write(&blocked, b"file")?;
    assert!(load_or_create_environment_revision_key(&blocked.join("key")).is_err());
    assert!(
        load_or_create_environment_revision_key(Path::new("/proc/mm-environment-key/key")).is_err()
    );
    let oversized_name = temp.path().join("x".repeat(300));
    assert!(load_or_create_environment_revision_key(&oversized_name).is_err());
    Ok(())
}
