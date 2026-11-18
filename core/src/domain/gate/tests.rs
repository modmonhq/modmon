use super::*;

fn driver(value: &str) -> GateDriverId {
    GateDriverId::new(value).unwrap_or_else(|_| GateDriverId(value.to_owned()))
}
fn principal(drivers: Vec<GateDriverId>) -> GatePrincipal {
    GatePrincipal {
        user_id: Uuid::nil(),
        normalized_username: "user".to_owned(),
        display_username: "User".to_owned(),
        authenticated_drivers: drivers,
    }
}

#[test]
fn any_and_all_are_driver_agnostic_truth_tables() {
    let password = driver(PASSWORD_CAP_DRIVER);
    let fake = driver("fake-second");
    let configured = vec![fake.clone(), password.clone()];
    let challenge = GateDriverResult::Challenge(GateChallenge {
        driver: fake.clone(),
        kind: GateChallengeKind::Header {
            name: "x-fake".to_owned(),
        },
    });
    let cases = [
        (
            GateDriverResult::Authenticated(principal(vec![password.clone()])),
            GateDriverResult::Authenticated(principal(vec![fake.clone()])),
            true,
            true,
        ),
        (
            GateDriverResult::Authenticated(principal(vec![password.clone()])),
            GateDriverResult::Denied,
            true,
            false,
        ),
        (
            GateDriverResult::Denied,
            GateDriverResult::Denied,
            false,
            false,
        ),
        (
            GateDriverResult::Unavailable,
            GateDriverResult::Denied,
            false,
            false,
        ),
        (challenge, GateDriverResult::Denied, false, false),
    ];
    for (left, right, any, all) in cases {
        let results = BTreeMap::from([(fake.clone(), left), (password.clone(), right)]);
        assert_eq!(
            matches!(
                evaluate_gate_policy(GatePolicyMode::Any, &configured, &results),
                GateEvaluation::Authorized(_)
            ),
            any
        );
        assert_eq!(
            matches!(
                evaluate_gate_policy(GatePolicyMode::All, &configured, &results),
                GateEvaluation::Authorized(_)
            ),
            all
        );
    }
}

#[test]
fn usernames_are_canonical_and_plan_hashes_reject_changes() -> Result<(), String> {
    assert_eq!(normalize_username("  Alice.Example ")?, "alice.example");
    assert!(normalize_username("álîce").is_err());
    let plan = GateMutationPlan {
        schema: 1,
        action: GateMutationAction::AddUser,
        policy: "internal".to_owned(),
        project_id: ProjectId(Uuid::nil()),
        environment: "local".to_owned(),
        subject: "alice".to_owned(),
        policy_revision: Some(1),
        subject_revision: None,
        ingress_generation: None,
        requires_hidden_input: true,
        effects: vec!["create application principal".to_owned()],
        plan_hash: String::new(),
    }
    .seal()?;
    plan.validate()?;
    let mut invalid_schema = plan.clone();
    invalid_schema.schema = 2;
    assert_eq!(
        invalid_schema.validate(),
        Err("gate mutation plan schema or hash is invalid".to_owned())
    );
    let mut invalid_hash = plan.clone();
    invalid_hash.plan_hash.clear();
    assert_eq!(
        invalid_hash.validate(),
        Err("gate mutation plan schema or hash is invalid".to_owned())
    );
    let mut changed = plan;
    changed.subject = "mallory".to_owned();
    assert!(changed.validate().is_err());
    Ok(())
}

#[test]
fn policy_identity_validation_evaluation_and_redaction_contracts_are_complete() -> Result<(), String>
{
    let policy_id = Uuid::from_u128(1);
    assert_eq!(
        gate_session_cookie_name(policy_id),
        "__Host-mm_gate_00000000000000000000000000000001"
    );
    assert_eq!(
        gate_csrf_cookie_name(policy_id),
        "__Host-mm_gate_csrf_00000000000000000000000000000001"
    );
    let driver = GateDriverId::new(PASSWORD_CAP_DRIVER)?;
    assert_eq!(driver.as_str(), PASSWORD_CAP_DRIVER);
    let name = GatePolicyName::new("internal")?;
    assert_eq!(name.as_str(), "internal");
    assert_eq!(name.to_string(), "internal");
    for invalid in ["", "Upper", "-start", "end-", &"x".repeat(64)] {
        assert!(GateDriverId::new(invalid).is_err());
        assert!(GatePolicyName::new(invalid).is_err());
    }

    let reference = SecretReference::new(
        crate::domain::secret_contract::StorageProviderId::new("age".to_owned())?,
        "gate/internal",
    )?;
    let policy = GatePolicy {
        id: policy_id,
        name,
        project_id: ProjectId(Uuid::from_u128(2)),
        environment: "staging".to_owned(),
        mode: GatePolicyMode::All,
        drivers: vec![driver.clone()],
        session_ttl_seconds: DEFAULT_SESSION_TTL_SECONDS,
        secret_provider: "age".to_owned(),
        password_pepper: reference.clone(),
        session_keys: VersionedSecretReferences {
            current_version: 1,
            current: reference.clone(),
            previous: None,
        },
        cap_site_key: "site-key".to_owned(),
        cap_site_secret: reference,
        revision: 1,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    policy.validate()?;
    let middleware = policy.forward_auth_middleware_name(TargetId(Uuid::from_u128(3)));
    assert!(middleware.starts_with("mm-gate-internal-"));
    assert_eq!(middleware.len(), "mm-gate-internal-".len() + 16);

    let mutations: [fn(&mut GatePolicy); 7] = [
        |value| value.drivers.clear(),
        |value| value.drivers.push(value.drivers[0].clone()),
        |value| value.session_ttl_seconds = 59,
        |value| value.revision = 0,
        |value| value.session_keys.current_version = 0,
        |value| value.cap_site_key.clear(),
        |value| value.cap_site_key = "x\n".to_owned(),
    ];
    for mutate in mutations {
        let mut candidate = policy.clone();
        mutate(&mut candidate);
        assert!(candidate.validate().is_err());
    }
    let mut oversized_site_key = policy.clone();
    oversized_site_key.cap_site_key = "x".repeat(129);
    assert!(oversized_site_key.validate().is_err());

    let second = GateDriverId::new("second")?;
    let configured = [driver.clone(), second.clone()];
    let first_principal = principal(vec![driver.clone()]);
    let mut second_principal = principal(vec![second.clone()]);
    second_principal.user_id = Uuid::from_u128(99);
    let mismatched = BTreeMap::from([
        (
            driver.clone(),
            GateDriverResult::Authenticated(first_principal),
        ),
        (
            second.clone(),
            GateDriverResult::Authenticated(second_principal),
        ),
    ]);
    assert_eq!(
        evaluate_gate_policy(GatePolicyMode::All, &configured, &mismatched),
        GateEvaluation::Denied
    );
    assert_eq!(
        evaluate_gate_policy(GatePolicyMode::All, &configured, &BTreeMap::new()),
        GateEvaluation::Unavailable
    );
    assert_eq!(
        evaluate_gate_policy(GatePolicyMode::All, &[], &BTreeMap::new()),
        GateEvaluation::Denied
    );
    let challenged = BTreeMap::from([
        (
            driver.clone(),
            GateDriverResult::Challenge(GateChallenge {
                driver: driver.clone(),
                kind: GateChallengeKind::Redirect {
                    location: "/login".to_owned(),
                },
            }),
        ),
        (
            second.clone(),
            GateDriverResult::Authenticated(principal(vec![driver.clone()])),
        ),
    ]);
    assert!(matches!(
        evaluate_gate_policy(GatePolicyMode::All, &configured, &challenged),
        GateEvaluation::Challenge(_)
    ));

    let hash = PasswordHashRecord {
        encoded: "password-sentinel".to_owned(),
        parameter_version: 1,
    };
    assert!(!format!("{hash:?}").contains("password-sentinel"));
    let claims = GateSessionClaims {
        session_id: Uuid::from_u128(4),
        policy_id,
        user_id: Uuid::from_u128(5),
        password_version: 1,
        authenticated_drivers: vec![driver],
        issued_at_unix_ms: 1,
        expires_at_unix_ms: 2,
        csrf_binding: "csrf-sentinel".to_owned(),
    };
    assert!(!format!("{claims:?}").contains("csrf-sentinel"));

    assert_eq!(
        [
            GateMutationAction::AddPolicy,
            GateMutationAction::RemovePolicy,
            GateMutationAction::AttachRoute,
            GateMutationAction::DetachRoute,
            GateMutationAction::AddUser,
            GateMutationAction::DisableUser,
            GateMutationAction::EnableUser,
            GateMutationAction::ResetPassword,
            GateMutationAction::RemoveUser,
            GateMutationAction::RevokeSession,
            GateMutationAction::RotateSessionKey,
        ]
        .map(GateMutationAction::as_str),
        [
            "add_policy",
            "remove_policy",
            "attach_route",
            "detach_route",
            "add_user",
            "disable_user",
            "enable_user",
            "reset_password",
            "remove_user",
            "revoke_session",
            "rotate_session_key",
        ]
    );
    assert!(normalize_username("  ").is_err());
    assert!(normalize_username("bad\nname").is_err());
    Ok(())
}
