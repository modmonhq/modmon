//! The project's configuration as a remote host's `mmd` reads it: with what
//! the host's policy says about OCI runtimes, so every plan made from it
//! keeps to that policy.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::ports::{ProjectDocumentStore, ProjectInitialization};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::hardening::HostRuntimes;

/// A document store that reads every configuration under one host's
/// runtime policy.
pub struct HostDocuments {
    inner: Arc<dyn ProjectDocumentStore>,
    runtimes: HostRuntimes,
}

impl HostDocuments {
    #[must_use]
    pub fn new(inner: Arc<dyn ProjectDocumentStore>, runtimes: HostRuntimes) -> Self {
        Self { inner, runtimes }
    }

    fn on_host(&self, mut document: ConfigDocument) -> ConfigDocument {
        document.config.host_runtimes = Some(self.runtimes.clone());
        document
    }
}

impl ProjectDocumentStore for HostDocuments {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        self.inner.discover(start)
    }

    fn load_from(&self, start: &Path) -> Result<ConfigDocument, MmError> {
        self.inner
            .load_from(start)
            .map(|document| self.on_host(document))
    }

    fn load(&self, path: &Path) -> Result<ConfigDocument, MmError> {
        self.inner.load(path).map(|document| self.on_host(document))
    }

    fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<Box<dyn ProjectInitialization>, MmError> {
        self.inner.initialize(root, requested_name)
    }

    fn schema(&self) -> schemars::Schema {
        self.inner.schema()
    }

    fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<ConfigDocument, MmError> {
        self.inner.set_default_environment(start, environment)
    }

    fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: crate::domain::environment_delivery::EnvironmentDeliveryMode,
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_environment_delivery(start, environment, mode)
    }

    fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_development_mount(start, module, mount, expected_source_hash)
    }

    fn set_development_enabled(
        &self,
        start: &Path,
        enabled: &[String],
    ) -> Result<ConfigDocument, MmError> {
        self.inner
            .set_development_enabled(start, enabled)
            .map(|document| self.on_host(document))
    }

    fn ignored_sources(&self, document: &ConfigDocument) -> Vec<PathBuf> {
        self.inner.ignored_sources(document)
    }
}
