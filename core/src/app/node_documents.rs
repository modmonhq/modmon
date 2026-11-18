//! The project's configuration as one node of a cluster target sees it.
//!
//! `mmd` on a node plans, applies, and reports through the same services as
//! a single host. What differs is the configuration they read: here every
//! service placed on other nodes is not part of the environment, so each of
//! those services renders, starts, and lists only what belongs on this node.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::app::ports::{ProjectDocumentStore, ProjectInitialization};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::placement::{NodeScope, view_from_node};

/// A document store that reads every configuration from `scope`'s node.
pub struct NodeDocuments {
    inner: Arc<dyn ProjectDocumentStore>,
    scope: NodeScope,
}

impl NodeDocuments {
    #[must_use]
    pub fn new(inner: Arc<dyn ProjectDocumentStore>, scope: NodeScope) -> Self {
        Self { inner, scope }
    }

    fn seen_from_node(&self, mut document: ConfigDocument) -> Result<ConfigDocument, MmError> {
        view_from_node(&mut document.config, &self.scope)
            .map_err(|message| MmError::Placement { message })?;
        Ok(document)
    }
}

impl ProjectDocumentStore for NodeDocuments {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError> {
        self.inner.discover(start)
    }

    fn load_from(&self, start: &Path) -> Result<ConfigDocument, MmError> {
        self.seen_from_node(self.inner.load_from(start)?)
    }

    fn load(&self, path: &Path) -> Result<ConfigDocument, MmError> {
        self.seen_from_node(self.inner.load(path)?)
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
        _start: &Path,
        _environment: Option<&str>,
    ) -> Result<ConfigDocument, MmError> {
        Err(read_only())
    }

    fn set_environment_delivery(
        &self,
        _start: &Path,
        _environment: &str,
        _mode: crate::domain::environment_delivery::EnvironmentDeliveryMode,
    ) -> Result<ConfigDocument, MmError> {
        Err(read_only())
    }

    fn set_development_mount(
        &self,
        _start: &Path,
        _module: &str,
        _mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
        _expected_source_hash: &str,
    ) -> Result<ConfigDocument, MmError> {
        Err(read_only())
    }

    fn ignored_sources(&self, document: &ConfigDocument) -> Vec<PathBuf> {
        self.inner.ignored_sources(document)
    }
}

/// A node reads the commit it was given; it never edits configuration.
fn read_only() -> MmError {
    MmError::State {
        message: "a cluster node reads the deployed commit's configuration and does not edit it"
            .to_owned(),
    }
}

#[cfg(test)]
mod tests;
