use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;

#[test]
fn external_setup_is_public_state_then_emits_a_typed_plan_request()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = ProviderModel::new(context()?);
    assert_eq!(update(&mut model, key('x')), Some(ProviderAction::None));
    let ProviderScreen::ExternalSetup(form) = &mut model.screen else {
        return Err(std::io::Error::other("external setup form was not opened").into());
    };
    form.values.insert(
        "address".to_owned(),
        "https://provider.example.test:8200".to_owned(),
    );
    form.values
        .insert("trust-bundle".to_owned(), "ca.pem".to_owned());
    form.values.insert(
        "expected-trust-sha256".to_owned(),
        format!("sha256:{}", "a".repeat(64)),
    );
    form.field = form.fields.len();
    let Some(ProviderAction::PlanExternal(request)) = update(
        &mut model,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    ) else {
        return Err(std::io::Error::other("form did not emit a typed plan request").into());
    };
    assert_eq!(request.instance.as_str(), "team-secrets");
    assert_eq!(request.locations, vec!["database/password"]);
    assert_eq!(request.trust_bundle, PathBuf::from("/project/ca.pem"));
    let headless_request = request.clone();
    let tui_plan = crate::app::plan_remote_secret_backend_setup(request)?;
    let headless_plan = crate::app::plan_remote_secret_backend_setup(headless_request)?;
    assert_eq!(tui_plan.plan_hash, headless_plan.plan_hash);
    assert!(!format!("{model:?}").contains("developer-token-sentinel"));
    Ok(())
}

#[test]
fn review_cancel_resume_error_retry_and_resize_preserve_exact_review()
-> Result<(), Box<dyn std::error::Error>> {
    let mut model = ProviderModel::new(context()?);
    let review = ProviderReview::Rotation(DirectSecretRotationPlan {
        project_name: "project".to_owned(),
        environment: "integration".to_owned(),
        name: "database.password".to_owned(),
        provider: "age".to_owned(),
        current_revision: "revision".to_owned(),
        requires_hidden_input: true,
        plan_hash: "reviewed-plan".to_owned(),
    });
    model.last_review = Some(review.clone());
    model.screen = ProviderScreen::Review(Box::new(review));
    assert_eq!(
        update(&mut model, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        Some(ProviderAction::None)
    );
    assert_eq!(model.screen, ProviderScreen::Cancelled);
    assert_eq!(
        update(&mut model, key('r')),
        Some(ProviderAction::RetryApply)
    );
    model.screen = ProviderScreen::Error("injected failure".to_owned());
    assert_eq!(
        update(&mut model, key('r')),
        Some(ProviderAction::RetryApply)
    );
    assert_eq!(
        model.last_review.as_ref().map(ProviderReview::plan_hash),
        Some("reviewed-plan")
    );
    let review = model.last_review.clone().ok_or_else(|| {
        std::io::Error::other("review unexpectedly missing before apply transition")
    })?;
    model.screen = ProviderScreen::Review(Box::new(review));
    assert_eq!(
        update(
            &mut model,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
        ),
        Some(ProviderAction::ApplyReview)
    );
    assert!(matches!(model.screen, ProviderScreen::Applying(_)));
    Ok(())
}

#[test]
fn future_driver_form_fields_come_from_its_descriptor() -> Result<(), Box<dyn std::error::Error>> {
    let mut context = context()?;
    let future = crate::domain::secret_backend::ProviderDriverId::new("future")?;
    context.external_connections[0].instance.driver = future.clone();
    context.external_connections[0].instance.driver_display_name = "Future Safe".to_owned();
    context.drivers = vec![ProviderDescriptor {
        driver: future,
        display_name: "Future Safe".to_owned(),
        modes: vec![crate::domain::secret_backend::ProviderModeDescriptor {
            mode: ProviderMode::External,
            scopes: vec![crate::domain::secret_backend::ProviderScope::Project],
            operations: vec![ProviderOperation::Test],
            configuration: vec![ProviderConfigurationField {
                key: "address".to_owned(),
                label: "Future endpoint".to_owned(),
                kind: crate::domain::secret_backend::ProviderFieldKind::HttpsUrl,
                required: true,
                secret: false,
                default: None,
                allowed_values: Vec::new(),
            }],
        }],
    }];
    let mut model = ProviderModel::new(context);
    assert_eq!(update(&mut model, key('x')), Some(ProviderAction::None));
    let ProviderScreen::ExternalSetup(form) = &model.screen else {
        return Err("future driver form did not open".into());
    };
    assert_eq!(form.displayed_fields()[0].0, "Future endpoint");
    Ok(())
}

fn context() -> Result<SecretBackendTuiContext, Box<dyn std::error::Error>> {
    let external_name = ProviderInstanceName::new("team-secrets")?;
    let managed_name = ProviderInstanceName::new("local-secrets")?;
    let driver = crate::adapters::secrets::openbao_provider_descriptor()?;
    let external_configuration = ProviderConfiguration::External {
        address: "https://provider.example.test:8200".to_owned(),
        namespace: None,
        storage_mount: "mm-kv".to_owned(),
        automation_auth_mount: "mm-approle".to_owned(),
        trust_bundle: "ca.pem".into(),
        expected_trust_sha256: format!("sha256:{}", "a".repeat(64)),
    };
    let managed_configuration = ProviderConfiguration::Managed {
        installation: Some("local-secrets".to_owned()),
        release: "2.4".to_owned(),
        management_port: Some(18_200),
        nodes: None,
    };
    let external = ProviderInstanceDescriptor {
        name: external_name.clone(),
        driver: driver.driver.clone(),
        driver_display_name: driver.display_name.clone(),
        mode: ProviderMode::External,
        scope: crate::domain::secret_backend::ProviderScope::Project,
        operations: driver
            .mode(ProviderMode::External)
            .ok_or("external descriptor")?
            .operations
            .clone(),
        configuration: serde_json::to_value(&external_configuration)?,
    };
    let managed = ProviderInstanceDescriptor {
        name: managed_name.clone(),
        driver: driver.driver.clone(),
        driver_display_name: driver.display_name.clone(),
        mode: ProviderMode::Managed,
        scope: crate::domain::secret_backend::ProviderScope::Installation,
        operations: driver
            .mode(ProviderMode::Managed)
            .ok_or("managed descriptor")?
            .operations
            .clone(),
        configuration: serde_json::to_value(&managed_configuration)?,
    };
    Ok(SecretBackendTuiContext {
        root: PathBuf::from("/project"),
        target: "local".to_owned(),
        config_hash: "a".repeat(64),
        project_id: ProjectId(uuid::Uuid::parse_str(
            "019c527b-6242-7b43-bfd4-bd60e27c1de3",
        )?),
        environment: "integration".to_owned(),
        external_connections: vec![ConnectionScope {
            instance: external,
            configuration: external_configuration.clone(),
            name: external_name.to_string(),
            locations: vec!["database/password".to_owned()],
        }],
        managed_connections: vec![ConnectionScope {
            instance: managed,
            configuration: managed_configuration.clone(),
            name: managed_name.to_string(),
            locations: vec!["application/key".to_owned()],
        }],
        rotatable_secrets: vec!["database.password".to_owned()],
        drivers: vec![driver],
        instances: [
            (
                external_name,
                ProviderInstanceConfig {
                    driver: crate::domain::secret_backend::ProviderDriverId::new("openbao")?,
                    scope: crate::domain::secret_backend::ProviderScope::Project,
                    configuration: external_configuration,
                },
            ),
            (
                managed_name,
                ProviderInstanceConfig {
                    driver: crate::domain::secret_backend::ProviderDriverId::new("openbao")?,
                    scope: crate::domain::secret_backend::ProviderScope::Installation,
                    configuration: managed_configuration,
                },
            ),
        ]
        .into_iter()
        .collect(),
    })
}

fn key(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)
}
