use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use mm::adapters::target::FilesystemTraefikDynamicConfigStore;
use mm::app::ports::TraefikDynamicConfigStore;
use mm::app::render_traefik_dynamic_routes;
use mm::domain::error::MmError;
use mm::domain::target::TargetId;
use mm::domain::traefik_dynamic::RenderedTraefikDynamicConfig;
use tempfile::TempDir;
use uuid::Uuid;

mod temp_dirs;

const TARGET_ID: u128 = 0x319c527b62427b43bfd4bd60e27c1de3;

#[test]
fn publishes_a_deterministic_target_scoped_config() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let root = home.path().join("traefik");
    let store = FilesystemTraefikDynamicConfigStore::new(root.clone());
    let target_id = target_id();
    let config = rendered(target_id)?;

    let other_target_id = TargetId(Uuid::from_u128(TARGET_ID + 1));
    assert!(matches!(
        store.publish(other_target_id, &config),
        Err(MmError::Security { .. })
    ));
    assert!(!root.exists());

    let mut changed_digest = config.clone();
    changed_digest.manifest.digest = "0".repeat(64);
    assert!(matches!(
        store.publish(target_id, &changed_digest),
        Err(MmError::Security { .. })
    ));
    assert!(!root.exists());

    let first = store.publish(target_id, &config)?;
    let second = store.publish(target_id, &config)?;
    assert_eq!(first, second);
    assert_eq!(first.manifest, config.manifest);
    assert_eq!(
        first.root,
        root.join(target_id.0.to_string())
            .join("routes")
            .join(&config.manifest.digest)
    );
    assert_eq!(first.path, first.root.join("dynamic.yaml"));
    assert_eq!(fs::read(&first.path)?, config.bytes);
    let stored_manifest = fs::read(first.root.join("manifest.json"))?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&stored_manifest)?,
        serde_json::to_value(&config.manifest)?
    );
    assert_eq!(
        artifact_entries(&first.root)?,
        ["dynamic.yaml", "manifest.json"]
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(fs::metadata(&root)?.permissions().mode() & 0o077, 0);
        assert_eq!(fs::metadata(&first.root)?.permissions().mode() & 0o077, 0);
    }
    Ok(())
}

#[test]
fn rejects_changed_bytes_manifest_and_directory_contents() -> Result<(), Box<dyn std::error::Error>>
{
    let home = temp_dirs::tempdir()?;
    let store = FilesystemTraefikDynamicConfigStore::new(home.path().join("traefik"));
    let target_id = target_id();
    let config = rendered(target_id)?;
    let prepared = store.publish(target_id, &config)?;
    let manifest_path = prepared.root.join("manifest.json");
    let original_manifest = fs::read(&manifest_path)?;

    fs::write(&prepared.path, b"tampered\n")?;
    assert!(matches!(
        store.publish(target_id, &config),
        Err(MmError::Security { .. })
    ));
    fs::write(&prepared.path, &config.bytes)?;

    let mut wrong_manifest = config.manifest.clone();
    wrong_manifest.target_id = TargetId(Uuid::from_u128(TARGET_ID + 1));
    fs::write(&manifest_path, serde_json::to_vec_pretty(&wrong_manifest)?)?;
    assert!(matches!(
        store.publish(target_id, &config),
        Err(MmError::Security { .. })
    ));
    fs::write(&manifest_path, original_manifest)?;

    fs::write(prepared.root.join("unexpected"), b"tampered")?;
    assert!(matches!(
        store.publish(target_id, &config),
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_artifact_files_and_digest_locks() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let home = temp_dirs::tempdir()?;
    let outside_directory = home.path().join("outside-directory");
    fs::create_dir(&outside_directory)?;
    let linked_root = home.path().join("linked-root");
    symlink(&outside_directory, &linked_root)?;
    let target_id = target_id();
    let config = rendered(target_id)?;
    assert!(matches!(
        FilesystemTraefikDynamicConfigStore::new(linked_root).publish(target_id, &config),
        Err(MmError::Security { .. })
    ));
    assert!(fs::read_dir(&outside_directory)?.next().is_none());

    let root = home.path().join("traefik");
    let store = FilesystemTraefikDynamicConfigStore::new(root.clone());
    let prepared = store.publish(target_id, &config)?;
    let outside = home.path().join("outside");
    fs::write(&outside, b"outside")?;

    fs::remove_file(&prepared.path)?;
    symlink(&outside, &prepared.path)?;
    assert!(matches!(
        store.publish(target_id, &config),
        Err(MmError::Security { .. })
    ));
    assert_eq!(fs::read(&outside)?, b"outside");

    fs::remove_file(&prepared.path)?;
    fs::write(&prepared.path, &config.bytes)?;
    let lock = root
        .join(target_id.0.to_string())
        .join("routes")
        .join(format!(".{}.lock", config.manifest.digest));
    fs::remove_file(&lock)?;
    symlink(&outside, &lock)?;
    assert!(matches!(
        store.publish(target_id, &config),
        Err(MmError::Security { .. })
    ));
    assert_eq!(fs::read(&outside)?, b"outside");
    Ok(())
}

#[test]
fn concurrent_publishers_return_one_complete_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let root = home.path().join("traefik");
    let store = Arc::new(FilesystemTraefikDynamicConfigStore::new(root.clone()));
    let target_id = target_id();
    let config = Arc::new(rendered(target_id)?);
    let barrier = Arc::new(Barrier::new(8));
    let handles = (0..8)
        .map(|_| {
            let store = Arc::clone(&store);
            let config = Arc::clone(&config);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                store.publish(target_id, &config)
            })
        })
        .collect::<Vec<_>>();

    let mut published = Vec::new();
    for handle in handles {
        published.push(handle.join().map_err(|_| "publisher panicked")??);
    }
    let first = published.first().ok_or("no artifact was published")?;
    assert!(published.iter().all(|artifact| artifact == first));
    assert_eq!(fs::read(&first.path)?, config.bytes);
    assert_eq!(
        artifact_entries(&first.root)?,
        ["dynamic.yaml", "manifest.json"]
    );

    let routes = root.join(target_id.0.to_string()).join("routes");
    let mut route_entries = fs::read_dir(routes)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    route_entries.sort();
    assert_eq!(
        route_entries,
        [
            std::ffi::OsString::from(format!(".{}.lock", config.manifest.digest)),
            std::ffi::OsString::from(&config.manifest.digest),
        ]
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn publication_rejects_every_changed_artifact_type_size_link_and_mode()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let (_home, store, target_id, config, prepared) = published_fixture()?;
    fs::remove_file(&prepared.path)?;
    assert!(store.publish(target_id, &config).is_err());

    let (home, store, target_id, config, prepared) = published_fixture()?;
    fs::hard_link(&prepared.path, home.path().join("alias"))?;
    assert!(store.publish(target_id, &config).is_err());

    let (_home, store, target_id, config, prepared) = published_fixture()?;
    fs::write(&prepared.path, vec![b'x'; 1024 * 1024 + 1])?;
    assert!(store.publish(target_id, &config).is_err());

    let (_home, store, target_id, config, prepared) = published_fixture()?;
    fs::remove_file(prepared.root.join("manifest.json"))?;
    assert!(store.publish(target_id, &config).is_err());

    let (_home, store, target_id, config, prepared) = published_fixture()?;
    let manifest = prepared.root.join("manifest.json");
    fs::remove_file(&manifest)?;
    fs::create_dir(&manifest)?;
    assert!(store.publish(target_id, &config).is_err());

    let (_home, store, target_id, config, prepared) = published_fixture()?;
    fs::set_permissions(&prepared.root, fs::Permissions::from_mode(0o750))?;
    assert!(store.publish(target_id, &config).is_err());

    let (_home, store, target_id, config, prepared) = published_fixture()?;
    let lock = prepared
        .root
        .parent()
        .ok_or("routes directory missing")?
        .join(format!(".{}.lock", config.manifest.digest));
    fs::set_permissions(lock, fs::Permissions::from_mode(0o666))?;
    assert!(store.publish(target_id, &config).is_err());

    let home = temp_dirs::tempdir()?;
    let root = home.path().join("not-a-directory");
    fs::write(&root, b"file")?;
    let final_target = TargetId(Uuid::from_u128(TARGET_ID));
    assert!(
        FilesystemTraefikDynamicConfigStore::new(root)
            .publish(final_target, &rendered(final_target)?)
            .is_err()
    );
    Ok(())
}

fn target_id() -> TargetId {
    TargetId(Uuid::from_u128(TARGET_ID))
}

type PublishedFixture = (
    TempDir,
    FilesystemTraefikDynamicConfigStore,
    TargetId,
    RenderedTraefikDynamicConfig,
    mm::domain::traefik_dynamic::PreparedTraefikDynamicConfig,
);

fn published_fixture() -> Result<PublishedFixture, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let home = temp_dirs::tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let target_id = target_id();
    let config = rendered(target_id)?;
    let store = FilesystemTraefikDynamicConfigStore::new(home.path().join("traefik"));
    let prepared = store.publish(target_id, &config)?;
    Ok((home, store, target_id, config, prepared))
}

fn rendered(target_id: TargetId) -> Result<RenderedTraefikDynamicConfig, MmError> {
    render_traefik_dynamic_routes(
        target_id,
        &[],
        &std::collections::BTreeMap::new(),
        &std::collections::BTreeSet::new(),
    )
    .map_err(|error| MmError::Target {
        message: error.to_string(),
    })
}

fn artifact_entries(root: &std::path::Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut entries = fs::read_dir(root)?
        .map(|entry| {
            entry?
                .file_name()
                .into_string()
                .map_err(|_| std::io::Error::other("artifact entry name is not valid Unicode"))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    entries.sort();
    Ok(entries)
}
