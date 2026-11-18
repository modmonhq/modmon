use super::*;

#[test]
fn dashboard_ingress_token_is_private_stable_and_validated()
-> Result<(), Box<dyn std::error::Error>> {
    let root = crate::test_support::tempdir()?;
    let store = FilesystemServiceStackStore::new(root.path().join("services"));
    let token = store.load_or_create_dashboard_token()?;
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(store.load_or_create_dashboard_token()?, token);

    let path = root.path().join("services/dashboard-ingress.token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o600
        );
    }

    std::fs::write(&path, b"invalid")?;
    assert!(store.load_or_create_dashboard_token().is_err());
    Ok(())
}

#[test]
fn credentials_state_and_exact_artifacts_reject_corruption_and_collisions()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let store = FilesystemServiceStackStore::new(temporary.path().join("services"));
    assert!(validate_dashboard_token(&"a".repeat(64)).is_ok());
    assert!(validate_dashboard_token(&"A".repeat(64)).is_err());

    let token = store.load_or_create_dashboard_token()?;
    let token_path = temporary.path().join("services/dashboard-ingress.token");
    let mut non_utf8 = vec![b'a'; token.len()];
    non_utf8[0] = 0xff;
    fs::write(&token_path, non_utf8)?;
    assert!(store.load_or_create_dashboard_token().is_err());

    let key = store.ensure_cap_admin_key_file()?;
    assert_eq!(store.ensure_cap_admin_key_file()?, key);
    fs::write(&key, b"invalid")?;
    assert!(store.ensure_cap_admin_key_file().is_err());

    assert!(store.load_state()?.is_none());
    fs::write(temporary.path().join("services/state.json"), b"not-json")?;
    assert!(store.load_state().is_err());
    store.remove_state()?;
    store.remove_state()?;

    let identity = "b".repeat(64);
    let input = ServiceStackArtifactInput {
        plan_identity: &identity,
        manifest: b"manifest",
        compose: b"services: {}\n",
        route: b"http: {}\n",
        dashboard_unit: b"[Service]\n",
    };
    let prepared = store.publish(input)?;
    assert!(prepared.compose_path.is_file());
    store.publish(ServiceStackArtifactInput {
        plan_identity: &identity,
        manifest: b"manifest",
        compose: b"services: {}\n",
        route: b"http: {}\n",
        dashboard_unit: b"[Service]\n",
    })?;
    fs::write(&prepared.compose_path, b"changed")?;
    assert!(
        store
            .publish(ServiceStackArtifactInput {
                plan_identity: &identity,
                manifest: b"manifest",
                compose: b"services: {}\n",
                route: b"http: {}\n",
                dashboard_unit: b"[Service]\n",
            })
            .is_err()
    );
    assert!(
        store
            .publish(ServiceStackArtifactInput {
                plan_identity: "invalid",
                manifest: b"",
                compose: b"",
                route: b"",
                dashboard_unit: b"",
            })
            .is_err()
    );
    Ok(())
}

#[test]
fn filesystem_failures_for_state_directories_and_atomic_files_are_mapped()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let root = temporary.path().join("services");
    fs::create_dir(&root)?;
    let store = FilesystemServiceStackStore::new(root.clone());
    let state = root.join("state.json");
    fs::create_dir(&state)?;
    assert!(store.load_state().is_err());
    assert!(store.remove_state().is_err());
    assert!(publish_exact(&state, b"state").is_err());

    assert!(prepare_directory(Path::new("/proc/mm-service-stack")).is_err());
    assert!(write_private(Path::new("/proc/mm-service-stack-file"), b"value").is_err());
    let directory = temporary.path().join("atomic-directory");
    fs::create_dir(&directory)?;
    assert!(write_private(&directory, b"value").is_err());
    Ok(())
}
