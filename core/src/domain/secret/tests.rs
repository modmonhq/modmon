use super::*;

#[test]
fn references_material_and_resolution_contracts_are_bounded_and_redacted() -> Result<(), String> {
    let scope = SecretScope::new("staging");
    assert_eq!(scope.environment(), "staging");
    let provider = StorageProviderId::new("vault".to_owned())?;
    let reference = SecretReference::new(provider.clone(), "services/api")?;
    assert_eq!(reference.scheme(), "vault");
    assert_eq!(reference.provider(), &provider);
    assert_eq!(reference.locator(), "services/api");
    assert_eq!(reference.connection(), None);
    assert_eq!(reference.canonical(), "vault/services/api");
    assert_eq!(
        format!("{reference:?}"),
        "SecretReference(vault/[REDACTED])"
    );

    let connected = SecretReference::with_connection(provider, "primary-1", "services/api")?;
    assert_eq!(connected.connection(), Some("primary-1"));
    assert_eq!(connected.canonical(), "vault/primary-1/services/api");
    let parsed: SecretReference = "vault://services/api".parse()?;
    assert_eq!(parsed, reference);
    assert!("vault/services/api".parse::<SecretReference>().is_err());
    assert_eq!(
        serde_json::to_string(&reference).map_err(|error| error.to_string())?,
        "\"vault://services/api\""
    );
    assert_eq!(
        serde_json::from_str::<SecretReference>("\"vault://services/api\"")
            .map_err(|error| error.to_string())?,
        reference
    );
    let schema = schemars::schema_for!(SecretReference);
    assert_eq!(
        schema.get("pattern").and_then(serde_json::Value::as_str),
        Some("^[a-z][a-z0-9+.-]*://[^\\u0000-\\u001f\\u007f]+$")
    );

    for locator in [
        "",
        ".",
        "services/../api",
        "services/\napi",
        &"x".repeat(1025),
    ] {
        assert!(
            SecretReference::new(StorageProviderId::new("vault".to_owned())?, locator).is_err()
        );
    }
    for connection in ["", "1primary", "Primary", "primary_name", &"x".repeat(64)] {
        assert!(
            SecretReference::with_connection(
                StorageProviderId::new("vault".to_owned())?,
                connection,
                "services/api"
            )
            .is_err()
        );
    }

    assert!(SecretMaterial::new(String::new()).is_err());
    assert!(SecretMaterial::new("x".repeat(MAX_SECRET_MATERIAL_BYTES + 1)).is_err());
    let material = SecretMaterial::new("secret-sentinel".to_owned())?;
    assert!(material.expose(|value| value == "secret-sentinel"));
    assert_eq!(format!("{material:?}"), "SecretMaterial([REDACTED])");
    assert_eq!(material.into_zeroizing().as_str(), "secret-sentinel");

    let metadata = SecretMetadata {
        status: SecretStatus::Set,
        revision: Some("active".to_owned()),
        created_at_unix_ms: Some(1),
        candidate_revision: Some("candidate".to_owned()),
    };
    assert_eq!(
        metadata.expected_state(),
        SecretExpectedState {
            active_revision: Some("active".to_owned()),
            candidate_revision: Some("candidate".to_owned()),
        }
    );
    let replacement = SecretReplacement {
        scope,
        reference: reference.clone(),
        material: SecretMaterial::new("replacement-sentinel".to_owned())?,
        expected: metadata.expected_state(),
    };
    let debug = format!("{replacement:?}");
    assert!(!debug.contains("replacement-sentinel"));
    assert!(debug.contains("[REDACTED]"));
    let resolution = SecretResolution {
        metadata,
        material: Some(SecretMaterial::new("resolution-sentinel".to_owned())?),
    };
    let debug = format!("{resolution:?}");
    assert!(!debug.contains("resolution-sentinel"));
    assert!(debug.contains("[REDACTED]"));
    Ok(())
}
