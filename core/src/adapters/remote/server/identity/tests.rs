use std::fs;

#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt as _, symlink};

use super::DaemonIdentity;

#[test]
fn creates_private_identity_and_reloads_stable_fingerprint()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let created = DaemonIdentity::load_or_create(temp.path())?;
    let reloaded = DaemonIdentity::load_or_create(temp.path())?;
    assert_eq!(created.fingerprint, reloaded.fingerprint);
    assert_eq!(created.fingerprint.len(), 64);
    assert_eq!(
        fs::read(temp.path().join("daemon.identity"))?.len(),
        32
    );
    #[cfg(unix)]
    assert_eq!(
        fs::metadata(temp.path().join("daemon.identity"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    Ok(())
}

#[test]
fn rejects_wrong_length_directory_symlink_and_unsafe_mode()
-> Result<(), Box<dyn std::error::Error>> {
    for bytes in [vec![], vec![1; 31], vec![1; 33]] {
        let temp = crate::test_support::tempdir()?;
        fs::write(temp.path().join("daemon.identity"), bytes)?;
        assert!(DaemonIdentity::load_or_create(temp.path()).is_err());
    }

    let temp = crate::test_support::tempdir()?;
    fs::create_dir(temp.path().join("daemon.identity"))?;
    assert!(DaemonIdentity::load_or_create(temp.path()).is_err());

    #[cfg(unix)]
    {
        let temp = crate::test_support::tempdir()?;
        let source = temp.path().join("source");
        fs::write(&source, [1; 32])?;
        symlink(&source, temp.path().join("daemon.identity"))?;
        assert!(DaemonIdentity::load_or_create(temp.path()).is_err());

        let temp = crate::test_support::tempdir()?;
        let path = temp.path().join("daemon.identity");
        fs::write(&path, [1; 32])?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o622))?;
        assert!(DaemonIdentity::load_or_create(temp.path()).is_err());
    }
    Ok(())
}

#[test]
fn reports_state_root_filesystem_errors() -> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let missing_parent = temp.path().join("missing");
    assert!(DaemonIdentity::load_or_create(&missing_parent).is_err());

    let not_directory = temp.path().join("file");
    fs::write(&not_directory, b"file")?;
    assert!(DaemonIdentity::load_or_create(&not_directory).is_err());
    Ok(())
}
