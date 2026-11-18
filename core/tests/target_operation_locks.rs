use std::fs;
use std::path::{Path, PathBuf};

use mm::adapters::target::FilesystemTargetOperationLock;
use mm::app::ports::TargetOperationLock;
use mm::domain::error::MmError;
use mm::domain::target::TargetId;
use uuid::Uuid;

mod temp_dirs;

const FIRST_TARGET: u128 = 0x319c527b62427b43bfd4bd60e27c1de3;
const SECOND_TARGET: u128 = 0x419c527b62427b43bfd4bd60e27c1de3;

#[test]
fn excludes_a_second_operation_for_the_same_target() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let root = lock_root(home.path());
    let first = FilesystemTargetOperationLock::new(root.clone());
    let second = FilesystemTargetOperationLock::new(root);
    let target_id = TargetId(Uuid::from_u128(FIRST_TARGET));
    let _guard = first.acquire(target_id)?;

    let error = match second.acquire(target_id) {
        Ok(_) => return Err("a second operation acquired the same target lock".into()),
        Err(error) => error,
    };
    assert!(matches!(
        &error,
        MmError::TargetOperationInProgress { target_id: actual }
            if *actual == target_id.0
    ));
    assert_eq!(error.code(), "MM_TARGET_OPERATION_IN_PROGRESS");
    assert!(error.to_string().contains(&target_id.0.to_string()));
    Ok(())
}

#[test]
fn releases_the_operation_lock_when_the_guard_is_dropped() -> Result<(), Box<dyn std::error::Error>>
{
    let home = temp_dirs::tempdir()?;
    let root = lock_root(home.path());
    let first = FilesystemTargetOperationLock::new(root.clone());
    let second = FilesystemTargetOperationLock::new(root);
    let target_id = TargetId(Uuid::from_u128(FIRST_TARGET));

    let guard = first.acquire(target_id)?;
    drop(guard);
    let _replacement = second.acquire(target_id)?;
    Ok(())
}

#[test]
fn allows_operations_for_different_targets() -> Result<(), Box<dyn std::error::Error>> {
    let home = temp_dirs::tempdir()?;
    let locks = FilesystemTargetOperationLock::new(lock_root(home.path()));

    let _first = locks.acquire(TargetId(Uuid::from_u128(FIRST_TARGET)))?;
    let _second = locks.acquire(TargetId(Uuid::from_u128(SECOND_TARGET)))?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_a_symlinked_target_lock_file() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let home = temp_dirs::tempdir()?;
    let root = lock_root(home.path());
    fs::create_dir_all(&root)?;
    fs::set_permissions(
        root.parent().ok_or("lock root has no parent")?,
        fs::Permissions::from_mode(0o700),
    )?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let outside = home.path().join("outside-lock");
    let target_id = TargetId(Uuid::from_u128(FIRST_TARGET));
    symlink(&outside, lock_file(&root, target_id))?;

    let locks = FilesystemTargetOperationLock::new(root);
    assert!(matches!(
        locks.acquire(target_id),
        Err(MmError::Security { .. })
    ));
    assert!(!outside.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn creates_private_lock_directories_and_files() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let home = temp_dirs::tempdir()?;
    let root = lock_root(home.path());
    let target_id = TargetId(Uuid::from_u128(FIRST_TARGET));
    let locks = FilesystemTargetOperationLock::new(root.clone());

    let guard = locks.acquire(target_id)?;
    assert_eq!(
        fs::metadata(root.parent().ok_or("lock root has no parent")?)?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(&root)?.permissions().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(lock_file(&root, target_id))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    drop(guard);
    assert!(lock_file(&root, target_id).is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_non_private_non_regular_and_multiply_linked_lock_paths()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let home = temp_dirs::tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let target = TargetId(Uuid::from_u128(FIRST_TARGET));

    let permissive_root = home.path().join("permissive");
    fs::create_dir(&permissive_root)?;
    fs::set_permissions(&permissive_root, fs::Permissions::from_mode(0o750))?;
    assert!(
        FilesystemTargetOperationLock::new(permissive_root)
            .acquire(target)
            .is_err()
    );

    let file_root = home.path().join("file-root");
    fs::write(&file_root, b"file")?;
    assert!(
        FilesystemTargetOperationLock::new(file_root)
            .acquire(target)
            .is_err()
    );

    let root = home.path().join("locks");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let directory_lock = lock_file(&root, target);
    fs::create_dir(&directory_lock)?;
    let locks = FilesystemTargetOperationLock::new(root.clone());
    assert!(locks.acquire(target).is_err());
    fs::remove_dir(&directory_lock)?;

    fs::write(&directory_lock, b"")?;
    fs::set_permissions(&directory_lock, fs::Permissions::from_mode(0o644))?;
    assert!(locks.acquire(target).is_err());
    fs::remove_file(&directory_lock)?;

    let original = root.join("original");
    fs::write(&original, b"")?;
    fs::set_permissions(&original, fs::Permissions::from_mode(0o600))?;
    fs::hard_link(&original, &directory_lock)?;
    assert!(locks.acquire(target).is_err());

    assert!(
        FilesystemTargetOperationLock::new(PathBuf::from("safe/../unsafe"))
            .acquire(target)
            .is_err()
    );
    assert!(
        FilesystemTargetOperationLock::new(PathBuf::from("/"))
            .acquire(target)
            .is_err()
    );
    Ok(())
}

fn lock_root(home: &Path) -> PathBuf {
    home.join("run").join("locks")
}

fn lock_file(root: &Path, target_id: TargetId) -> PathBuf {
    root.join(format!("{}.lock", target_id.0))
}
