use super::*;
use crate::app::ReceiptCheckout;
use crate::domain::build::{BuildImportImage, BuildImportPlan, BuildReceipt, IMPORT_PLAN_SCHEMA};
use crate::domain::config::resolve_environment;

impl BuildService {
    /// Reviews `mm build import`. A normal import records the receipt as this
    /// checkout's published build for the selected environment, under the
    /// same rules as deploying it; a cache-only import, of any commit, only
    /// seeds build reuse.
    pub async fn review_import(
        &self,
        selection: &WorkspaceSelection,
        receipt: &BuildReceipt,
        cache_only: bool,
    ) -> Result<BuildImportPlan, MmError> {
        Ok(self.prepare_import(selection, receipt, cache_only).await?.0)
    }

    /// Applies an import reviewed with `review_import`.
    pub async fn apply_import(
        &self,
        selection: &WorkspaceSelection,
        receipt: &BuildReceipt,
        cache_only: bool,
        expected_plan_hash: &str,
    ) -> Result<BuildImportPlan, MmError> {
        let preview = self.review_import(selection, receipt, cache_only).await?;
        require_import_hash(&preview, expected_plan_hash)?;
        let _guard = self.ports.locks.acquire(preview.project_id)?;
        let (plan, bound) = self.prepare_import(selection, receipt, cache_only).await?;
        require_import_hash(&plan, expected_plan_hash)?;

        let document = self.ports.documents.load_from(&selection.start)?;
        let now = self.ports.clock.now_unix_ms();
        let workspace = self.ports.state.register_workspace(
            &WorkspaceRegistration {
                project_id: plan.project_id,
                root: document.root,
                config_path: document.path,
            },
            now,
        )?;
        let operation_id = OperationId::new();
        self.ports.state.begin_operation(&NewOperation {
            id: operation_id,
            workspace_id: workspace.id,
            kind: OperationKind::Build,
            environment: plan.environment.clone(),
            requested_at_unix_ms: now,
            plan_hash: Some(
                plan.build_plan_hash
                    .clone()
                    .unwrap_or_else(|| plan.plan_hash.clone()),
            ),
        })?;
        let recorded = self.record_import(operation_id, &plan, &bound);
        let (state, code) = match &recorded {
            Ok(()) => (OperationState::Succeeded, None),
            Err(error) => (OperationState::Failed, Some(error.code())),
        };
        self.finish(operation_id, state, code)?;
        recorded.map(|()| plan)
    }

    /// The import plan and the receipt bound to the selected environment.
    async fn prepare_import(
        &self,
        selection: &WorkspaceSelection,
        receipt: &BuildReceipt,
        cache_only: bool,
    ) -> Result<(BuildImportPlan, BuildReceipt), MmError> {
        receipt
            .validate()
            .map_err(|message| MmError::Security { message })?;
        let document = self.ports.documents.load_from(&selection.start)?;
        let environment = resolve_environment(&document, selection.environment.as_deref())?;
        if receipt.plan.project_id != document.config.project.id {
            return Err(MmError::Security {
                message: "the build receipt is for another project".to_owned(),
            });
        }
        let bound = if cache_only {
            // Reuse is keyed by environment; nothing is deployed, so the
            // commit and configuration may differ from this checkout.
            receipt
                .rebind(&environment, &receipt.plan.config_hash)
                .map_err(|message| MmError::Build { message })?
        } else {
            self.planner
                .receipt_for_environment(selection, receipt, &ReceiptCheckout::Worktree)
                .await?
                .receipt
        };
        let mut images = Vec::with_capacity(bound.result.images.len());
        for image in &bound.result.images {
            let unit = bound
                .plan
                .units
                .iter()
                .find(|unit| unit.module == image.module)
                .ok_or_else(|| MmError::Security {
                    message: "build receipt image has no build unit".to_owned(),
                })?;
            // A module this checkout no longer builds into the same
            // repository must not seed reuse.
            let configured = document
                .config
                .modules
                .get(&unit.module)
                .and_then(|module| module.build.as_ref())
                .filter(|build| build.enabled)
                .and_then(|build| build.image.as_deref())
                .map(crate::app::build::normalize_image_repository)
                .transpose()?;
            let reusable = unit.reuse_policy == BuildReusePolicy::Allowed
                && configured.as_deref() == Some(image.repository.as_str());
            images.push(BuildImportImage {
                module: image.module.clone(),
                target: image.target.clone(),
                repository: image.repository.clone(),
                digest: image.digest.clone(),
                reuse_fingerprint: reusable.then(|| image.execution_fingerprint.clone()),
            });
        }
        let mut plan = BuildImportPlan {
            schema: IMPORT_PLAN_SCHEMA.to_owned(),
            project_id: receipt.plan.project_id,
            receipt_hash: receipt.receipt_hash.clone(),
            commit: receipt.source_commit().to_owned(),
            source_environment: receipt.plan.environment.clone(),
            environment,
            cache_only,
            build_plan_hash: (!cache_only).then(|| bound.plan.plan_hash.clone()),
            images,
            plan_hash: String::new(),
        };
        plan.plan_hash = plan
            .canonical_hash()
            .map_err(|message| MmError::Build { message })?;
        Ok((plan, bound))
    }

    fn record_import(
        &self,
        operation_id: OperationId,
        plan: &BuildImportPlan,
        bound: &BuildReceipt,
    ) -> Result<(), MmError> {
        if !plan.cache_only {
            self.ports
                .state
                .save_image_build_plan(operation_id, &bound.plan)?;
            let mut result = bound.result.clone();
            result.operation_id = operation_id;
            self.ports
                .state
                .save_image_build_result(operation_id, &result)?;
        }
        for image in &plan.images {
            let Some(fingerprint) = &image.reuse_fingerprint else {
                continue;
            };
            self.ports
                .state
                .record_reusable_image(ReusableImageRecord {
                    project_id: plan.project_id,
                    environment: &plan.environment,
                    fingerprint,
                    module: &image.module,
                    variant: image.target.as_deref(),
                    repository: &image.repository,
                    digest: &image.digest,
                    operation_id,
                })?;
        }
        Ok(())
    }
}

fn require_import_hash(plan: &BuildImportPlan, expected: &str) -> Result<(), MmError> {
    if plan.plan_hash == expected {
        Ok(())
    } else {
        Err(MmError::State {
            message: format!(
                "build import plan changed (expected {expected}, current {}); review the new plan",
                plan.plan_hash
            ),
        })
    }
}
