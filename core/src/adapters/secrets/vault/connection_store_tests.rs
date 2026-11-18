use super::*;

fn connection() -> VaultConnection {
    let ca_pem = "vault trust bundle".to_owned();
    VaultConnection {
        mode: VaultMode::External,
        address: "https://127.0.0.1:8200".to_owned(),
        namespace: None,
        mount: "secret".to_owned(),
        auth_mount: "approle".to_owned(),
        ca_sha256: ca_sha256(&ca_pem),
        ca_pem,
        project_id: ProjectId(uuid::Uuid::from_u128(61)),
        credentials: [(
            "local".to_owned(),
            VaultAppRoleCredential {
                role_id: Zeroizing::new("role-secret".to_owned()),
                secret_id: Zeroizing::new("id-secret".to_owned()),
            },
        )]
        .into(),
        allowed_locations: [("local".to_owned(), vec!["app/password".to_owned()])].into(),
        managed_installation: None,
    }
}

#[test]
fn encrypted_connection_recovery_and_backup_lifecycles_round_trip()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let store = VaultConnectionStore::new(
        temporary.path().join("identities/connection.key"),
        temporary.path().join("secrets/connections.age"),
    );
    assert!(!store.has_connection("primary")?);
    store.save_connection("primary", connection())?;
    let loaded = store.connection("primary")?;
    assert!(!format!("{loaded:?} {:?}", loaded.credentials["local"]).contains("role-secret"));
    assert!(!store.remove_connection("missing")?);
    assert!(store.remove_connection("primary")?);
    let recovery = VaultRecoveryMaterial {
        root_token: Zeroizing::new("root".to_owned()),
        unseal_key: Zeroizing::new("unseal".to_owned()),
    };
    assert!(format!("{recovery:?}").contains("[REDACTED]"));
    store.save_recovery("managed", recovery)?;
    assert!(store.has_recovery("managed")?);
    assert_eq!(store.recovery("managed")?.root_token.as_str(), "root");
    assert!(store.remove_recovery("managed")?);
    assert!(!store.remove_recovery("managed")?);
    assert!(store.recovery("managed").is_err());
    let encrypted = store.encrypt_integrity_checked(b"snapshot")?;
    assert_eq!(
        store.decrypt_integrity_checked(&encrypted)?.as_slice(),
        b"snapshot"
    );
    let mut tampered = encrypted;
    tampered[0] ^= 1;
    assert!(store.decrypt_integrity_checked(&tampered).is_err());
    Ok(())
}

#[test]
fn connection_validation_rejects_untrusted_fields() {
    let valid = connection();
    assert!(validate_connection(&valid).is_ok());
    for changed in [
        {
            let mut value = valid.clone();
            value.address = "http://localhost".to_owned();
            value
        },
        {
            let mut value = valid.clone();
            value.mount = "../secret".to_owned();
            value
        },
        {
            let mut value = valid.clone();
            value.ca_sha256 = "wrong".to_owned();
            value
        },
        {
            let mut value = valid.clone();
            value.credentials.clear();
            value
        },
        {
            let mut value = valid.clone();
            value.allowed_locations.clear();
            value
        },
    ] {
        assert!(validate_connection(&changed).is_err());
    }
    assert!(validate_name("safe-name").is_ok());
    assert!(validate_name("../unsafe").is_err());
}

#[test]
fn authenticated_envelopes_documents_identities_and_credentials_fail_closed()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let store = VaultConnectionStore::new(
        temporary.path().join("identity.key"),
        temporary.path().join("connections.age"),
    );
    let identity = store.load_or_create_identity()?;
    let truncated = age::encrypt(&identity.to_public(), b"short")?;
    assert!(store.decrypt_integrity_checked(&truncated).is_err());
    let mut invalid = vec![0_u8; 32];
    invalid.extend_from_slice(b"snapshot");
    let invalid = age::encrypt(&identity.to_public(), &invalid)?;
    assert!(store.decrypt_integrity_checked(&invalid).is_err());

    store.write_document(&Document::default())?;
    assert!(store.connection("missing").is_err());
    store.write_document(&Document {
        version: DOCUMENT_VERSION + 1,
        connections: BTreeMap::new(),
        recovery: BTreeMap::new(),
    })?;
    assert!(store.has_connection("missing").is_err());

    let invalid_identity = VaultConnectionStore::new(
        temporary.path().join("invalid-identity.key"),
        temporary.path().join("other.age"),
    );
    write_private(&invalid_identity.identity_path, &[0xff])?;
    assert!(invalid_identity.read_identity().is_err());
    write_private(&invalid_identity.identity_path, b"not-an-age-identity")?;
    assert!(invalid_identity.read_identity().is_err());

    let mut invalid_connection = connection();
    invalid_connection.address = "::not-a-url".to_owned();
    assert!(validate_connection(&invalid_connection).is_err());
    let mut incomplete = connection();
    incomplete
        .credentials
        .get_mut("local")
        .ok_or("credential missing")?
        .role_id
        .clear();
    assert!(validate_connection(&incomplete).is_err());
    assert!(parent(Path::new("/")).is_err());
    assert_eq!(
        VaultMode::from(crate::domain::secret_backend::ProviderMode::Managed),
        VaultMode::Managed
    );
    Ok(())
}

#[test]
fn connection_store_filesystem_failures_are_mapped_without_partial_publication()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let blocked = temporary.path().join("blocked");
    fs::write(&blocked, b"file")?;
    assert!(prepare_directory(&blocked.join("child")).is_err());
    assert!(write_private(&blocked.join("child/value"), b"secret").is_err());

    let identity_directory = temporary.path().join("identity-directory");
    fs::create_dir(&identity_directory)?;
    let store =
        VaultConnectionStore::new(identity_directory, temporary.path().join("connections.age"));
    assert!(store.read_identity().is_err());

    let identity = temporary.path().join("identity.key");
    let document_directory = temporary.path().join("document-directory");
    fs::create_dir(&document_directory)?;
    let store = VaultConnectionStore::new(identity.clone(), document_directory);
    store.load_or_create_identity()?;
    assert!(store.read_document().is_err());

    let locked = VaultConnectionStore::new(identity, PathBuf::from("/proc/mm-vault.age"));
    assert!(locked.lock().is_err());
    assert!(locked.write_document(&Document::default()).is_err());
    Ok(())
}
