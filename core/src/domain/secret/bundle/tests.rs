use super::*;

const PUBLIC_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 mm-deploy-test";

fn project() -> ProjectId {
    ProjectId(uuid::Uuid::from_u128(
        0x019c_527b_6242_7b43_bfd4_bd60_e27c_1de3,
    ))
}

fn secret(locator: &str, revision: &str, value: &str) -> Result<BundledSecret, String> {
    Ok(BundledSecret {
        reference: format!("age://{locator}").parse()?,
        revision: revision.to_owned(),
        material: SecretMaterial::new(value.to_owned())?,
    })
}

fn bundle() -> Result<SecretBundle, String> {
    Ok(SecretBundle {
        project_id: project(),
        environment: "integration".to_owned(),
        secrets: vec![
            secret("database-password", "rev-1", "hunter2-value")?,
            secret("api/token", "rev-2", "token-value")?,
        ],
        deploy_key: Some(BundledDeployKey {
            private_key: Zeroizing::new(b"raw openssh key bytes\n".to_vec()),
            public_key: PUBLIC_KEY.to_owned(),
        }),
    })
}

#[test]
fn bundles_round_trip_through_versioned_json() -> Result<(), String> {
    let original = bundle()?;
    let plaintext = original.encode()?;
    let json: serde_json::Value =
        serde_json::from_slice(&plaintext).map_err(|error| error.to_string())?;
    assert_eq!(json["version"], 1);
    assert_eq!(json["environment"], "integration");
    assert_eq!(json["secrets"][1]["reference"], "age://api/token");
    assert!(json["deploy-key"]["private-key"].is_string());

    let decoded = SecretBundle::decode(&plaintext)?;
    assert_eq!(decoded.project_id, project());
    assert_eq!(decoded.environment, "integration");
    assert_eq!(decoded.secrets.len(), 2);
    assert_eq!(decoded.secrets[0].reference.locator(), "database-password");
    assert_eq!(decoded.secrets[0].revision, "rev-1");
    decoded.secrets[0]
        .material
        .expose(|value| assert_eq!(value, "hunter2-value"));
    let key = decoded.deploy_key.ok_or("deploy key missing")?;
    assert_eq!(key.private_key.as_slice(), b"raw openssh key bytes\n");
    assert_eq!(key.public_key, PUBLIC_KEY);
    Ok(())
}

#[test]
fn debug_output_and_decode_errors_never_show_values() -> Result<(), String> {
    let original = bundle()?;
    let debug = format!("{original:?}");
    assert!(!debug.contains("hunter2-value"));
    assert!(!debug.contains("raw openssh key"));
    assert!(debug.contains("[REDACTED]"));

    let mut json: serde_json::Value =
        serde_json::from_slice(&original.encode()?).map_err(|error| error.to_string())?;
    json["secrets"][0]["value"] = serde_json::json!(["hunter2-value"]);
    let bytes = serde_json::to_vec(&json).map_err(|error| error.to_string())?;
    let error = SecretBundle::decode(&bytes)
        .err()
        .ok_or("decode accepted")?;
    assert!(!error.contains("hunter2"), "{error}");
    Ok(())
}

#[test]
fn rejects_wrong_scope_versions_and_invalid_entries() -> Result<(), String> {
    let original = bundle()?;
    original.ensure_scope(project(), "integration")?;
    assert!(original.ensure_scope(project(), "production").is_err());
    assert!(
        original
            .ensure_scope(ProjectId(uuid::Uuid::nil()), "integration")
            .is_err()
    );

    let mut json: serde_json::Value =
        serde_json::from_slice(&original.encode()?).map_err(|error| error.to_string())?;
    json["version"] = serde_json::json!(2);
    let bytes = serde_json::to_vec(&json).map_err(|error| error.to_string())?;
    assert!(SecretBundle::decode(&bytes).is_err_and(|error| error.contains("version 2")));

    let mut duplicate = bundle()?;
    duplicate
        .secrets
        .push(secret("database-password", "rev-3", "other")?);
    assert!(
        duplicate
            .validate()
            .is_err_and(|error| error.contains("twice"))
    );

    let mut control = bundle()?;
    control.secrets = vec![secret("multi", "rev-1", "line\nbreak")?];
    assert!(control.encode().is_err());

    let mut revision = bundle()?;
    revision.secrets = vec![secret("name", "bad revision", "value")?];
    assert!(revision.validate().is_err());

    let mut vault = bundle()?;
    vault.secrets = vec![BundledSecret {
        reference: "vault://name".parse()?,
        revision: "rev-1".to_owned(),
        material: SecretMaterial::new("value".to_owned())?,
    }];
    assert!(vault.validate().is_err());

    let mut environment = bundle()?;
    environment.environment = "../prod".to_owned();
    assert!(environment.validate().is_err());

    let mut key = bundle()?;
    key.deploy_key = Some(BundledDeployKey {
        private_key: Zeroizing::new(b"key".to_vec()),
        public_key: "ssh-rsa AAAAB3NzaC1yc2E".to_owned(),
    });
    assert!(key.validate().is_err());
    Ok(())
}
