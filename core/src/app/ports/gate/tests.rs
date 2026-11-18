use super::ProvisionedCapSite;
use crate::domain::secret::SecretMaterial;

#[test]
fn provisioned_site_debug_redacts_the_secret() -> Result<(), String> {
    let site = ProvisionedCapSite {
        site_key: "public-key".to_owned(),
        site_secret: SecretMaterial::new("private-secret".to_owned())?,
    };
    let debug = format!("{site:?}");
    assert!(debug.contains("public-key"));
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("private-secret"));
    Ok(())
}
