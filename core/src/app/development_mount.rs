use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::app::WorkspaceSelection;
use crate::app::ports::{ProjectDocumentStore, RuntimeDriver, TargetStore};
use crate::domain::config::{ConfigDocument, ModuleConfig, ServiceRole, resolve_environment};
use crate::domain::development_mount::{
    DevelopmentMountConfig, DevelopmentMountEvidence, DevelopmentMountMode, DevelopmentMountPlan,
    DevelopmentMountPreset, DevelopmentMountProposal, HostPlatform, MountConfidence,
    MountEvidenceSource, PreservedContainerPath, is_absolute_container_path,
};
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeCapability;
use crate::domain::target::TargetTransport;

const PLAN_SCHEMA: &str = "mm-development-mount-plan-v1";
const MAX_DISCOVERY_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SCANNED_ENTRIES: usize = 100_000;

mod discovery;
pub(crate) use discovery::discovery_proposals;
use discovery::*;
mod policy;
use policy::*;
pub(crate) use policy::{join_container_path, validate_intent};

#[derive(Debug, Clone, Default)]
pub struct DevelopmentMountPlanRequest {
    pub module: String,
    pub source: Option<String>,
    pub target: Option<String>,
    pub services: Vec<String>,
    pub mode: DevelopmentMountMode,
    pub preset: Option<DevelopmentMountPreset>,
    pub safe_exceptions: Vec<String>,
}

#[derive(Clone)]
pub struct DevelopmentMountManager {
    documents: Arc<dyn ProjectDocumentStore>,
    targets: Arc<dyn TargetStore>,
    runtime: Arc<dyn RuntimeDriver>,
}

impl std::fmt::Debug for DevelopmentMountManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DevelopmentMountManager")
            .field("runtime", &self.runtime.kind())
            .finish_non_exhaustive()
    }
}

impl DevelopmentMountManager {
    #[must_use]
    pub fn new(
        documents: Arc<dyn ProjectDocumentStore>,
        targets: Arc<dyn TargetStore>,
        runtime: Arc<dyn RuntimeDriver>,
    ) -> Self {
        Self {
            documents,
            targets,
            runtime,
        }
    }

    pub async fn plan(
        &self,
        selection: &WorkspaceSelection,
        request: &DevelopmentMountPlanRequest,
    ) -> Result<DevelopmentMountPlan, MmError> {
        let document = self.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        let (resolved, target) = crate::app::resolve_selection(
            &document,
            &environment,
            selection.runtime,
            selection.target.as_deref(),
            None,
            None,
            self.targets.as_ref(),
        )?;
        if document.config.project.development_environment.as_deref() != Some(&environment) {
            return Err(MmError::Security {
                message: format!(
                    "development mounts are available only in configured development environment '{}' (selected '{environment}')",
                    document
                        .config
                        .project
                        .development_environment
                        .as_deref()
                        .unwrap_or("not configured")
                ),
            });
        }
        if target.definition.transport != TargetTransport::Local {
            return Err(MmError::Security {
                message: format!(
                    "development bind mounts are local-only; target '{}' uses a remote transport",
                    target.name
                ),
            });
        }
        if resolved.runtime != self.runtime.kind() {
            return Err(MmError::Security {
                message: "development mount planner runtime does not match the resolved runtime"
                    .to_owned(),
            });
        }
        let probe = self.runtime.probe().await;
        probe.require_capabilities(
            "development_bind_mounts",
            &[
                RuntimeCapability::BindMounts,
                RuntimeCapability::ManageVolumes,
            ],
            Some(&target.name),
        )?;
        let proposal = proposal(&document, request)?;
        let mut plan = DevelopmentMountPlan {
            schema: PLAN_SCHEMA.to_owned(),
            project_id: document.config.project.id,
            project_name: document.config.project.name.clone(),
            config_hash: document.source_hash.clone(),
            environment: environment.clone(),
            target: target.name,
            local_target: true,
            development_environment: true,
            runtime: resolved.runtime,
            proposal,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan_hash(&plan)?;
        Ok(plan)
    }

    pub async fn configure_reviewed(
        &self,
        selection: &WorkspaceSelection,
        request: &DevelopmentMountPlanRequest,
        expected_plan_hash: &str,
    ) -> Result<DevelopmentMountPlan, MmError> {
        let plan = self.plan(selection, request).await?;
        if plan.plan_hash != expected_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "development mount plan changed (expected {expected_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        let intent = plan.proposal.intent.as_ref().ok_or_else(|| MmError::Environment {
            message: format!(
                "module '{}' has no mount target; pass --mount-target after reviewing project conventions",
                request.module
            ),
        })?;
        self.documents.set_development_mount(
            &selection.start,
            &request.module,
            Some(intent),
            &plan.config_hash,
        )?;
        self.plan(selection, request).await
    }

    pub async fn disable_reviewed(
        &self,
        selection: &WorkspaceSelection,
        request: &DevelopmentMountPlanRequest,
        expected_plan_hash: &str,
    ) -> Result<DevelopmentMountPlan, MmError> {
        let plan = self.plan(selection, request).await?;
        if plan.plan_hash != expected_plan_hash {
            return Err(MmError::State {
                message: format!(
                    "development mount plan changed (expected {expected_plan_hash}, current {}); review the new plan",
                    plan.plan_hash
                ),
            });
        }
        if !plan.proposal.configured {
            return Err(MmError::Environment {
                message: format!(
                    "module '{}' has no configured development mount",
                    request.module
                ),
            });
        }
        self.documents.set_development_mount(
            &selection.start,
            &request.module,
            None,
            &plan.config_hash,
        )?;
        self.plan(selection, request).await
    }
}

pub(crate) fn active_mounts(
    document: &ConfigDocument,
    environment: &str,
    local_target: bool,
) -> Result<BTreeMap<String, DevelopmentMountConfig>, MmError> {
    if document.config.project.development_environment.as_deref() != Some(environment)
        || !local_target
    {
        return Ok(BTreeMap::new());
    }
    let mut mounts = BTreeMap::new();
    for (module_name, module) in &document.config.modules {
        if let Some(mount) = &module.development_mount {
            validate_intent(document, module_name, module, mount)?;
            mounts.insert(module_name.clone(), mount.clone());
        }
    }
    reject_mount_conflicts(document, &mounts)?;
    Ok(mounts)
}

#[cfg(test)]
mod tests;
