use std::time::Duration;

use async_trait::async_trait;

use crate::app::ports::ReleaseManifestClient;
use crate::domain::release::{RELEASE_MANIFEST_URL, ReleaseManifest};

#[derive(Debug, Clone)]
pub struct ReqwestReleaseManifestClient {
    client: reqwest::Client,
    url: String,
}

impl ReqwestReleaseManifestClient {
    pub fn stable() -> Result<Self, String> {
        Self::new(RELEASE_MANIFEST_URL)
    }

    pub fn new(url: impl Into<String>) -> Result<Self, String> {
        let url = url.into();
        let client = reqwest::Client::builder()
            .https_only(!url.starts_with("http://127.0.0.1:"))
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("mm-update-check/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("could not initialize update client: {error}"))?;
        Ok(Self { client, url })
    }
}

#[async_trait]
impl ReleaseManifestClient for ReqwestReleaseManifestClient {
    async fn fetch_stable(&self) -> Result<ReleaseManifest, String> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|error| format!("stable release manifest is unavailable: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "stable release manifest returned HTTP {}",
                response.status()
            ));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("could not read stable release manifest: {error}"))?;
        ReleaseManifest::parse(&bytes)
    }
}
