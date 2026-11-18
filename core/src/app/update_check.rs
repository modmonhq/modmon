use std::sync::Arc;

use crate::app::ports::ReleaseManifestClient;
use crate::domain::release::{
    UpdateCheckReport, UpdateStatus, compare_installed, installed_target,
};

#[derive(Clone)]
pub struct UpdateCheckService {
    client: Arc<dyn ReleaseManifestClient>,
}

impl UpdateCheckService {
    #[must_use]
    pub fn new(client: Arc<dyn ReleaseManifestClient>) -> Self {
        Self { client }
    }

    pub async fn check(&self, current: &str) -> UpdateCheckReport {
        let target = installed_target().map(str::to_owned);
        match self.client.fetch_stable().await {
            Ok(manifest) => compare_installed(current, target.as_deref(), &manifest)
                .unwrap_or_else(|message| UpdateCheckReport {
                    status: UpdateStatus::Unavailable,
                    current: current.to_owned(),
                    latest: Some(manifest.version),
                    target,
                    message,
                }),
            Err(message) => UpdateCheckReport {
                status: UpdateStatus::Unavailable,
                current: current.to_owned(),
                latest: None,
                target,
                message,
            },
        }
    }
}

#[cfg(test)]
mod tests;
