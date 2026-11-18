use async_trait::async_trait;

use crate::domain::release::ReleaseManifest;

#[async_trait]
pub trait ReleaseManifestClient: Send + Sync {
    async fn fetch_stable(&self) -> Result<ReleaseManifest, String>;
}
