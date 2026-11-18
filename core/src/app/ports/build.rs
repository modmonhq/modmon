use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::domain::build::{
    BuildSource, BuiltImage, ImageBuildUnitPlan, ImageBuildVariantPlan, ResolvedBuildDependency,
};
use crate::domain::error::MmError;
use crate::domain::operation::OperationId;

#[async_trait]
pub trait BuildSourceInspector: Send + Sync {
    async fn inspect(&self, project_root: &Path) -> Result<BuildSource, MmError>;
}

pub struct BuildVariantRequest<'a> {
    pub operation_id: OperationId,
    pub project_root: &'a Path,
    pub plan_hash: &'a str,
    pub platforms: &'a [String],
    pub publish: bool,
    pub unit: &'a ImageBuildUnitPlan,
    pub variant: &'a ImageBuildVariantPlan,
    pub secrets: &'a BTreeMap<String, crate::domain::secret::SecretMaterial>,
    pub dependencies: &'a [ResolvedBuildDependency],
    pub upstream_contexts: &'a [UpstreamBuildContext],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamBuildContext {
    pub immutable_reference: String,
    pub digest: String,
    pub local_archive: Option<PathBuf>,
}

pub trait BuildOperationGuard: Send {
    fn preserve(self: Box<Self>);
}

struct NoopBuildOperationGuard;

impl BuildOperationGuard for NoopBuildOperationGuard {
    fn preserve(self: Box<Self>) {}
}

#[async_trait]
pub trait ImageBuilder: Send + Sync {
    fn begin_operation(
        &self,
        _operation_id: OperationId,
    ) -> Result<Box<dyn BuildOperationGuard>, MmError> {
        Ok(Box::new(NoopBuildOperationGuard))
    }

    fn cleanup_operation(&self, _operation_id: OperationId) -> Result<(), MmError> {
        Ok(())
    }

    async fn build_variant(&self, request: BuildVariantRequest<'_>) -> Result<BuiltImage, MmError>;

    async fn retag_variant(
        &self,
        source_repository: &str,
        digest: &str,
        destination_repository: &str,
        tags: &[String],
    ) -> Result<String, MmError>;
}

#[cfg(test)]
mod tests;
