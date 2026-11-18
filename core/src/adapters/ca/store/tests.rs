    use super::*;

    #[test]
    fn install_issue_rotate_and_uninstall_keep_keys_off_metadata() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        let installed = store.install()?;
        assert!(installed.installed);
        assert!(
            installed
                .fingerprint
                .as_deref()
                .is_some_and(|value| value.starts_with("sha256:"))
        );
        let issued = store.issue(&["app.test".to_owned()].into_iter().collect())?;
        assert_eq!(issued[0].hosts, ["app.test"]);
        assert!(
            store
                .certificate_pem("app.test")?
                .contains("BEGIN CERTIFICATE")
        );
        assert!(store.key_pem("app.test")?.contains("BEGIN"));
        let status = serde_json::to_string(&store.status()?).map_err(|error| MmError::State {
            message: error.to_string(),
        })?;
        assert!(!status.contains("BEGIN"));
        let rotated = store.rotate()?;
        assert_ne!(rotated.fingerprint, installed.fingerprint);
        assert!(rotated.issued_hosts.contains("app.test"));
        store.uninstall()?;
        assert!(!store.status()?.installed);
        Ok(())
    }

    #[test]
    fn public_names_are_rejected() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        assert!(
            store
                .issue(&["www.example.com".to_owned()].into_iter().collect())
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn pre_release_state_is_rejected_without_rewrite() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        let path = store.root.join(STATE_FILE);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).map_err(|source| MmError::Io {
                context: "reading CA test state".to_owned(),
                source,
            })?)
            .map_err(|error| MmError::State {
                message: error.to_string(),
            })?;
        value["version"] = serde_json::json!(2);
        fs::write(
            &path,
            serde_json::to_vec_pretty(&value).map_err(|error| MmError::State {
                message: error.to_string(),
            })?,
        )
        .map_err(|source| MmError::Io {
            context: "writing CA test state".to_owned(),
            source,
        })?;
        let before = fs::read(&path).map_err(|source| MmError::Io {
            context: "reading CA test state".to_owned(),
            source,
        })?;
        assert!(store.status().is_err());
        assert_eq!(
            fs::read(path).map_err(|source| MmError::Io {
                context: "reading CA test state".to_owned(),
                source,
            })?,
            before
        );
        Ok(())
    }

    #[test]
    fn issued_leaf_names_the_requested_host() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        store.issue(&["app.test".to_owned()].into_iter().collect())?;
        let pem = store.certificate_pem("app.test")?;
        assert!(pem.contains("BEGIN CERTIFICATE"));
        verify_leaf_matches_host(&pem, "app.test")?;
        assert!(verify_leaf_matches_host(&pem, "other.test").is_err());
        Ok(())
    }

    #[test]
    fn issuance_is_idempotent_and_reconcile_preserves_shared_references() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        let hosts = ["app.test".to_owned(), "api.test".to_owned()]
            .into_iter()
            .collect();
        let first = store.issue(&hosts)?;
        let second = store.issue(&hosts)?;
        assert_eq!(first, second);

        store.reconcile(&["app.test".to_owned()].into_iter().collect())?;
        assert!(store.certificate_path("app.test")?.is_file());
        assert!(store.certificate_path("api.test").is_err());
        assert_eq!(
            store.status()?.issued_hosts,
            ["app.test".to_owned()].into_iter().collect()
        );
        Ok(())
    }

    #[test]
    fn interrupted_rotation_can_restore_the_exact_previous_generation() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        let original = store.install()?;
        store.issue(&["app.test".to_owned()].into_iter().collect())?;
        let rotated = store.rotate()?;
        assert!(rotated.rotation_pending);
        assert_ne!(rotated.fingerprint, original.fingerprint);

        let restored = store.rollback_rotation()?;
        assert_eq!(restored.fingerprint, original.fingerprint);
        assert_eq!(restored.generation, original.generation);
        assert!(!restored.rotation_pending);
        let certificate = store.certificate_pem("app.test")?;
        verify_leaf_matches_host(&certificate, "app.test")?;
        let issued = store.issue(&["app.test".to_owned()].into_iter().collect())?;
        assert_eq!(
            issued[0].fingerprint,
            certificate_fingerprint(certificate.as_bytes())
                .map_err(ca_error)?
                .to_string()
        );
        Ok(())
    }

    #[test]
    fn tampered_leaf_material_is_reissued_instead_of_trusted_from_state() -> Result<(), MmError> {
        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        let first = store.issue(&["app.test".to_owned()].into_iter().collect())?;
        fs::write(store.issued_path("app.test", "crt"), b"not a certificate").map_err(
            |source| MmError::Io {
                context: "tampering with test certificate".to_owned(),
                source,
            },
        )?;

        let status = store.status()?;
        assert!(status.leaves[0].stale);
        let renewed = store.issue(&["app.test".to_owned()].into_iter().collect())?;
        assert_ne!(renewed[0].fingerprint, first[0].fingerprint);
        verify_leaf_matches_host(&store.certificate_pem("app.test")?, "app.test")?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn keys_are_private_and_symlinked_key_material_is_rejected() -> Result<(), MmError> {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let directory = crate::test_support::tempdir().map_err(|source| MmError::Io {
            context: "creating CA test directory".to_owned(),
            source,
        })?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        store.issue(&["app.test".to_owned()].into_iter().collect())?;
        let metadata = fs::metadata(store.key_path("app.test")?).map_err(|source| MmError::Io {
            context: "inspecting test key".to_owned(),
            source,
        })?;
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        let key = store.issued_path("app.test", "key");
        let replacement = directory.path().join("replacement.key");
        fs::rename(&key, &replacement).map_err(|source| MmError::Io {
            context: "moving test key".to_owned(),
            source,
        })?;
        symlink(&replacement, &key).map_err(|source| MmError::Io {
            context: "creating test key symlink".to_owned(),
            source,
        })?;
        assert!(store.key_path("app.test").is_err());
        Ok(())
    }

    #[test]
    fn local_ca_port_covers_rotation_completion_and_absent_store_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        let port: &dyn crate::app::ports::LocalCaStore = &store;

        assert!(!port.status()?.installed);
        assert!(port.rotate().is_err());
        assert!(port.finish_rotation().is_err());
        assert!(port.rollback_rotation().is_err());
        assert_eq!(port.rotation_previous_fingerprint()?, None);
        assert!(port.issue(&BTreeSet::new()).is_err());
        assert!(port.certificate_pem("missing.test").is_err());
        assert!(port.key_pem("missing.test").is_err());
        assert!(port.ca_certificate_pem().is_err());
        assert!(port.set_trust_disabled(true).is_err());
        assert!(port.reconcile(&BTreeSet::new()).is_err());
        assert!(port.certificate_path("missing.test").is_err());
        assert!(port.key_path("missing.test").is_err());

        let installed = port.install()?;
        assert_eq!(port.install()?.fingerprint, installed.fingerprint);
        assert!(port.set_trust_disabled(true)?.trust_disabled);
        let hosts = ["service.local".to_owned()].into_iter().collect();
        assert_eq!(port.issue(&hosts)?.len(), 1);
        assert!(port.certificate_pem("service.local")?.contains("BEGIN CERTIFICATE"));
        assert!(port.key_pem("service.local")?.contains("BEGIN"));
        assert!(port.ca_certificate_pem()?.contains("BEGIN CERTIFICATE"));
        assert!(port.certificate_path("service.local")?.is_file());
        assert!(port.key_path("service.local")?.is_file());

        let rotated = port.rotate()?;
        assert!(rotated.rotation_pending);
        assert_eq!(
            port.rotation_previous_fingerprint()?,
            installed.fingerprint
        );
        assert!(port.rotate().is_err());
        assert!(!port.finish_rotation()?.rotation_pending);
        assert_eq!(port.rotation_previous_fingerprint()?, None);
        assert!(!port.finish_rotation()?.rotation_pending);
        assert!(port.rollback_rotation().is_err());
        assert!(!port.uninstall()?.installed);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn state_tampering_marks_leaves_stale_and_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::fs::PermissionsExt;

        let directory = crate::test_support::tempdir()?;
        let store = FilesystemLocalCaStore::new(directory.path().join("ca"));
        store.install()?;
        store.issue(&["app.test".to_owned()].into_iter().collect())?;

        let state_path = store.root.join(STATE_FILE);
        let original_state = fs::read(&state_path)?;
        let cert_path = store.root.join(CA_CERT_FILE);
        let key_path = store.root.join(CA_KEY_FILE);
        let original_cert = fs::read(&cert_path)?;
        let original_key = fs::read(&key_path)?;
        let (other_cert, other_key) = generate_ca()?;

        fs::write(&cert_path, other_cert)?;
        assert!(store.status().is_err());
        fs::write(&cert_path, &original_cert)?;

        fs::write(&key_path, other_key)?;
        assert!(store.issue(&["other.test".to_owned()].into_iter().collect()).is_err());
        fs::write(&key_path, &original_key)?;
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))?;

        let leaf_cert_path = store.issued_path("app.test", "crt");
        let leaf_key_path = store.issued_path("app.test", "key");
        let leaf_cert = fs::read(&leaf_cert_path)?;
        let leaf_key = fs::read(&leaf_key_path)?;
        let mut state = store.load_state()?;
        let leaf = state
            .leaves
            .get_mut("app.test")
            .ok_or("issued leaf state missing")?;
        leaf.generation = 0;
        store.save_state(&state)?;
        assert!(store.status()?.leaves[0].stale);

        fs::write(&state_path, &original_state)?;
        fs::set_permissions(&leaf_key_path, fs::Permissions::from_mode(0o644))?;
        assert!(store.status()?.leaves[0].stale);
        assert!(store.key_path("app.test").is_err());
        fs::set_permissions(&leaf_key_path, fs::Permissions::from_mode(0o600))?;

        fs::remove_file(&leaf_cert_path)?;
        assert!(store.status()?.leaves[0].stale);
        fs::write(&leaf_cert_path, &leaf_cert)?;
        fs::remove_file(&leaf_key_path)?;
        assert!(store.status()?.leaves[0].stale);
        fs::write(&leaf_key_path, &leaf_key)?;
        fs::set_permissions(&leaf_key_path, fs::Permissions::from_mode(0o600))?;
        fs::write(&leaf_cert_path, [0xff, 0xfe])?;
        assert!(store.status()?.leaves[0].stale);
        assert!(store.certificate_pem("app.test").is_err());
        fs::write(&leaf_cert_path, &leaf_cert)?;

        fs::remove_file(&state_path)?;
        assert!(store.status().is_err());
        fs::write(&state_path, b"{")?;
        assert!(store.status().is_err());
        fs::write(&state_path, original_state)?;

        let backup = store.root.join(ROTATION_STATE_BACKUP);
        fs::write(&backup, b"not-json")?;
        assert!(store.rotation_previous_fingerprint().is_err());
        assert!(store.rollback_rotation().is_err());
        Ok(())
    }

    #[test]
    fn certificate_and_atomic_file_helpers_cover_rejected_inputs()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = crate::test_support::tempdir()?;
        let (certificate, key) = generate_ca()?;
        assert!(certificate_matches_key(&certificate, &key));
        assert!(!certificate_matches_key(&certificate, "not-a-key"));
        assert!(!certificate_matches_key("not-a-certificate", &key));
        assert!(!certificate_matches_key(
            "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----\n",
            &key
        ));
        assert!(verify_leaf_matches_host("not-a-certificate", "app.test").is_err());
        assert!(leaf_expiration("not-a-certificate").is_err());

        let missing = directory.path().join("missing");
        assert!(remove_if_exists(&missing).is_ok());
        let nonempty = directory.path().join("nonempty");
        fs::create_dir(&nonempty)?;
        fs::write(nonempty.join("child"), b"child")?;
        assert!(remove_if_exists(&nonempty).is_err());
        assert!(sync_directory(&missing).is_err());
        assert!(read_text(&missing).is_err());
        let invalid_text = directory.path().join("invalid-text");
        fs::write(&invalid_text, [0xff])?;
        assert!(read_text(&invalid_text).is_err());

        assert!(write_private_temp(Path::new("/"), b"value", 0o600).is_err());
        let absent_parent = directory.path().join("absent/file");
        assert!(write_private_temp(&absent_parent, b"value", 0o600).is_err());
        let blocked = directory.path().join("blocked");
        fs::create_dir(directory.path().join(".blocked.tmp"))?;
        assert!(write_private_temp(&blocked, b"value", 0o600).is_err());

        let destination = directory.path().join("destination");
        fs::create_dir(&destination)?;
        assert!(write_atomic_bundle(&[(&destination, b"value", 0o600)]).is_err());
        fs::remove_file(directory.path().join(".destination.tmp"))?;
        Ok(())
    }
