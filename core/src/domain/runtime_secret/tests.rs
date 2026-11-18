use super::*;

fn consumer() -> RuntimeSecretConsumer {
    RuntimeSecretConsumer {
        stack: "web".to_owned(),
        service: "api".to_owned(),
        target: "token".to_owned(),
        uid: "1000".to_owned(),
        gid: "1000".to_owned(),
        mode: 0o400,
    }
}

fn item(name: &str) -> Result<RuntimeSecretPlanItem, String> {
    Ok(RuntimeSecretPlanItem {
        logical_name: name.to_owned(),
        backend: "age".to_owned(),
        delivery: DeliveryDriverId::new("runtime-native")?,
        source_revision: "revision-1".to_owned(),
        previous_source_revision: None,
        consumers: vec![consumer(), consumer()],
    })
}

#[test]
fn consumers_enforce_safe_names_numeric_ownership_and_unix_modes() {
    assert!(consumer().validate().is_ok());
    let mutations: [fn(&mut RuntimeSecretConsumer); 8] = [
        |value| value.stack.clear(),
        |value| value.service = "not/a/service".to_owned(),
        |value| value.target = "../token".to_owned(),
        |value| value.uid = "user".to_owned(),
        |value| value.uid = "1".repeat(11),
        |value| value.gid.clear(),
        |value| value.mode = 0,
        |value| value.mode = 0o1000,
    ];
    for mutate in mutations {
        let mut candidate = consumer();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
}

#[test]
fn plans_are_sorted_deduplicated_canonical_and_owner_bound() -> Result<(), String> {
    let plan = RuntimeSecretPlan::new(
        "project".to_owned(),
        "target".to_owned(),
        "production".to_owned(),
        RuntimeKind::DockerCompose,
        vec![item("z-token")?, item("a-token")?],
    )?;
    assert_eq!(
        plan.items
            .iter()
            .map(|item| item.logical_name.as_str())
            .collect::<Vec<_>>(),
        ["a-token", "z-token"]
    );
    assert!(plan.items.iter().all(|item| item.consumers.len() == 1));
    assert_eq!(plan.canonical_hash()?, plan.plan_hash);

    let mut reordered = plan.clone();
    reordered.items.reverse();
    assert_eq!(reordered.canonical_hash()?, plan.plan_hash);
    reordered.schema = "unknown".to_owned();
    assert!(reordered.canonical_hash().is_err());

    assert!(
        RuntimeSecretPlan::new(
            String::new(),
            "target".to_owned(),
            "prod".to_owned(),
            RuntimeKind::DockerCompose,
            Vec::new()
        )
        .is_err()
    );
    assert!(
        RuntimeSecretPlan::new(
            "project".to_owned(),
            "target\n".to_owned(),
            "prod".to_owned(),
            RuntimeKind::DockerCompose,
            Vec::new()
        )
        .is_err()
    );
    assert!(
        RuntimeSecretPlan::new(
            "project".to_owned(),
            "target".to_owned(),
            "bad env".to_owned(),
            RuntimeKind::DockerCompose,
            Vec::new()
        )
        .is_err()
    );
    assert!(
        RuntimeSecretPlan::new(
            "project".to_owned(),
            "target".to_owned(),
            "prod".to_owned(),
            RuntimeKind::DockerCompose,
            vec![item("same")?, item("same")?]
        )
        .is_err()
    );

    for mutate in [
        |value: &mut RuntimeSecretPlanItem| value.logical_name = "/bad".to_owned(),
        |value: &mut RuntimeSecretPlanItem| value.backend.clear(),
        |value: &mut RuntimeSecretPlanItem| value.source_revision = "x".repeat(1025),
        |value: &mut RuntimeSecretPlanItem| value.source_revision = "bad\nrevision".to_owned(),
        |value: &mut RuntimeSecretPlanItem| value.consumers.clear(),
    ] {
        let mut candidate = item("token")?;
        mutate(&mut candidate);
        assert!(
            RuntimeSecretPlan::new(
                "project".to_owned(),
                "target".to_owned(),
                "prod".to_owned(),
                RuntimeKind::DockerCompose,
                vec![candidate]
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn runtime_material_debug_output_never_contains_plaintext() -> Result<(), String> {
    let material = RuntimeSecretMaterial {
        logical_name: "token".to_owned(),
        material: SecretMaterial::new("highly-sensitive".to_owned())?,
    };
    let debug = format!("{material:?}");
    assert_eq!(
        debug,
        "RuntimeSecretMaterial { logical_name: \"token\", material: \"[REDACTED]\" }"
    );
    assert!(!debug.contains("highly-sensitive"));
    Ok(())
}
