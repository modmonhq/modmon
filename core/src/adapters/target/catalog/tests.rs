use std::fs;
use std::os::unix::fs::PermissionsExt;
use uuid::Uuid;

use super::*;
use crate::domain::target::TargetId;

fn local(id: u128) -> TargetDefinition {
    TargetDefinition::local(TargetId(Uuid::from_u128(id)))
}

fn ingress() -> TargetIngress {
    TargetIngress::Traefik {
        image: format!("traefik@sha256:{}", "a".repeat(64)),
        acme_email: None,
        acme_ca_server: None,
        acme_ca_certificate: None,
    }
}

#[test]
fn catalog_store_covers_lifecycle_idempotency_and_optimistic_conflicts()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("private/targets.yaml");
    fs::create_dir(path.parent().ok_or("target catalog path has no parent")?)?;
    let store = FilesystemTargetStore::new(path.clone());
    assert!(store.list_targets()?.is_empty());
    assert!(store.resolve_target("missing").is_err());
    assert!(store.resolve_target("../unsafe").is_err());
    assert!(store.insert_target("../unsafe", local(1)).is_err());
    assert!(store.ensure_target("../unsafe", local(1)).is_err());
    let mut invalid = local(1);
    invalid.runtime = Some(crate::domain::target::TargetRuntimeEndpoint {
        socket: Some("relative.sock".into()),
        endpoint: None,
        executable: None,
    });
    assert!(store.ensure_target("invalid", invalid).is_err());
    assert!(store.remove_target("../unsafe", "stale").is_err());
    assert!(
        store
            .update_target_transport("../unsafe", "stale", TargetTransport::Local)
            .is_err()
    );
    assert!(
        store
            .update_target_ingress("../unsafe", "stale", ingress())
            .is_err()
    );

    let inserted = store.insert_target("local", local(1))?;
    assert_eq!(store.list_targets()?.len(), 1);
    assert_eq!(store.resolve_target("local")?, inserted);
    assert!(store.insert_target("local", local(2)).is_err());
    assert_eq!(store.ensure_target("local", local(2))?, inserted);
    let ensured = store.ensure_target("second", local(2))?;
    assert_eq!(store.list_targets()?.len(), 2);

    assert!(
        store
            .update_target_transport("missing", "stale", TargetTransport::Local)
            .is_err()
    );
    assert!(
        store
            .update_target_transport("local", "stale", TargetTransport::Local)
            .is_err()
    );
    let updated = store.update_target_transport(
        "local",
        &inserted.fingerprint,
        TargetTransport::Ssh {
            host: "example.test".to_owned(),
            port: 22,
            user: "deploy".to_owned(),
            host_key_fingerprint: "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
            host_key: None,
            identity_file: None,
            remote_root: None,
            daemon_fingerprint: None,
        },
    )?;
    assert!(matches!(
        updated.definition.transport,
        TargetTransport::Ssh { .. }
    ));

    assert!(
        store
            .update_target_ingress("missing", "stale", ingress())
            .is_err()
    );
    assert!(
        store
            .update_target_ingress("local", "stale", ingress())
            .is_err()
    );
    let ingress = store.update_target_ingress("local", &updated.fingerprint, ingress())?;
    assert!(ingress.definition.ingress.is_some());

    assert!(store.remove_target("missing", "stale").is_err());
    assert!(store.remove_target("local", "stale").is_err());
    assert_eq!(
        store.remove_target("local", &ingress.fingerprint)?.name,
        "local"
    );
    assert_eq!(
        store.remove_target("second", &ensured.fingerprint)?.name,
        "second"
    );
    assert!(store.list_targets()?.is_empty());
    assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    Ok(())
}

#[test]
fn catalog_store_rejects_malformed_oversized_public_and_aliased_files()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::{PermissionsExt, symlink};

    let temp = crate::test_support::tempdir()?;
    let path = temp.path().join("targets.yaml");
    let store = FilesystemTargetStore::new(path.clone());
    fs::write(&path, b"not: [yaml")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(store.list_targets().is_err());
    fs::write(
        &path,
        format!(
            "targets:\n  invalid:\n    id: {}\n    transport: local\n    bootstrap: ansible\n    available-runtimes: []\n",
            Uuid::from_u128(10)
        ),
    )?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    assert!(store.list_targets().is_err());
    fs::write(&path, vec![b'x'; MAX_TARGET_CATALOG_BYTES + 1])?;
    assert!(store.list_targets().is_err());
    fs::write(&path, b"targets: {}\n")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert!(store.list_targets().is_err());
    fs::remove_file(&path)?;
    let source = temp.path().join("source.yaml");
    fs::write(&source, b"targets: {}\n")?;
    symlink(&source, &path)?;
    assert!(store.list_targets().is_err());
    assert!(set_private_file(&temp.path().join("missing")).is_err());
    Ok(())
}
