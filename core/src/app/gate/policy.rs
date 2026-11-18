use super::*;

impl GateManager {
    #[must_use]
    pub fn new(dependencies: GateManagerDependencies) -> Self {
        Self { dependencies }
    }

    pub fn plan_add_policy(
        &self,
        request: &AddGatePolicyRequest,
    ) -> Result<GateMutationPlan, MmError> {
        request.validate()?;
        if self
            .dependencies
            .repository
            .policy(
                request.project_id,
                &request.environment,
                request.name.as_str(),
            )?
            .is_some()
        {
            return Err(MmError::State {
                message: "a gate policy with that name already exists".to_owned(),
            });
        }
        plan(GateMutationPlanInput {
            action: GateMutationAction::AddPolicy,
            policy: request.name.as_str(),
            project_id: request.project_id,
            environment: &request.environment,
            subject: request.name.as_str().to_owned(),
            policy_revision: None,
            subject_revision: None,
            ingress_generation: None,
            requires_hidden_input: false,
            effects: vec![
                "create password pepper and versioned session key in the selected secret backend"
                    .to_owned(),
                "provision an mm-owned Cap site and store its secret in the selected backend"
                    .to_owned(),
                "enable conditional Cap and Valkey components".to_owned(),
            ],
        })
    }

    pub async fn add_policy_reviewed(
        &self,
        request: AddGatePolicyRequest,
        reviewed: &str,
    ) -> Result<GatePolicy, MmError> {
        let preview = self.plan_add_policy(&request)?;
        require_review(&preview, reviewed)?;
        let provider = StorageProviderId::new(request.secret_provider.clone())
            .map_err(|message| MmError::Security { message })?;
        let base = format!(
            "gate/{}/{}/{}",
            request.project_id.0,
            request.environment,
            request.name.as_str()
        );
        let pepper = SecretReference::new(provider.clone(), format!("{base}/password-pepper"))
            .map_err(secret_error)?;
        let session = SecretReference::new(provider.clone(), format!("{base}/session-key/1"))
            .map_err(secret_error)?;
        let cap = SecretReference::new(provider, format!("{base}/cap-site-secret"))
            .map_err(secret_error)?;
        let scope = SecretScope::new(request.environment.clone());
        let now = self.dependencies.clock.now_unix_ms();
        let pepper_material = random_secret()?;
        let session_material = random_secret()?;
        let policy_id = Uuid::new_v4();
        let cap_name = format!(
            "mm-{}-{}-{}-{}",
            request.project_id.0.simple(),
            &blake3::hash(request.environment.as_bytes()).to_hex()[..12],
            request.name.as_str(),
            policy_id.simple(),
        );
        let cap_site = self.dependencies.cap_sites.create(&cap_name).await?;
        let cap_site_key = cap_site.site_key.clone();
        let mut created = Vec::new();
        for (reference, material) in [
            (&pepper, pepper_material),
            (&session, session_material),
            (&cap, cap_site.site_secret),
        ] {
            let result = match self
                .dependencies
                .secret_creator
                .create(&scope, reference, material, now)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    let cleanup = self
                        .cleanup_policy_resources(&scope, created, &cap_site_key)
                        .await;
                    return Err(with_gate_cleanup(&preview, error, cleanup));
                }
            };
            if !result.changed {
                let cleanup = self
                    .cleanup_policy_resources(&scope, created, &cap_site_key)
                    .await;
                return Err(with_gate_cleanup(
                    &preview,
                    MmError::State {
                        message: "owned gate secret material already exists; retry after the concurrent policy operation completes".to_owned(),
                    },
                    cleanup,
                ));
            }
            created.push((reference.clone(), result.revision));
        }
        let password_driver = GateDriverId::new(PASSWORD_CAP_DRIVER).map_err(secret_error)?;
        let policy = new_gate_policy(
            request,
            NewGatePolicyResources {
                policy_id,
                password_driver,
                password_pepper: pepper,
                session_key: session,
                cap_site_key: cap_site_key.clone(),
                cap_site_secret: cap,
                now,
            },
        );
        if let Err(error) = self.dependencies.repository.insert_policy(&policy) {
            let cleanup = self
                .cleanup_policy_resources(&scope, created, &cap_site_key)
                .await;
            return Err(with_gate_cleanup(&preview, error, cleanup));
        }
        self.audit(&preview)?;
        Ok(policy)
    }

    async fn cleanup_policy_resources(
        &self,
        scope: &SecretScope,
        created: Vec<(SecretReference, String)>,
        cap_site_key: &str,
    ) -> Vec<String> {
        let mut failures = Vec::new();
        for (reference, revision) in created.into_iter().rev() {
            if let Err(error) = self
                .dependencies
                .secret_remover
                .unset(
                    scope,
                    &reference,
                    SecretExpectedState {
                        active_revision: Some(revision),
                        candidate_revision: None,
                    },
                )
                .await
            {
                failures.push(error.to_string());
            }
        }
        if let Err(error) = self.dependencies.cap_sites.remove(cap_site_key).await {
            failures.push(error.to_string());
        }
        failures
    }

    pub fn show(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: Option<&str>,
    ) -> Result<Vec<GatePolicy>, MmError> {
        match name {
            Some(name) => self
                .policy(project_id, environment, name)
                .map(|value| vec![value]),
            None => self
                .dependencies
                .repository
                .list_policies(project_id, environment),
        }
    }

    pub fn policy_by_id(&self, id: Uuid) -> Result<GatePolicy, MmError> {
        self.dependencies
            .repository
            .policy_by_id(id)?
            .ok_or_else(|| MmError::State {
                message: "gate policy not found".to_owned(),
            })
    }

    pub fn plan_remove_policy(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
    ) -> Result<GateMutationPlan, MmError> {
        let policy = self.policy(project_id, environment, name)?;
        let attachments = self.dependencies.repository.list_attachments(policy.id)?;
        if !attachments.is_empty() {
            return Err(MmError::State {
                message: "detach every owned route before removing the gate policy".to_owned(),
            });
        }
        plan(GateMutationPlanInput {
            action: GateMutationAction::RemovePolicy,
            policy: name,
            project_id,
            environment,
            subject: name.to_owned(),
            policy_revision: Some(policy.revision),
            subject_revision: None,
            ingress_generation: None,
            requires_hidden_input: false,
            effects: vec![
                "remove policy, users, sessions, and replay metadata".to_owned(),
                "remove owned gate secrets from the selected backend".to_owned(),
            ],
        })
    }

    pub async fn remove_policy_reviewed(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
        reviewed: &str,
    ) -> Result<bool, MmError> {
        let preview = self.plan_remove_policy(project_id, environment, name)?;
        require_review(&preview, reviewed)?;
        let policy = self.policy(project_id, environment, name)?;
        if preview.policy_revision != Some(policy.revision) {
            return Err(stale());
        }
        let removed = self
            .dependencies
            .repository
            .remove_policy(policy.id, policy.revision)?;
        if !removed {
            return Err(stale());
        }
        let mut cleanup_failures = Vec::new();
        if let Err(error) = self
            .dependencies
            .cap_sites
            .remove(&policy.cap_site_key)
            .await
        {
            cleanup_failures.push(error.to_string());
        }
        let scope = SecretScope::new(environment);
        for reference in [
            &policy.password_pepper,
            &policy.session_keys.current,
            &policy.cap_site_secret,
        ] {
            let metadata = match self
                .dependencies
                .secret_inspector
                .inspect(&scope, reference)
                .await
            {
                Ok(metadata) => metadata,
                Err(error) => {
                    cleanup_failures.push(error.to_string());
                    continue;
                }
            };
            if let Err(error) = self
                .dependencies
                .secret_remover
                .unset(&scope, reference, metadata.expected_state())
                .await
            {
                cleanup_failures.push(error.to_string());
            }
        }
        if let Some(previous) = policy.session_keys.previous {
            match self
                .dependencies
                .secret_inspector
                .inspect(&scope, &previous.reference)
                .await
            {
                Ok(metadata) => {
                    if let Err(error) = self
                        .dependencies
                        .secret_remover
                        .unset(&scope, &previous.reference, metadata.expected_state())
                        .await
                    {
                        cleanup_failures.push(error.to_string());
                    }
                }
                Err(error) => cleanup_failures.push(error.to_string()),
            }
        }
        if !cleanup_failures.is_empty() {
            return Err(MmError::operation_failure(
                &preview.plan_hash,
                crate::domain::error::MutationStatus::Applied,
                MmError::State {
                    message: "gate policy was removed but owned resource cleanup was incomplete"
                        .to_owned(),
                },
                cleanup_failures,
                "inspect and remove the owned Cap site and gate secrets before recreating the policy",
            ));
        }
        self.audit(&preview)?;
        Ok(true)
    }
}

fn with_gate_cleanup(plan: &GateMutationPlan, primary: MmError, cleanup: Vec<String>) -> MmError {
    if cleanup.is_empty() {
        primary
    } else {
        MmError::operation_failure(
            &plan.plan_hash,
            crate::domain::error::MutationStatus::Unknown,
            primary,
            cleanup,
            "inspect and remove the owned Cap site and gate secrets before retrying",
        )
    }
}

struct NewGatePolicyResources {
    policy_id: Uuid,
    password_driver: GateDriverId,
    password_pepper: SecretReference,
    session_key: SecretReference,
    cap_site_key: String,
    cap_site_secret: SecretReference,
    now: i64,
}

fn new_gate_policy(request: AddGatePolicyRequest, resources: NewGatePolicyResources) -> GatePolicy {
    GatePolicy {
        id: resources.policy_id,
        name: request.name,
        project_id: request.project_id,
        environment: request.environment,
        mode: request.mode,
        drivers: vec![resources.password_driver],
        session_ttl_seconds: request.session_ttl_seconds,
        secret_provider: request.secret_provider,
        password_pepper: resources.password_pepper,
        session_keys: VersionedSecretReferences {
            current_version: 1,
            current: resources.session_key,
            previous: None,
        },
        cap_site_key: resources.cap_site_key,
        cap_site_secret: resources.cap_site_secret,
        revision: 1,
        created_at_unix_ms: resources.now,
        updated_at_unix_ms: resources.now,
    }
}
