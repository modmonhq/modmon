use std::path::{Path, PathBuf};

use crate::domain::config::ConfigDocument;
use crate::domain::environment::EnvironmentIndex;
use crate::domain::error::MmError;
use crate::domain::onboarding::{
    ConfigLayoutPreview, ConfigLayoutRequest, ConfigMutationPreview, ConfigMutationRequest,
    InitializationPreview, ProjectDiscovery,
};

pub trait ProjectInitialization: Send + Sync {
    fn document(&self) -> &ConfigDocument;
    fn starter_files(&self) -> &[PathBuf];
    fn rollback(&self) -> Result<(), MmError>;
}

pub trait ProjectDocumentStore: Send + Sync {
    fn discover(&self, start: &Path) -> Result<PathBuf, MmError>;
    fn load_from(&self, start: &Path) -> Result<ConfigDocument, MmError>;
    fn load(&self, path: &Path) -> Result<ConfigDocument, MmError>;
    fn initialize(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<Box<dyn ProjectInitialization>, MmError>;
    fn initialize_reviewed(
        &self,
        _root: &Path,
        _requested_name: Option<&str>,
        _plan_hash: &str,
    ) -> Result<Box<dyn ProjectInitialization>, MmError> {
        Err(MmError::State {
            message: "reviewed initialization is not supported by this document store".to_owned(),
        })
    }
    fn schema(&self) -> schemars::Schema;
    fn set_default_environment(
        &self,
        start: &Path,
        environment: Option<&str>,
    ) -> Result<ConfigDocument, MmError>;
    fn set_environment_delivery(
        &self,
        start: &Path,
        environment: &str,
        mode: crate::domain::environment_delivery::EnvironmentDeliveryMode,
    ) -> Result<ConfigDocument, MmError>;
    fn set_development_mount(
        &self,
        start: &Path,
        module: &str,
        mount: Option<&crate::domain::development_mount::DevelopmentMountConfig>,
        expected_source_hash: &str,
    ) -> Result<ConfigDocument, MmError>;
    /// Split configuration files that git ignores. A remote environment loads
    /// configuration from a commit, so it would never see them.
    fn ignored_sources(&self, _document: &ConfigDocument) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Replaces the optional units `.config/mm.overrides.yaml` turns on.
    fn set_development_enabled(
        &self,
        _start: &Path,
        _enabled: &[String],
    ) -> Result<ConfigDocument, MmError> {
        Err(MmError::State {
            message: "this document store cannot change the local overrides".to_owned(),
        })
    }
}

pub trait EnvironmentSourceDiscovery: Send + Sync {
    fn index(&self, document: &ConfigDocument, environment: &str) -> EnvironmentIndex;
}

pub trait MaterializedEnvironmentWriter: Send + Sync {
    fn write(&self, path: &Path, bytes: &[u8], force: bool) -> Result<(), MmError>;
}

pub trait GuidedConfigEditor: Send + Sync {
    fn preview_initialization(
        &self,
        root: &Path,
        requested_name: Option<&str>,
    ) -> Result<InitializationPreview, MmError>;
    fn preview(
        &self,
        start: &Path,
        request: &ConfigMutationRequest,
    ) -> Result<ConfigMutationPreview, MmError>;
    fn commit(
        &self,
        preview: &ConfigMutationPreview,
        expected_source_hash: &str,
        expected_result_hash: &str,
    ) -> Result<ConfigDocument, MmError>;
    /// The resource a request names, as it is written today: the starting
    /// point for an edit that changes some of its keys. `None` when absent.
    fn definition(
        &self,
        _start: &Path,
        _request: &ConfigMutationRequest,
    ) -> Result<Option<serde_json::Value>, MmError> {
        Ok(None)
    }
    fn discover_project(&self, root: &Path) -> Result<ProjectDiscovery, MmError>;
    /// Previews moving one top-level key to or from `.config/mm/<key>.yaml`.
    fn preview_layout(
        &self,
        _start: &Path,
        _request: &ConfigLayoutRequest,
    ) -> Result<ConfigLayoutPreview, MmError> {
        Err(MmError::State {
            message: "split configuration files are not supported by this editor".to_owned(),
        })
    }
    /// Writes a reviewed layout change. The second file write rolls back the
    /// first if it fails.
    fn commit_layout(
        &self,
        _preview: &ConfigLayoutPreview,
        _expected_source_hash: &str,
        _expected_result_hash: &str,
    ) -> Result<ConfigDocument, MmError> {
        Err(MmError::State {
            message: "split configuration files are not supported by this editor".to_owned(),
        })
    }
}
