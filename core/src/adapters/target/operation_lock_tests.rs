use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};

use super::*;

fn target(value: u128) -> TargetId {
    TargetId(uuid::Uuid::from_u128(value))
}

#[test]
fn acquisition_is_private_exclusive_and_reusable() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let root = temp.path().join("locks");
    let adapter = FilesystemTargetOperationLock::new(root.clone());
    assert!(format!("{adapter:?}").contains("FilesystemTargetOperationLock"));

    let guard = TargetOperationLock::acquire(&adapter, target(1))?;
    let path = lock_path(&root, target(1));
    assert!(path.is_file());
    assert_eq!(fs::metadata(&root)?.permissions().mode() & 0o777, 0o700);
    assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
    assert!(matches!(
        TargetOperationLock::acquire(&adapter, target(1)),
        Err(MmError::TargetOperationInProgress { .. })
    ));

    drop(guard);
    let _reacquired = TargetOperationLock::acquire(&adapter, target(1))?;
    let _independent = TargetOperationLock::acquire(&adapter, target(2))?;
    Ok(())
}

#[test]
fn build_locks_use_the_project_identity() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700))?;
    let root = temp.path().join("locks");
    let adapter = FilesystemTargetOperationLock::new(root.clone());
    let project = ProjectId(uuid::Uuid::from_u128(3));
    let guard = BuildOperationLock::acquire(&adapter, project)?;
    assert!(lock_path(&root, TargetId(project.0)).is_file());
    assert!(matches!(
        BuildOperationLock::acquire(&adapter, project),
        Err(MmError::TargetOperationInProgress { .. })
    ));
    drop(guard);
    Ok(())
}

#[test]
fn malformed_and_non_private_roots_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    assert!(validate_root_path(Path::new("")).is_err());
    assert!(validate_root_path(Path::new("safe/../unsafe")).is_err());
    assert!(validate_root_path(Path::new("safe/locks")).is_ok());
    assert!(
        unsafe_lock_path(Path::new("unsafe"))
            .to_string()
            .contains("symlink-free")
    );

    let temp = crate::test_support::tempdir()?;
    let permissive = temp.path().join("permissive");
    fs::create_dir(&permissive)?;
    fs::set_permissions(&permissive, fs::Permissions::from_mode(0o750))?;
    let adapter = FilesystemTargetOperationLock::new(permissive);
    assert!(matches!(
        TargetOperationLock::acquire(&adapter, target(4)),
        Err(MmError::Security { .. })
    ));

    let file = temp.path().join("file-root");
    fs::write(&file, "not a directory")?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))?;
    let adapter = FilesystemTargetOperationLock::new(file);
    assert!(TargetOperationLock::acquire(&adapter, target(5)).is_err());
    Ok(())
}

#[test]
fn symlinked_roots_and_lock_entries_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let real = temp.path().join("real");
    fs::create_dir(&real)?;
    fs::set_permissions(&real, fs::Permissions::from_mode(0o700))?;
    let linked = temp.path().join("linked");
    symlink(&real, &linked)?;
    let adapter = FilesystemTargetOperationLock::new(linked);
    assert!(TargetOperationLock::acquire(&adapter, target(6)).is_err());

    let root = temp.path().join("locks");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let victim = temp.path().join("victim");
    fs::write(&victim, "unchanged")?;
    fs::set_permissions(&victim, fs::Permissions::from_mode(0o600))?;
    symlink(&victim, lock_path(&root, target(7)))?;
    let adapter = FilesystemTargetOperationLock::new(root);
    assert!(TargetOperationLock::acquire(&adapter, target(7)).is_err());
    assert_eq!(fs::read_to_string(victim)?, "unchanged");
    Ok(())
}

#[test]
fn multiply_linked_lock_files_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("locks");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let first = root.join("first");
    fs::write(&first, "")?;
    fs::set_permissions(&first, fs::Permissions::from_mode(0o600))?;
    fs::hard_link(&first, lock_path(&root, target(8)))?;

    let adapter = FilesystemTargetOperationLock::new(root);
    assert!(matches!(
        TargetOperationLock::acquire(&adapter, target(8)),
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[test]
fn directory_and_non_private_lock_entries_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("locks");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;

    let directory_lock = lock_path(&root, target(9));
    fs::create_dir(&directory_lock)?;
    let adapter = FilesystemTargetOperationLock::new(root.clone());
    assert!(TargetOperationLock::acquire(&adapter, target(9)).is_err());
    fs::remove_dir(directory_lock)?;

    let permissive_lock = lock_path(&root, target(10));
    fs::write(&permissive_lock, b"")?;
    fs::set_permissions(&permissive_lock, fs::Permissions::from_mode(0o644))?;
    assert!(TargetOperationLock::acquire(&adapter, target(10)).is_err());
    assert!(
        TargetOperationLock::acquire(
            &FilesystemTargetOperationLock::new(PathBuf::from("/")),
            target(11),
        )
        .is_err()
    );
    Ok(())
}
