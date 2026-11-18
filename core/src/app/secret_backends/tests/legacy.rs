use super::*;

#[tokio::test]
async fn future_driver_is_descriptor_driven_and_unsupported_add_has_no_effect()
-> Result<(), Box<dyn std::error::Error>> {
    let fake = Arc::new(FakeRemote {
        calls: AtomicUsize::new(0),
        succeed: false,
    });
    let driver = ProviderDriverId::new("future")?;
    let name = ProviderInstanceName::new("team-secrets")?;
    let manager = SecretBackendManager::new(
        vec![ProviderDriverRegistration {
            descriptor: ProviderDescriptor {
                driver: driver.clone(),
                display_name: "Future Safe".to_owned(),
                modes: vec![ProviderModeDescriptor {
                    mode: ProviderMode::External,
                    scopes: vec![ProviderScope::Project],
                    operations: vec![ProviderOperation::Edit, ProviderOperation::Test],
                    configuration: Vec::new(),
                }],
            },
            remote: Some(fake.clone()),
            managed: None,
        }],
        [(
            name.clone(),
            ProviderInstanceConfig {
                driver,
                scope: ProviderScope::Project,
                configuration: crate::domain::secret_backend::ProviderConfiguration::External {
                    address: "https://future.example.test".to_owned(),
                    namespace: None,
                    storage_mount: "secrets".to_owned(),
                    automation_auth_mount: "automation".to_owned(),
                    trust_bundle: "ca.pem".into(),
                    expected_trust_sha256: format!("sha256:{}", "a".repeat(64)),
                },
            },
        )]
        .into_iter()
        .collect(),
    )?;
    assert_eq!(manager.drivers()[0].display_name, "Future Safe");
    assert_eq!(manager.instance(&name)?.name, name);

    let request = RemoteSecretBackendSetupRequest {
        instance: name,
        operation: ProviderOperation::Add,
        address: "https://future.example.test".to_owned(),
        namespace: None,
        storage_mount: "secrets".to_owned(),
        automation_auth_mount: "automation".to_owned(),
        trust_bundle: "ca.pem".into(),
        expected_trust_sha256: format!("sha256:{}", "a".repeat(64)),
        project_id: crate::domain::workspace::ProjectId(uuid::Uuid::nil()),
        environment: "test".to_owned(),
        locations: vec!["database/password".to_owned()],
        allow_bootstrap: false,
        target: "local".to_owned(),
        config_hash: "b".repeat(64),
    };
    let public_plan = plan_remote_secret_backend_setup(request.clone())?;
    assert!(!serde_json::to_string(&public_plan)?.contains("secret-sentinel"));
    assert!(!format!("{public_plan:?}").contains("secret-sentinel"));
    let error = manager
        .plan_remote_setup(request.clone())
        .err()
        .ok_or("unsupported add planned")?;
    assert!(error.to_string().contains("does not support add"));
    let mut edit_request = request.clone();
    edit_request.operation = ProviderOperation::Edit;
    assert_eq!(
        manager.plan_remote_setup(edit_request)?.request.operation,
        ProviderOperation::Edit
    );
    assert!(
        manager
            .apply_remote_setup(
                request,
                "unused",
                SecretMaterial::new("secret-sentinel".to_owned())?
            )
            .await
            .is_err()
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    assert!(!format!("{manager:?}").contains("secret-sentinel"));
    Ok(())
}
