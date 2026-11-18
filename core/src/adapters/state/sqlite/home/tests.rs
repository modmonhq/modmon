use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use super::*;

#[test]
fn explicit_home_is_created_privatized_and_maps_every_managed_path()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("nested/mm-home");
    let home = MmHome::resolve(Some(&root))?;
    assert_eq!(home.root, dunce::canonicalize(&root)?);
    assert_eq!(home.state_database, root.join("state.sqlite3"));
    assert_eq!(home.identities_directory, root.join("identities"));
    assert_eq!(home.secrets_directory, root.join("secrets"));
    assert_eq!(home.run_directory, root.join("run"));
    assert_eq!(home.target_locks_directory, root.join("run/locks"));
    assert_eq!(home.compose_directory, root.join("compose"));
    assert_eq!(home.builds_directory, root.join("builds"));
    assert_eq!(home.ca_directory, root.join("ca"));
    assert_eq!(home.targets_file, root.join("targets.yaml"));
    assert_eq!(home.ingress_directory, root.join("ingress"));
    assert_eq!(home.services_directory, root.join("services"));
    assert_eq!(fs::metadata(&root)?.permissions().mode() & 0o777, 0o700);

    let file = root.join("private");
    fs::write(&file, b"secret")?;
    make_private_file(&file)?;
    assert_eq!(fs::metadata(file)?.permissions().mode() & 0o777, 0o600);
    Ok(())
}

#[test]
fn explicit_home_rejects_files_symlinks_and_public_unrelated_content()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let file = temp.path().join("file-home");
    fs::write(&file, b"not a directory")?;
    assert!(MmHome::resolve(Some(&file)).is_err());

    let source = temp.path().join("source-home");
    fs::create_dir(&source)?;
    let alias = temp.path().join("alias-home");
    symlink(&source, &alias)?;
    assert!(MmHome::resolve(Some(&alias)).is_err());

    let public = temp.path().join("public-home");
    fs::create_dir(&public)?;
    fs::write(public.join("unrelated"), b"value")?;
    fs::set_permissions(&public, fs::Permissions::from_mode(0o755))?;
    assert!(MmHome::resolve(Some(&public)).is_err());
    assert!(make_private_file(&temp.path().join("missing")).is_err());
    Ok(())
}

#[test]
fn public_home_is_claimable_only_when_entries_match_managed_shapes()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("claimable");
    fs::create_dir(&root)?;
    for directory in [
        "identities",
        "secrets",
        "run",
        "compose",
        "ingress",
        "services",
    ] {
        fs::create_dir(root.join(directory))?;
    }
    for file in [
        "targets.yaml",
        "targets.yaml.lock",
        "state.sqlite3",
        "state.sqlite3-wal",
        "state.sqlite3-shm",
        "state.sqlite3-journal",
    ] {
        fs::write(root.join(file), b"")?;
    }
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755))?;
    assert!(is_claimable_mm_home(&root)?);
    assert!(MmHome::resolve(Some(&root)).is_ok());

    let wrong = temp.path().join("wrong-shape");
    fs::create_dir(&wrong)?;
    fs::write(wrong.join("identities"), b"not a directory")?;
    assert!(!is_claimable_mm_home(&wrong)?);
    fs::remove_file(wrong.join("identities"))?;
    fs::create_dir(wrong.join("state.sqlite3"))?;
    assert!(!is_claimable_mm_home(&wrong)?);
    Ok(())
}

#[test]
fn claimability_rejects_symlink_and_non_unicode_entries() -> Result<(), Box<dyn std::error::Error>>
{
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = crate::test_support::tempdir()?;
    let root = temp.path().join("entries");
    fs::create_dir(&root)?;
    fs::create_dir(root.join("real"))?;
    symlink(root.join("real"), root.join("identities"))?;
    assert!(!is_claimable_mm_home(&root)?);
    fs::remove_file(root.join("identities"))?;
    if !crate::test_support::NON_UTF8_FILE_NAMES {
        return Ok(());
    }
    fs::write(root.join(OsString::from_vec(vec![0xff])), b"value")?;
    assert!(!is_claimable_mm_home(&root)?);
    Ok(())
}
