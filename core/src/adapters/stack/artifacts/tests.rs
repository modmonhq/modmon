use std::fs;

use super::*;

fn failure(message: &str) -> MmError {
    MmError::State {
        message: message.to_owned(),
    }
}

#[test]
fn publication_and_file_helpers_cover_cleanup_validation_and_collision_failures()
-> Result<(), Box<dyn std::error::Error>> {
    resolve_publication(Ok(()), Ok(()), "hash")?;
    assert!(resolve_publication(Err(failure("primary")), Ok(()), "hash").is_err());
    assert!(resolve_publication(Ok(()), Err(failure("cleanup")), "hash").is_err());
    assert!(
        resolve_publication(Err(failure("primary")), Err(failure("cleanup")), "hash",).is_err()
    );

    let temporary = crate::test_support::tempdir()?;
    let path = temporary.path().join("artifact");
    write_new(&path, b"content")?;
    verify_file(&path, b"content")?;
    assert!(verify_file(&path, b"changed").is_err());
    assert!(verify_file(&temporary.path().join("missing"), b"content").is_err());
    assert!(write_new(&path, b"duplicate").is_err());
    assert!(
        write_new(
            &temporary.path().join("oversized"),
            &vec![b'x'; MAX_ARTIFACT_BYTES + 1],
        )
        .is_err()
    );

    let directory_lock = temporary.path().join("lock-directory");
    fs::create_dir(&directory_lock)?;
    assert!(artifact_lock(&directory_lock).is_err());
    let file_parent = temporary.path().join("file-parent");
    fs::write(&file_parent, b"file")?;
    assert!(create_directories(&file_parent.join("child")).is_err());
    assert!(create_directories(Path::new("/proc/mm-stack-artifacts")).is_err());
    assert!(write_new(&temporary.path().join("x".repeat(300)), b"content").is_err());
    assert!(artifact_lock(&temporary.path().join("y".repeat(300))).is_err());
    assert!(verify_artifact(&file_parent, &[], b"manifest").is_err());
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;

        let root = temporary.path().join("non-unicode");
        fs::create_dir(&root)?;
        fs::write(root.join(OsString::from_vec(vec![0xff])), b"file")?;
        assert!(verify_artifact(&root, &[], b"manifest").is_err());
    }
    assert!(validate_component("safe-name_1", "stack").is_ok());
    assert!(validate_component("../unsafe", "stack").is_err());
    assert!(validate_hash(&"a".repeat(64)).is_ok());
    assert!(validate_hash("short").is_err());
    assert_eq!(source_file_name(7), "user.007.compose.yaml");
    assert!(format!("{}", unexpected_artifact(&path)).contains("unexpected"));
    Ok(())
}
