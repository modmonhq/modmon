use tempfile::TempDir;

use super::*;
use crate::domain::secret::BundledSecret;

fn store(temp: &TempDir) -> AgeSecretStore {
    AgeSecretStore::new(
        temp.path().join("identities/age.key"),
        temp.path().join("secrets/project.age"),
    )
}

fn secret(locator: &str, revision: &str, value: &str) -> Result<BundledSecret, String> {
    Ok(BundledSecret {
        reference: format!("age://{locator}").parse()?,
        revision: revision.to_owned(),
        material: SecretMaterial::new(value.to_owned())?,
    })
}

async fn active(
    store: &AgeSecretStore,
    scope: &SecretScope,
    locator: &str,
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error>> {
    let reference: SecretReference = format!("age://{locator}").parse()?;
    let resolution = SecretMaterialResolver::resolve(store, scope, &reference).await?;
    Ok((
        resolution.metadata.revision,
        resolution
            .material
            .map(|material| material.expose(str::to_owned)),
    ))
}

#[tokio::test]
async fn recipient_initializes_once_and_stays_stable() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(&temp);
    assert!(!store.initialized());
    let first = store.ensure_recipient()?;
    assert!(first.starts_with("age1"));
    assert!(store.initialized());
    assert_eq!(store.ensure_recipient()?, first);
    Ok(())
}

#[tokio::test]
async fn mirror_keeps_given_revisions_and_skips_unchanged_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(&temp);
    let scope = SecretScope::new("integration");
    assert!(store.mirror(&scope, vec![secret("a", "r1", "v1")?], 1).is_err());
    store.ensure_recipient()?;

    let results = store.mirror(
        &scope,
        vec![secret("a", "r1", "v1")?, secret("b", "r2", "v2")?],
        10,
    )?;
    assert_eq!(
        results,
        vec![
            SecretWriteResult {
                revision: "r1".to_owned(),
                changed: true
            },
            SecretWriteResult {
                revision: "r2".to_owned(),
                changed: true
            },
        ]
    );
    assert_eq!(
        active(&store, &scope, "a").await?,
        (Some("r1".to_owned()), Some("v1".to_owned()))
    );
    // Other environments are separate entries.
    assert_eq!(
        active(&store, &SecretScope::new("production"), "a").await?,
        (None, None)
    );

    let unchanged = fs::read(&store.store_path)?;
    let results = store.mirror(&scope, vec![secret("a", "r1", "v1")?], 20)?;
    assert!(!results[0].changed);
    assert_eq!(fs::read(&store.store_path)?, unchanged, "no-op rewrote the store");

    let results = store.mirror(&scope, vec![secret("a", "r3", "v3")?], 30)?;
    assert!(results[0].changed);
    assert_eq!(
        active(&store, &scope, "a").await?,
        (Some("r3".to_owned()), Some("v3".to_owned()))
    );
    let document = store.read_document()?;
    let entry = document
        .entries
        .get("integration/a")
        .ok_or("entry missing")?;
    assert_eq!(
        entry.previous.as_ref().map(|previous| previous.revision.as_str()),
        Some("r1")
    );
    // Secrets the batch does not list stay.
    assert_eq!(active(&store, &scope, "b").await?.0, Some("r2".to_owned()));
    Ok(())
}

#[tokio::test]
async fn mirror_rejects_invalid_batches_without_writing() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let store = store(&temp);
    let scope = SecretScope::new("integration");
    store.ensure_recipient()?;
    let before = fs::read(&store.store_path)?;
    for batch in [
        vec![secret("a", "r1", "v1")?, secret("a", "r2", "v2")?],
        vec![secret("a", "bad revision", "v1")?],
        vec![secret("a", "r1", "line\nbreak")?],
        vec![BundledSecret {
            reference: "vault://a".parse()?,
            revision: "r1".to_owned(),
            material: SecretMaterial::new("v1".to_owned())?,
        }],
    ] {
        assert!(store.mirror(&scope, batch, 1).is_err());
    }
    assert_eq!(fs::read(&store.store_path)?, before);
    Ok(())
}
