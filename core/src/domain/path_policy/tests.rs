use super::{
    ensure_no_symlink_components, ensure_owned_by_current_user, ensure_private_file,
    ensure_private_permissions_supported, ensure_safe_identity_parent, ensure_safe_path_ancestors,
    ensure_user_controlled_directory, ensure_user_controlled_file, read_regular_file,
};
use crate::domain::error::MmError;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

fn set_mode(path: &std::path::Path, mode: u32) -> Result<(), std::io::Error> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[test]
fn symlink_components_are_rejected_even_when_the_leaf_does_not_exist()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let real = temporary.path().join("real");
    fs::create_dir(&real)?;
    let link = temporary.path().join("link");
    symlink(&real, &link)?;

    ensure_no_symlink_components(&real.join("missing"))?;
    assert!(matches!(
        ensure_no_symlink_components(&link.join("missing")),
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[test]
fn private_and_user_controlled_paths_enforce_type_and_permission_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    set_mode(temporary.path(), 0o700)?;
    let private = temporary.path().join("identity.txt");
    fs::write(&private, b"private")?;
    set_mode(&private, 0o600)?;

    ensure_safe_path_ancestors(temporary.path())?;
    ensure_private_file(&private)?;
    ensure_owned_by_current_user(&private)?;
    ensure_safe_identity_parent(temporary.path())?;
    ensure_user_controlled_directory(temporary.path())?;
    ensure_user_controlled_file(&private)?;
    ensure_private_permissions_supported()?;

    set_mode(&private, 0o640)?;
    assert!(matches!(
        ensure_private_file(&private),
        Err(MmError::Security { .. })
    ));
    set_mode(&private, 0o620)?;
    assert!(matches!(
        ensure_user_controlled_file(&private),
        Err(MmError::Security { .. })
    ));
    set_mode(&private, 0o600)?;
    assert!(matches!(
        ensure_private_file(temporary.path()),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        ensure_user_controlled_file(temporary.path()),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        ensure_user_controlled_directory(&private),
        Err(MmError::Security { .. })
    ));

    set_mode(temporary.path(), 0o720)?;
    assert!(matches!(
        ensure_safe_identity_parent(temporary.path()),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        ensure_user_controlled_directory(temporary.path()),
        Err(MmError::Security { .. })
    ));
    Ok(())
}

#[test]
fn bounded_file_reads_distinguish_absence_type_and_size() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = crate::test_support::tempdir()?;
    let missing = temporary.path().join("missing");
    assert_eq!(read_regular_file(&missing, 10)?, None);

    let file = temporary.path().join("document");
    fs::write(&file, b"contents")?;
    assert_eq!(read_regular_file(&file, 8)?, Some(b"contents".to_vec()));
    assert!(matches!(
        read_regular_file(&file, 7),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        read_regular_file(temporary.path(), 100),
        Err(MmError::Security { .. })
    ));

    let link = temporary.path().join("document-link");
    symlink(&file, &link)?;
    assert!(matches!(
        read_regular_file(&link, 100),
        Err(MmError::Io { .. })
    ));
    Ok(())
}

#[test]
fn missing_paths_preserve_operation_specific_io_context_and_relative_ancestors()
-> Result<(), Box<dyn std::error::Error>> {
    // A relative path is judged by the ancestors of the current directory.
    assert_eq!(
        ensure_safe_path_ancestors(std::path::Path::new(".")).is_ok(),
        ensure_safe_path_ancestors(&std::env::current_dir()?.join(".")).is_ok()
    );
    let temporary = crate::test_support::tempdir()?;
    set_mode(temporary.path(), 0o700)?;
    let file = temporary.path().join("file");
    fs::write(&file, b"value")?;
    set_mode(&file, 0o600)?;
    assert!(matches!(
        ensure_safe_path_ancestors(&file),
        Err(MmError::Security { .. })
    ));

    let missing = temporary.path().join("missing");
    for (result, context) in [
        (ensure_private_file(&missing), "inspecting private file"),
        (
            ensure_owned_by_current_user(&missing),
            "inspecting ownership",
        ),
        (
            ensure_safe_identity_parent(&missing),
            "inspecting identity directory",
        ),
        (
            ensure_user_controlled_directory(&missing),
            "inspecting project directory",
        ),
        (
            ensure_user_controlled_file(&missing),
            "inspecting project file",
        ),
    ] {
        assert!(matches!(
            result,
            Err(MmError::Io { context: actual, .. }) if actual.contains(context)
        ));
    }
    Ok(())
}

#[test]
fn metadata_and_ancestor_failures_preserve_their_security_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    set_mode(temporary.path(), 0o700)?;
    let oversized = temporary.path().join("x".repeat(300));
    for result in [
        ensure_no_symlink_components(&oversized),
        ensure_safe_path_ancestors(&oversized),
    ] {
        assert!(matches!(result, Err(MmError::Io { .. })));
    }

    let file = temporary.path().join("file");
    fs::write(&file, b"value")?;
    set_mode(&file, 0o600)?;
    set_mode(temporary.path(), 0o720)?;
    assert!(matches!(
        ensure_user_controlled_file(&file),
        Err(MmError::Security { .. })
    ));

    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn proc_files_cover_stream_growth_and_read_failure_boundaries() {
    assert!(matches!(
        read_regular_file(std::path::Path::new("/proc/self/cmdline"), 0),
        Err(MmError::Security { .. })
    ));
    assert!(matches!(
        read_regular_file(std::path::Path::new("/proc/self/mem"), usize::MAX),
        Err(MmError::Io { .. })
    ));
}
