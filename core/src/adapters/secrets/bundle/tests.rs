use super::*;
use crate::domain::secret::{BundledDeployKey, BundledSecret, SecretMaterial};
use crate::domain::workspace::ProjectId;

const PUBLIC_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJZYcbK/sLQnEppB74bVUmUQnMUahuREnHUmhtFYGcf7 mm-deploy-test";

fn bundle() -> Result<SecretBundle, Box<dyn std::error::Error>> {
    Ok(SecretBundle {
        project_id: ProjectId(uuid::Uuid::from_u128(7)),
        environment: "integration".to_owned(),
        secrets: vec![BundledSecret {
            reference: "age://database-password".parse()?,
            revision: "0b5c1d4e-rev".to_owned(),
            material: SecretMaterial::new("sealed-value".to_owned())?,
            set_at_unix_ms: None,
            set_by: None,
        }],
        deploy_key: Some(BundledDeployKey {
            private_key: Zeroizing::new(b"private key bytes\n".to_vec()),
            public_key: PUBLIC_KEY.to_owned(),
        }),
    })
}

#[test]
fn sealed_bundles_open_only_with_the_recipient_identity() -> Result<(), Box<dyn std::error::Error>>
{
    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public().to_string();
    let sealed = seal_secret_bundle(&recipient, &bundle()?)?;
    let text = std::str::from_utf8(&sealed)?;
    assert!(text.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
    assert!(!text.contains("sealed-value"));

    // A text transport may add a trailing newline.
    let mut with_newline = sealed.clone();
    with_newline.extend_from_slice(b"\n");
    let opened = open_secret_bundle(&identity, &with_newline)?;
    assert_eq!(opened.secrets[0].revision, "0b5c1d4e-rev");
    opened.secrets[0]
        .material
        .expose(|value| assert_eq!(value, "sealed-value"));
    assert_eq!(
        opened
            .deploy_key
            .as_ref()
            .map(|key| key.private_key.as_slice()),
        Some(b"private key bytes\n".as_slice())
    );

    let other = age::x25519::Identity::generate();
    let error = open_secret_bundle(&other, &sealed)
        .err()
        .ok_or("another identity opened the bundle")?;
    assert!(!error.to_string().contains("sealed-value"));

    // Binary age input opens too.
    let plaintext = bundle()?.encode()?;
    let binary = age::encrypt(&identity.to_public(), &plaintext)?;
    assert_eq!(open_secret_bundle(&identity, &binary)?.secrets.len(), 1);
    Ok(())
}

#[test]
fn rejects_bad_recipients_and_oversized_input() -> Result<(), Box<dyn std::error::Error>> {
    assert!(seal_secret_bundle("age1notarecipient", &bundle()?).is_err());
    let identity = age::x25519::Identity::generate();
    let oversized = vec![b'a'; MAX_SEALED_SECRET_BUNDLE_BYTES + 1];
    assert!(open_secret_bundle(&identity, &oversized).is_err());
    assert!(open_secret_bundle(&identity, b"not age").is_err());
    Ok(())
}
