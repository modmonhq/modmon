use super::*;

fn storage(kinds: Vec<SecretMaterialKind>) -> Result<StorageCapabilityDescriptor, String> {
    Ok(StorageCapabilityDescriptor {
        id: StorageProviderId::new("store")?,
        operations: vec![StorageOperation::InspectMetadata],
        revision_semantics: RevisionSemantics::Observed,
        max_material_bytes: MAX_SECRET_MATERIAL_BYTES,
        max_location_bytes: 255,
        environment_scoping: StorageEnvironmentScoping::Isolated,
        material_kinds: kinds,
        rotation: RotationCapability::Replace,
        authentication: AuthenticationKind::LocalIdentity,
        metadata_without_material_resolution: true,
    })
}

fn delivery_descriptor(
    kinds: Vec<SecretMaterialKind>,
) -> Result<DeliveryCapabilityDescriptor, String> {
    Ok(DeliveryCapabilityDescriptor {
        id: DeliveryDriverId::new("delivery")?,
        operation: DeliveryOperation::MountedFile,
        environment_scoping: DeliveryEnvironmentScoping::Workload,
        material_kinds: kinds,
        max_material_bytes: MAX_SECRET_MATERIAL_BYTES,
        max_materialization_name_bytes: 8,
        runtimes: vec![RuntimeKind::DockerCompose],
        workload_kinds: vec![SecretWorkloadKind::Service],
        plaintext_persistence: PlaintextPersistence::OperationScopedPrivateFile,
    })
}

fn request<'a>(
    logical: &'a LogicalSecretId,
    backend: &'a StorageCapabilityDescriptor,
    delivery: &'a DeliveryCapabilityDescriptor,
    runtime: RuntimeKind,
    workload: SecretWorkloadKind,
    kind: SecretMaterialKind,
    name: &'a MaterializationName,
) -> SecretCompatibilityRequest<'a> {
    SecretCompatibilityRequest {
        logical_secret: logical,
        backend,
        delivery,
        runtime,
        workload,
        material_kind: kind,
        materialization_name: name,
    }
}

#[test]
fn typed_identifiers_and_materialization_names_enforce_each_lexical_boundary() -> Result<(), String>
{
    for valid in ["a", "driver-1", &"x".repeat(63)] {
        let storage = StorageProviderId::new(valid)?;
        assert_eq!(storage.as_str(), valid);
        assert_eq!(
            StorageProviderId::try_from(valid.to_owned())?.to_string(),
            valid
        );
        assert_eq!(String::from(storage), valid);
        let delivery = DeliveryDriverId::new(valid)?;
        assert_eq!(delivery.as_str(), valid);
        assert_eq!(
            DeliveryDriverId::try_from(valid.to_owned())?.to_string(),
            valid
        );
        assert_eq!(String::from(delivery), valid);
    }
    for invalid in ["", "1driver", "Upper", "bad_name", &"x".repeat(64)] {
        assert!(StorageProviderId::new(invalid).is_err(), "{invalid:?}");
        assert!(DeliveryDriverId::new(invalid).is_err(), "{invalid:?}");
    }

    let logical = LogicalSecretId::new("database.password")?;
    assert_eq!(logical.as_str(), "database.password");
    assert_eq!(logical.to_string(), "database.password");
    for invalid in ["", "-leading", "bad/name", &"x".repeat(256)] {
        assert!(LogicalSecretId::new(invalid).is_err(), "{invalid:?}");
    }
    let environment = MaterializationName::environment("DB_PASSWORD")?;
    assert_eq!(environment.as_str(), "DB_PASSWORD");
    assert_eq!(environment.to_string(), "DB_PASSWORD");
    for invalid in ["", "1NAME", "BAD-NAME", &"X".repeat(256)] {
        assert!(
            MaterializationName::environment(invalid).is_err(),
            "{invalid:?}"
        );
    }
    assert_eq!(
        MaterializationName::file("secret.txt")?.as_str(),
        "secret.txt"
    );
    for invalid in [
        "",
        ".",
        "..",
        "nested/file",
        "back\\slash",
        "line\nbreak",
        &"x".repeat(256),
    ] {
        assert!(MaterializationName::file(invalid).is_err(), "{invalid:?}");
    }
    Ok(())
}

#[test]
fn compatibility_checks_backend_delivery_runtime_workload_kind_and_name_independently()
-> Result<(), String> {
    let logical = LogicalSecretId::new("token")?;
    let name = MaterializationName::file("token")?;
    let backend = storage(vec![SecretMaterialKind::Text])?;
    let delivery = delivery_descriptor(vec![SecretMaterialKind::Text])?;
    assert!(
        plan_secret_compatibility(request(
            &logical,
            &backend,
            &delivery,
            RuntimeKind::DockerCompose,
            SecretWorkloadKind::Service,
            SecretMaterialKind::Text,
            &name
        ))
        .is_ok()
    );
    assert!(
        plan_secret_compatibility(request(
            &logical,
            &storage(vec![])?,
            &delivery,
            RuntimeKind::DockerCompose,
            SecretWorkloadKind::Service,
            SecretMaterialKind::Text,
            &name
        ))
        .is_err()
    );
    assert!(
        plan_secret_compatibility(request(
            &logical,
            &backend,
            &delivery,
            RuntimeKind::PodmanCompose,
            SecretWorkloadKind::Service,
            SecretMaterialKind::Text,
            &name
        ))
        .is_err()
    );
    assert!(
        plan_secret_compatibility(request(
            &logical,
            &backend,
            &delivery,
            RuntimeKind::DockerCompose,
            SecretWorkloadKind::Job,
            SecretMaterialKind::Text,
            &name
        ))
        .is_err()
    );
    assert!(
        plan_secret_compatibility(request(
            &logical,
            &backend,
            &delivery_descriptor(vec![])?,
            RuntimeKind::DockerCompose,
            SecretWorkloadKind::Service,
            SecretMaterialKind::Text,
            &name
        ))
        .is_err()
    );
    let long_name = MaterializationName::file("long-token")?;
    assert!(
        plan_secret_compatibility(request(
            &logical,
            &backend,
            &delivery,
            RuntimeKind::DockerCompose,
            SecretWorkloadKind::Service,
            SecretMaterialKind::Text,
            &long_name
        ))
        .is_err()
    );
    assert!(backend.supports(StorageOperation::InspectMetadata));
    assert!(!backend.supports(StorageOperation::Delete));
    Ok(())
}
