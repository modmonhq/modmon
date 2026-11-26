use super::*;

#[test]
fn pre_release_secret_document_is_rejected_without_rewrite()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let store = AgeSecretStore::new(
        temporary.path().join("identity.key"),
        temporary.path().join("secrets.age"),
    );
    store.initialize()?;
    let identity = store.read_identity()?;
    let plaintext = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "entries": {}
    }))?;
    let ciphertext = age::encrypt(&identity.to_public(), &plaintext)?;
    atomic_write_private(&store.store_path, &ciphertext)?;
    let before = fs::read(&store.store_path)?;
    assert!(store.read_document().is_err());
    assert_eq!(fs::read(&store.store_path)?, before);
    Ok(())
}

#[test]
fn private_lifecycle_helpers_validate_redact_bound_and_publish_atomically()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = crate::test_support::tempdir()?;
    let store = AgeSecretStore::new(
        temporary.path().join("identity.key"),
        temporary.path().join("secrets.age"),
    );
    assert!(ensure_initialized(&store).is_err());
    assert!(validate_material(&SecretMaterial::new("valid".to_owned())?).is_ok());
    assert!(validate_material(&SecretMaterial::new("bad\nvalue".to_owned())?).is_err());

    let active = StoredSecret {
        value: Zeroizing::new("secret-value".to_owned()),
        revision: "active".to_owned(),
        created_at_unix_ms: 1,
        set_by: None,
    };
    let mut entry = SecretEntry {
        active,
        candidate: Some(StoredSecret {
            value: Zeroizing::new("candidate-value".to_owned()),
            revision: "candidate".to_owned(),
            created_at_unix_ms: 2,
            set_by: None,
        }),
        previous: None,
        retired: Vec::new(),
    };
    let document = SecretDocument {
        version: CURRENT_SECRET_DOCUMENT_VERSION,
        entries: [("local/value".to_owned(), entry)].into(),
        host: HostStaging::default(),
    };
    let debug = format!("{document:?} {:?}", document.entries["local/value"]);
    assert!(!debug.contains("secret-value"));
    entry = document.entries.into_values().next().ok_or("entry missing")?;
    assert!(!format!("{:?}", entry.active).contains("secret-value"));
    assert_eq!(stored_metadata(&entry).candidate_revision.as_deref(), Some("candidate"));
    assert!(ensure_expected_state(
        Some(&entry),
        &SecretExpectedState { active_revision: None, candidate_revision: None },
    ).is_err());
    assert!(replace_active(
        &mut entry,
        stored_secret(SecretMaterial::new("next".to_owned())?, 3, None),
        3,
    ).is_err());

    entry.candidate = None;
    entry.active.value.clear();
    assert_eq!(stored_metadata(&entry).status, SecretStatus::Invalid);
    assert_eq!(unset_metadata().status, SecretStatus::Unset);
    for index in 0..=MAX_RETIRED_REVISIONS {
        retire(&mut entry, format!("revision-{index}"), i64::try_from(index)?);
    }
    assert_eq!(entry.retired.len(), MAX_RETIRED_REVISIONS);
    assert!(format!("{}", lifecycle_conflict()).contains("lifecycle"));
    assert!(parent(Path::new("/")).is_err());
    let age_reference: SecretReference = "age://value".parse()?;
    let foreign_reference: SecretReference = "env://VALUE".parse()?;
    ensure_age(&age_reference)?;
    assert!(ensure_age(&foreign_reference).is_err());
    assert_eq!(storage_key(&SecretScope::new("local"), &age_reference), "local/value");

    let candidate = stored_secret(SecretMaterial::new("promoted".to_owned())?, 4, None);
    entry.previous = Some(stored_secret(SecretMaterial::new("previous".to_owned())?, 3, None));
    promote_candidate(&mut entry, candidate, 4);
    assert_eq!(entry.retired.len(), MAX_RETIRED_REVISIONS);

    let private = temporary.path().join("private/value");
    create_private_directory(private.parent().ok_or("private parent missing")?)?;
    atomic_write_private(&private, b"private")?;
    assert_eq!(fs::read(&private)?, b"private");
    let identity = temporary.path().join("published.key");
    assert!(publish_new_identity(&identity, b"identity")?);
    assert!(!publish_new_identity(&identity, b"replacement")?);
    create_private_directory(temporary.path())?;
    assert!(atomic_write_private(Path::new("/proc/mm-age-private"), b"value").is_err());
    let directory_destination = temporary.path().join("directory-destination");
    fs::create_dir(&directory_destination)?;
    assert!(atomic_write_private(&directory_destination, b"value").is_err());
    assert!(publish_new_identity(Path::new("/proc/mm-age-identity"), b"value").is_err());
    assert!(create_private_directory(Path::new("/proc/mm-age-directory")).is_err());
    assert!(set_private_file(&temporary.path().join("missing-file")).is_err());
    assert!(set_private_directory(&temporary.path().join("missing-directory")).is_err());
    assert!(age_storage_descriptor()?.operations.len() >= 8);
    Ok(())
}

mod release_0_1_2 {
    use std::collections::BTreeMap;

    use serde::{Deserialize, Serialize};
    use zeroize::Zeroizing;

    #[derive(Serialize, Deserialize)]
    pub(super) struct SecretDocument {
        pub(super) version: u32,
        pub(super) entries: BTreeMap<String, SecretEntry>,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct SecretEntry {
        pub(super) active: StoredSecret,
        pub(super) candidate: Option<StoredSecret>,
        pub(super) previous: Option<StoredSecret>,
        pub(super) retired: Vec<RetiredSecret>,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct StoredSecret {
        pub(super) value: Zeroizing<String>,
        pub(super) revision: String,
        pub(super) created_at_unix_ms: i64,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct RetiredSecret {
        pub(super) revision: String,
        pub(super) retired_at_unix_ms: i64,
    }
}

#[test]
fn release_0_1_2_and_this_release_read_each_others_documents()
-> Result<(), Box<dyn std::error::Error>> {
    let written = stored_secret(SecretMaterial::new("current".to_owned())?, 7, Some("alex"));
    let revision = written.revision.clone();
    let entries = BTreeMap::from([(
        "local/value".to_owned(),
        SecretEntry {
            active: written,
            candidate: Some(stored_secret(SecretMaterial::new("next".to_owned())?, 8, Some("sam"))),
            previous: None,
            retired: Vec::new(),
        },
    )]);
    let host = HostStaging::default();
    let current = serde_json::to_vec(&CurrentSecretDocument {
        version: CURRENT_SECRET_DOCUMENT_VERSION,
        entries: &entries,
        host: &host,
    })?;
    assert!(String::from_utf8(current.clone())?.contains("\"set_by\":\"alex\""));

    let old = serde_json::from_slice::<release_0_1_2::SecretDocument>(&current)?;
    let entry = &old.entries["local/value"];
    assert_eq!(entry.active.revision, revision);
    assert_eq!(entry.active.value.as_str(), "current");
    assert_eq!(entry.active.created_at_unix_ms, 7);

    let rewritten = serde_json::to_vec(&old)?;
    let read = serde_json::from_slice::<SecretDocument>(&rewritten)?;
    let metadata = stored_metadata(&read.entries["local/value"]);
    assert_eq!(metadata.revision.as_deref(), Some(revision.as_str()));
    assert_eq!(metadata.created_at_unix_ms, Some(7));
    assert_eq!(metadata.set_by, None, "a document an older mm rewrote names no writer");
    Ok(())
}
