use std::io::Write;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::app::ports::{DownloadedFile, ProgressReporter, ReleaseFiles, ReleaseManifestClient};
use crate::domain::error::MmError;
use crate::domain::progress::ProgressEvent;
use crate::domain::release::{
    CHECKSUMS_FILE, MANIFEST_FILE, RELEASE_MANIFEST_URL, ReleaseDownload, ReleaseManifest,
};

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

const MAX_RELEASE_TEXT_BYTES: usize = 1024 * 1024;
const PROGRESS_EVERY_BYTES: u64 = 1024 * 1024;

/// The files of the published releases: GitHub's, or a loopback test origin's.
#[derive(Debug, Clone)]
pub struct ReqwestReleaseClient {
    client: reqwest::Client,
    origin: String,
}

impl ReqwestReleaseClient {
    pub fn new(origin: &str) -> Result<Self, String> {
        let origin = origin.trim_end_matches('/').to_owned();
        let client = reqwest::Client::builder()
            .https_only(!origin.starts_with("http://127.0.0.1:"))
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .user_agent(concat!("mm-update/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| format!("could not initialize the release client: {error}"))?;
        Ok(Self { client, origin })
    }

    fn url(&self, tag: Option<&str>, file: &str) -> String {
        match tag {
            Some(tag) => format!("{}/releases/download/{tag}/{file}", self.origin),
            None => format!("{}/releases/latest/download/{file}", self.origin),
        }
    }

    async fn text(&self, url: &str) -> Result<Vec<u8>, String> {
        let mut response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(30))
            .send()
            .await
            .map_err(|error| format!("{url} is unavailable: {error}"))?;
        if !response.status().is_success() {
            return Err(format!("{url} returned HTTP {}", response.status()));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("could not read {url}: {error}"))?
        {
            body.extend_from_slice(&chunk);
            if body.len() > MAX_RELEASE_TEXT_BYTES {
                return Err(format!("{url} is larger than any release file mm reads"));
            }
        }
        Ok(body)
    }
}

#[async_trait]
impl ReleaseManifestClient for ReqwestReleaseClient {
    async fn fetch_stable(&self) -> Result<ReleaseManifest, String> {
        self.manifest(None).await
    }
}

#[async_trait]
impl ReleaseFiles for ReqwestReleaseClient {
    async fn manifest(&self, tag: Option<&str>) -> Result<ReleaseManifest, String> {
        let body = self.text(&self.url(tag, MANIFEST_FILE)).await?;
        ReleaseManifest::parse(&body)
    }

    async fn checksums(&self, tag: &str) -> Result<String, String> {
        let url = self.url(Some(tag), CHECKSUMS_FILE);
        String::from_utf8(self.text(&url).await?).map_err(|_| format!("{url} is not text"))
    }

    async fn download(
        &self,
        download: &ReleaseDownload,
        destination: &Path,
        limit: u64,
        (progress, step): (&dyn ProgressReporter, &str),
    ) -> Result<DownloadedFile, MmError> {
        let url = self.url(Some(&download.tag), &download.file);
        let failed = |problem: String| MmError::Update {
            message: format!("could not download {url}: {problem}; nothing was changed"),
        };
        let io = |source| MmError::Io {
            context: format!("writing {}", destination.display()),
            source,
        };
        let mut response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|error| failed(error.to_string()))?;
        if !response.status().is_success() {
            return Err(failed(format!("HTTP {}", response.status())));
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(io)?;
        let mut hasher = Sha256::new();
        let (mut size, mut reported) = (0_u64, 0_u64);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| failed(error.to_string()))?
        {
            let room =
                usize::try_from(limit.saturating_add(1).saturating_sub(size)).unwrap_or(usize::MAX);
            let kept = &chunk[..chunk.len().min(room)];
            hasher.update(kept);
            file.write_all(kept).map_err(io)?;
            size = size.saturating_add(u64::try_from(kept.len()).unwrap_or(u64::MAX));
            if size > limit {
                break;
            }
            if size - reported >= PROGRESS_EVERY_BYTES {
                progress.report(ProgressEvent::bytes(step, size, download.size));
                reported = size;
            }
        }
        progress.report(ProgressEvent::bytes(step, size, download.size));
        file.sync_all().map_err(io)?;
        Ok(DownloadedFile {
            size,
            sha256: hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        })
    }
}
