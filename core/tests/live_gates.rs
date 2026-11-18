use std::path::PathBuf;

use mm::adapters::gate::ReqwestCapSiteProvisioner;
use mm::app::ports::CapSiteProvisioner;
use mm::domain::service_stack::CAP_HOST;

#[tokio::test]
#[ignore = "requires MM_LIVE_CAP_ENDPOINT and a running pinned Cap/Valkey stack"]
async fn live_cap_administration_contract_provisions_and_removes_a_site()
-> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("MM_LIVE_CAP_ENDPOINT")?;
    let key_file = PathBuf::from(std::env::var("MM_LIVE_CAP_ADMIN_KEY_FILE")?);
    let provisioner = ReqwestCapSiteProvisioner::new(endpoint, CAP_HOST, key_file)?;

    let site = provisioner.create("mm-live-gate-conformance").await?;
    assert!(!site.site_key.is_empty());
    assert!(site.site_secret.expose(|secret| !secret.is_empty()));
    provisioner.remove(&site.site_key).await?;
    Ok(())
}
