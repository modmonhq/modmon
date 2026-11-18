use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};

use tempfile::TempDir;

use super::*;
use crate::domain::traefik_dynamic::TraefikDynamicConfigManifest;

fn rendered(target_id: TargetId, bytes: Vec<u8>) -> RenderedTraefikDynamicConfig {
    let digest = blake3::hash(&bytes).to_hex().to_string();
    RenderedTraefikDynamicConfig {
        manifest: TraefikDynamicConfigManifest {
            target_id,
            claim_count: 2,
            router_count: 2,
            service_count: 1,
            digest,
        },
        bytes,
    }
}

type PublishedFixture = (
    TempDir,
    FilesystemTraefikDynamicConfigStore,
    TargetId,
    RenderedTraefikDynamicConfig,
    PreparedTraefikDynamicConfig,
);

fn published() -> Result<PublishedFixture, Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let target_id = TargetId(uuid::Uuid::new_v4());
    let config = rendered(target_id, b"http:\n  routers: {}\n".to_vec());
    let store = FilesystemTraefikDynamicConfigStore::new(temp.path().join("dynamic"));
    let prepared = store.publish(target_id, &config)?;
    Ok((temp, store, target_id, config, prepared))
}

#[test]
fn publication_is_private_idempotent_and_detects_every_artifact_change()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let root = temp.path().join("dynamic");
    let target_id = TargetId(uuid::Uuid::from_u128(101));
    let config = rendered(target_id, b"http:\n  routers: {}\n".to_vec());
    let store = FilesystemTraefikDynamicConfigStore::new(root.clone());
    assert!(format!("{store:?}").contains("FilesystemTraefikDynamicConfigStore"));

    let first = store.publish(target_id, &config)?;
    assert_eq!(first.path, first.root.join(DYNAMIC_CONFIG_FILE));
    assert_eq!(fs::read(&first.path)?, config.bytes);
    assert_eq!(
        fs::read(first.root.join("manifest.json"))?,
        serialize_manifest(&config)?
    );
    assert_eq!(store.publish(target_id, &config)?, first);

    fs::write(&first.path, b"tampered")?;
    assert!(store.publish(target_id, &config).is_err());
    fs::write(&first.path, &config.bytes)?;
    fs::write(first.root.join("unexpected"), b"extra")?;
    assert!(store.publish(target_id, &config).is_err());
    fs::remove_file(first.root.join("unexpected"))?;
    fs::write(first.root.join("manifest.json"), b"not-json")?;
    assert!(store.publish(target_id, &config).is_err());
    Ok(())
}

#[test]
fn validation_rejects_wrong_targets_digests_sizes_and_unsafe_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let target_id = TargetId(uuid::Uuid::from_u128(102));
    let config = rendered(target_id, b"http: {}\n".to_vec());
    validate_config(target_id, &config)?;
    assert!(validate_config(TargetId(uuid::Uuid::from_u128(103)), &config).is_err());
    let mut changed = config.clone();
    changed.manifest.digest = "0".repeat(64);
    assert!(validate_config(target_id, &changed).is_err());
    assert!(
        validate_config(
            target_id,
            &rendered(target_id, vec![b'x'; MAX_DYNAMIC_CONFIG_BYTES + 1])
        )
        .is_err()
    );

    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let store = FilesystemTraefikDynamicConfigStore::new(temp.path().join("child/../dynamic"));
    assert!(store.publish(target_id, &config).is_err());
    let real = temp.path().join("real");
    fs::create_dir(&real)?;
    let linked = temp.path().join("linked");
    symlink(&real, &linked)?;
    assert!(
        FilesystemTraefikDynamicConfigStore::new(linked)
            .publish(target_id, &config)
            .is_err()
    );
    Ok(())
}

#[test]
fn publication_rejects_symlinked_locks_and_non_private_roots()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let target_id = TargetId(uuid::Uuid::from_u128(104));
    let config = rendered(target_id, b"tls: {}\n".to_vec());
    let root = temp.path().join("dynamic");
    let routes = root.join(target_id.0.to_string()).join("routes");
    fs::create_dir_all(&routes)?;
    for path in [
        root.as_path(),
        routes.parent().ok_or("target root missing")?,
        routes.as_path(),
    ] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    let victim = temp.path().join("victim");
    fs::write(&victim, b"unchanged")?;
    let lock = routes.join(format!(".{}.lock", config.manifest.digest));
    symlink(&victim, &lock)?;
    let store = FilesystemTraefikDynamicConfigStore::new(root.clone());
    assert!(store.publish(target_id, &config).is_err());
    assert_eq!(fs::read(&victim)?, b"unchanged");

    fs::remove_file(lock)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o770))?;
    assert!(store.publish(target_id, &config).is_err());
    Ok(())
}

#[test]
fn existing_publications_reject_missing_linked_oversized_and_mismatched_files()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store, target_id, config, prepared) = published()?;
    fs::remove_file(&prepared.path)?;
    assert!(store.publish(target_id, &config).is_err());

    let (temp, store, target_id, config, prepared) = published()?;
    let victim = temp.path().join("victim");
    fs::write(&victim, &config.bytes)?;
    fs::remove_file(&prepared.path)?;
    symlink(&victim, &prepared.path)?;
    assert!(store.publish(target_id, &config).is_err());

    let (temp, store, target_id, config, prepared) = published()?;
    let alias = temp.path().join("alias");
    fs::hard_link(&prepared.path, &alias)?;
    assert!(store.publish(target_id, &config).is_err());

    let (_temp, store, target_id, config, prepared) = published()?;
    fs::write(&prepared.path, vec![b'x'; MAX_DYNAMIC_CONFIG_BYTES + 1])?;
    assert!(store.publish(target_id, &config).is_err());

    let (_temp, store, target_id, config, prepared) = published()?;
    let manifest_path = prepared.root.join("manifest.json");
    let mut manifest = config.manifest.clone();
    manifest.target_id = TargetId(uuid::Uuid::new_v4());
    let mut bytes = serde_json::to_vec_pretty(&manifest)?;
    bytes.push(b'\n');
    fs::write(manifest_path, bytes)?;
    assert!(store.publish(target_id, &config).is_err());
    Ok(())
}

#[test]
fn existing_publications_reject_non_private_artifact_and_lock_modes()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store, target_id, config, prepared) = published()?;
    fs::set_permissions(&prepared.root, fs::Permissions::from_mode(0o750))?;
    assert!(store.publish(target_id, &config).is_err());

    let (_temp, store, target_id, config, prepared) = published()?;
    let routes = prepared.root.parent().ok_or("routes missing")?;
    let lock = routes.join(format!(".{}.lock", config.manifest.digest));
    fs::set_permissions(lock, fs::Permissions::from_mode(0o666))?;
    assert!(store.publish(target_id, &config).is_err());
    Ok(())
}

#[test]
fn publication_rejects_missing_wrong_named_and_non_directory_artifacts()
-> Result<(), Box<dyn std::error::Error>> {
    let (_temp, store, target_id, config, prepared) = published()?;
    fs::remove_file(prepared.root.join("manifest.json"))?;
    assert!(store.publish(target_id, &config).is_err());

    let (_temp, store, target_id, config, prepared) = published()?;
    fs::remove_file(prepared.root.join("manifest.json"))?;
    fs::write(prepared.root.join("wrong.json"), b"{}")?;
    assert!(store.publish(target_id, &config).is_err());

    let (_temp, store, target_id, config, prepared) = published()?;
    let manifest = prepared.root.join("manifest.json");
    fs::remove_file(&manifest)?;
    fs::create_dir(&manifest)?;
    assert!(store.publish(target_id, &config).is_err());

    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let target_id = TargetId(uuid::Uuid::new_v4());
    let config = rendered(target_id, b"http: {}\n".to_vec());
    let root = temp.path().join("dynamic");
    fs::write(&root, b"not a directory")?;
    assert!(
        FilesystemTraefikDynamicConfigStore::new(root)
            .publish(target_id, &config)
            .is_err()
    );
    Ok(())
}

#[test]
fn unix_path_and_directory_helpers_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;

    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let root = temp.path().join("private");
    let root_directory = unix::open_private_root(&root)?;
    unix::validate_root_path(&root)?;
    assert!(unix::validate_root_path(&root.join("child/../escape")).is_err());
    assert!(unix::walk_path(&root.join("missing"), false).is_err());
    assert!(unix::same_file(&root_directory, &File::open(&root)?)?);

    let other = temp.path().join("other");
    fs::create_dir(&other)?;
    fs::set_permissions(&other, fs::Permissions::from_mode(0o700))?;
    assert!(!unix::same_file(&root_directory, &File::open(&other)?)?);
    assert!(unix::verify_root_identity(&other, &root_directory).is_err());

    unix::create_private_directory(&root_directory, "child", &root.join("child"))?;
    unix::create_private_directory(&root_directory, "child", &root.join("child"))?;
    let child =
        unix::open_or_create_private_directory(&root_directory, "child", &root.join("child"))?;
    unix::validate_private_directory(&child, &root.join("child"))?;
    assert!(
        unix::verify_directory_entry(&root_directory, "missing", &child, &root.join("missing"))
            .is_err()
    );

    let regular_path = root.join("regular");
    fs::write(&regular_path, b"value")?;
    let regular = File::open(&regular_path)?;
    assert!(unix::validate_private_directory(&regular, &regular_path).is_err());
    assert!(unix::try_open_directory(&root_directory, "regular", &regular_path).is_err());
    assert!(
        unix::create_private_directory(&regular, "child", &regular_path.join("child")).is_err()
    );
    assert!(matches!(
        unix::publication_race(&regular_path),
        MmError::Security { .. }
    ));
    assert!(unix::verify_exact_entries(&regular, &regular_path).is_err());
    assert!(
        unix::read_bounded_file(
            &regular,
            "child",
            &regular_path.join("child"),
            MAX_DYNAMIC_CONFIG_BYTES,
        )
        .is_err()
    );
    assert!(
        unix::write_new_file(&regular, "child", &regular_path.join("child"), b"value",).is_err()
    );
    assert!(unix::entry_metadata(&regular, "child", &regular_path).is_err());
    assert!(unix::verify_directory_entry(&regular, "child", &child, &regular_path).is_err());
    Ok(())
}

#[test]
fn unix_file_helpers_detect_aliases_missing_entries_and_size_limits()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;

    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let directory = unix::open_private_root(&temp.path().join("private"))?;
    let root = temp.path().join("private");

    unix::write_new_file(&directory, "value", &root.join("value"), b"contents")?;
    assert!(unix::write_new_file(&directory, "value", &root.join("value"), b"again").is_err());
    let value = File::open(root.join("value"))?;
    unix::validate_regular_file(&value, &root.join("value"))?;
    unix::verify_file_entry(&directory, "value", &value, &root.join("value"))?;
    assert!(unix::verify_file_entry(&directory, "missing", &value, &root.join("missing")).is_err());
    assert!(unix::entry_metadata(&directory, "missing", &root.join("missing")).is_err());

    let alias = root.join("alias");
    fs::hard_link(root.join("value"), &alias)?;
    assert!(unix::validate_regular_file(&value, &root.join("value")).is_err());
    fs::remove_file(alias)?;

    fs::write(root.join("large"), b"too large")?;
    assert!(unix::read_bounded_file(&directory, "large", &root.join("large"), 2).is_err());
    fs::create_dir(root.join("directory"))?;
    assert!(
        unix::read_bounded_file(
            &directory,
            "directory",
            &root.join("directory"),
            MAX_DYNAMIC_CONFIG_BYTES,
        )
        .is_err()
    );
    assert!(unix::verify_exact_entries(&directory, &root).is_err());
    Ok(())
}

#[test]
fn unix_lock_and_staging_cleanup_helpers_enforce_identity_and_privacy()
-> Result<(), Box<dyn std::error::Error>> {
    use std::fs::File;

    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let root = temp.path().join("private");
    let directory = unix::open_private_root(&root)?;
    let lock_path = root.join("lock");
    let lock = unix::open_lock(&directory, "lock", &lock_path)?;
    unix::verify_lock(&directory, "lock", &lock, &lock_path)?;
    fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o666))?;
    assert!(unix::verify_lock(&directory, "lock", &lock, &lock_path).is_err());

    let staging_name = "staging";
    let staging_path = root.join(staging_name);
    fs::create_dir(&staging_path)?;
    fs::write(staging_path.join(DYNAMIC_CONFIG_FILE), b"dynamic")?;
    fs::write(staging_path.join("manifest.json"), b"manifest")?;
    let staging = File::open(&staging_path)?;
    unix::cleanup_staging(&directory, staging_name, &staging);
    assert!(!staging_path.exists());

    unix::cleanup_staging(&directory, "missing", &directory);
    Ok(())
}
