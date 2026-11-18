use super::*;

#[test]
fn openbao_and_vault_modes_parse_to_distinct_typed_variants()
-> Result<(), Box<dyn std::error::Error>> {
    for driver in ["openbao", "vault"] {
        let external: ProviderInstanceConfig = serde_yaml_ng::from_str(&format!(
            "driver: {driver}\nscope: project\nmode: external\naddress: https://provider.example.test\nstorage-mount: mm-kv\nautomation-auth-mount: mm-approle\ntrust-bundle: ca.pem\nexpected-trust-sha256: sha256:{}\n",
            "a".repeat(64)
        ))?;
        assert_eq!(external.driver.as_str(), driver);
        assert!(matches!(
            external.configuration,
            ProviderConfiguration::External { .. }
        ));

        let managed: ProviderInstanceConfig = serde_yaml_ng::from_str(&format!(
            "driver: {driver}\nscope: installation\nmode: managed\ninstallation: local-secrets\nrelease: '1.0'\nmanagement-port: 18200\n"
        ))?;
        assert_eq!(managed.driver.as_str(), driver);
        assert!(matches!(
            managed.configuration,
            ProviderConfiguration::Managed { .. }
        ));
    }
    Ok(())
}

#[test]
fn provider_identifiers_modes_operations_and_configuration_are_exact() -> Result<(), String> {
    for valid in ["a", "provider-1", &"x".repeat(63)] {
        let name = ProviderInstanceName::new(valid)?;
        assert_eq!(name.as_str(), valid);
        assert_eq!(name.to_string(), valid);
        assert_eq!(String::from(name), valid);
        assert_eq!(
            ProviderDriverId::try_from(valid.to_owned())?.as_str(),
            valid
        );
    }
    for invalid in ["", "1provider", "Provider", "bad_name", &"x".repeat(64)] {
        assert!(ProviderInstanceName::new(invalid).is_err(), "{invalid:?}");
        assert!(ProviderDriverId::new(invalid).is_err(), "{invalid:?}");
    }
    assert_eq!(ProviderMode::Managed.as_str(), "managed");
    assert_eq!(ProviderMode::External.as_str(), "external");

    let operations = [
        ProviderOperation::Add,
        ProviderOperation::Edit,
        ProviderOperation::Test,
        ProviderOperation::Status,
        ProviderOperation::Install,
        ProviderOperation::Restart,
        ProviderOperation::Upgrade,
        ProviderOperation::Seal,
        ProviderOperation::Unseal,
        ProviderOperation::Backup,
        ProviderOperation::Restore,
        ProviderOperation::Remove,
    ];
    assert_eq!(
        operations
            .iter()
            .map(|value| value.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        operations.len()
    );
    let actions = [
        ManagedSecretBackendAction::Install,
        ManagedSecretBackendAction::Restart,
        ManagedSecretBackendAction::Upgrade,
        ManagedSecretBackendAction::Seal,
        ManagedSecretBackendAction::Unseal,
        ManagedSecretBackendAction::Remove,
    ];
    assert_eq!(
        actions.map(ManagedSecretBackendAction::as_str),
        ["install", "restart", "upgrade", "seal", "unseal", "remove"]
    );
    assert_eq!(ManagedSecretDataAction::Backup.as_str(), "backup");
    assert_eq!(ManagedSecretDataAction::Restore.as_str(), "restore");

    let managed = ProviderConfiguration::Managed {
        installation: Some("local".to_owned()),
        release: "1.0".to_owned(),
        management_port: Some(8200),
        nodes: None,
    };
    assert_eq!(managed.mode(), ProviderMode::Managed);
    let external = ProviderConfiguration::External {
        address: "https://provider.test".to_owned(),
        namespace: None,
        storage_mount: "kv".to_owned(),
        automation_auth_mount: "approle".to_owned(),
        trust_bundle: PathBuf::from("ca.pem"),
        expected_trust_sha256: "a".repeat(64),
    };
    assert_eq!(external.mode(), ProviderMode::External);
    Ok(())
}

#[test]
fn provider_descriptor_requires_only_declared_mode_operations() -> Result<(), String> {
    let descriptor = ProviderDescriptor {
        driver: ProviderDriverId::new("future")?,
        display_name: "Future".to_owned(),
        modes: vec![ProviderModeDescriptor {
            mode: ProviderMode::External,
            scopes: vec![ProviderScope::Project],
            operations: vec![ProviderOperation::Status],
            configuration: Vec::new(),
        }],
    };
    assert!(descriptor.mode(ProviderMode::External).is_some());
    assert!(descriptor.mode(ProviderMode::Managed).is_none());
    assert!(
        descriptor
            .require(ProviderMode::External, ProviderOperation::Status)
            .is_ok()
    );
    assert!(
        descriptor
            .require(ProviderMode::External, ProviderOperation::Remove)
            .is_err()
    );
    assert!(
        descriptor
            .require(ProviderMode::Managed, ProviderOperation::Status)
            .is_err()
    );
    Ok(())
}
