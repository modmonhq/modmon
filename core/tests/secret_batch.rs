//! Asking the age store for many secrets at once reads it once, and says
//! the same as asking for each. Run with `--nocapture` for the times.

use std::sync::Arc;
use std::time::Instant;

use mm::adapters::secrets::AgeSecretStore;
use mm::app::ports::{SecretCreator, SecretMetadataInspector};
use mm::domain::secret::{SecretMaterial, SecretReference, SecretScope, SecretStatus};

mod temp_dirs;

#[tokio::test]
async fn inspecting_many_secrets_reads_the_store_once() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let store = Arc::new(AgeSecretStore::new(
        home.path().join("identity.key"),
        home.path().join("project.age"),
    ));
    store.initialize()?;
    let scope = SecretScope::new("integration");
    let mut references = Vec::new();
    for index in 0..68 {
        let reference: SecretReference = format!("age://app/secret-{index}").parse()?;
        if index % 10 != 0 {
            store
                .create(
                    &scope,
                    &reference,
                    SecretMaterial::new(format!("value-{index}"))?,
                    1_000,
                    None,
                )
                .await?;
        }
        references.push(reference);
    }
    let one_by_one = Instant::now();
    let mut each = Vec::new();
    for reference in &references {
        each.push(store.inspect(&scope, reference).await?);
    }
    let one_by_one = one_by_one.elapsed();
    let at_once = Instant::now();
    let many = store.inspect_many(&scope, &references).await?;
    let at_once = at_once.elapsed();
    println!("68 secrets: one by one {one_by_one:?}, at once {at_once:?}");
    assert_eq!(each, many);
    assert_eq!(
        many.iter()
            .filter(|metadata| metadata.status == SecretStatus::Unset)
            .count(),
        7
    );
    Ok(())
}
