use super::*;

impl DeploymentService {
    pub(super) async fn apply_with_environment(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
        target: &NamedTarget,
        operation_id: OperationId,
        previous: Option<&DeploymentPlan>,
    ) -> Result<DeploymentResult, MmError> {
        let mut guard = self
            .prepare_environment(selection, plan, operation_id)
            .await?;
        // What the apply changed, so that a failure puts back only that.
        let mut changes = crate::domain::rollout::RolloutChanges::default();
        let result = match self
            .apply_locked(selection, plan, target, guard.bundle(), &mut changes)
            .await
        {
            Ok(result) => Ok(result),
            Err(error) => {
                let mut cleanup_failures = Vec::new();
                if let Err(phase) = self.record_phase(operation_id, OperationPhase::Rollback) {
                    cleanup_failures.push(phase.to_string());
                }
                if let Err(rollback) = self
                    .rollback_runtime(plan, target, previous, guard.bundle(), Some(&changes))
                    .await
                {
                    cleanup_failures.push(rollback.to_string());
                }
                if cleanup_failures.is_empty() {
                    Err(error)
                } else {
                    Err(MmError::operation_failure(
                        operation_id.0.to_string(),
                        crate::domain::error::MutationStatus::Unknown,
                        error,
                        cleanup_failures,
                        "inspect the target and operation journal, then retry recovery",
                    ))
                }
            }
        };
        let cleanup = guard.dispose();
        match (result, cleanup) {
            (Ok(result), Ok(())) => Ok(result),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(cleanup)) => Err(MmError::operation_failure(
                operation_id.0.to_string(),
                crate::domain::error::MutationStatus::Applied,
                cleanup,
                Vec::new(),
                "remove the operation-scoped environment artifacts before retrying",
            )),
            (Err(primary), Err(cleanup)) => Err(MmError::operation_failure(
                operation_id.0.to_string(),
                crate::domain::error::MutationStatus::Unknown,
                primary,
                vec![cleanup.to_string()],
                "inspect the target and remove operation-scoped environment artifacts before retrying",
            )),
        }
    }

    pub(super) async fn prepare_environment(
        &self,
        selection: &WorkspaceSelection,
        plan: &DeploymentPlan,
        operation_id: OperationId,
    ) -> Result<EnvironmentArtifactGuard, MmError> {
        let selection = &selection.with_release(plan.release.clone());
        let review = self.ports.environments.review(selection).await?;
        if review.digest() != plan.environment_review.digest() {
            return Err(MmError::State {
                message: "environment metadata changed after the reviewed deployment plan"
                    .to_owned(),
            });
        }
        let compiled = if review.services.is_empty() {
            None
        } else {
            Some(self.ports.environments.compile(selection).await?)
        };
        let registry_credentials = self
            .ports
            .environments
            .registry_credentials(selection)
            .await?;
        if !registry_credentials.is_empty() && compiled.is_none() {
            return Err(MmError::Environment {
                message: "registries: needs at least one service with a compiled environment to carry its logins".to_owned(),
            });
        }
        let mut prepared = if let Some(compiled) = &compiled {
            let mut bundle = self.ports.environment_artifacts.publish(
                crate::app::ports::EnvironmentArtifactRequest {
                    operation_id,
                    delivery_driver: plan.environment_review.delivery.mode.driver(),
                    bundle: compiled,
                },
            )?;
            bundle.registry_credentials = registry_credentials;
            Some(bundle)
        } else {
            None
        };
        let runtime_prepared = if let Some(runtime_plan) = &plan.runtime_secret_plan {
            let compiled = compiled.as_ref().ok_or_else(|| MmError::Secret {
                message: "runtime secret plan has no compiled workload material".to_owned(),
            })?;
            let mut materials = BTreeMap::new();
            for secret in compiled
                .services
                .iter()
                .flat_map(|service| &service.runtime_secrets)
            {
                materials
                    .entry(secret.logical_name.clone())
                    .or_insert_with(|| secret.material.expose(str::to_owned));
            }
            let materials = materials
                .into_iter()
                .map(|(logical_name, value)| {
                    Ok(crate::domain::runtime_secret::RuntimeSecretMaterial {
                        logical_name,
                        material: crate::domain::secret::SecretMaterial::new(value)
                            .map_err(|message| MmError::Secret { message })?,
                    })
                })
                .collect::<Result<Vec<_>, MmError>>()?;
            let runtime_prepared = match self
                .ports
                .runtime_secrets
                .prepare(operation_id, runtime_plan, materials)
                .await
            {
                Ok(prepared) => prepared,
                Err(error) => {
                    if let Some(environment) = prepared.take() {
                        self.ports.environment_artifacts.dispose(&environment)?;
                    }
                    return Err(error);
                }
            };
            if let Some(environment) = &mut prepared {
                environment
                    .runtime_secret_overlays
                    .clone_from(&runtime_prepared.overlays);
                environment
                    .runtime_secret_rollback_overlays
                    .clone_from(&runtime_prepared.rollback_overlays);
            }
            Some(runtime_prepared)
        } else {
            None
        };
        Ok(EnvironmentArtifactGuard::new(
            Arc::clone(&self.ports.environment_artifacts),
            prepared,
            Arc::clone(&self.ports.runtime_secrets),
            runtime_prepared,
        ))
    }
}
