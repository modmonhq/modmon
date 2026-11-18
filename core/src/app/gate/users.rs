use super::*;

impl GateManager {
    pub fn plan_user(
        &self,
        action: GateMutationAction,
        project_id: ProjectId,
        environment: &str,
        policy_name: &str,
        username: &str,
    ) -> Result<GateMutationPlan, MmError> {
        let policy = self.policy(project_id, environment, policy_name)?;
        let normalized = normalize_username(username).map_err(secret_error)?;
        let existing = self.dependencies.repository.user(policy.id, &normalized)?;
        let subject_revision = existing.as_ref().map(|user| user.metadata.password_version);
        match action {
            GateMutationAction::AddUser if existing.is_some() => {
                return Err(MmError::State {
                    message: "a user with that normalized username already exists".to_owned(),
                });
            }
            GateMutationAction::AddUser => {}
            GateMutationAction::DisableUser
            | GateMutationAction::EnableUser
            | GateMutationAction::ResetPassword
            | GateMutationAction::RemoveUser
                if existing.is_none() =>
            {
                return Err(MmError::State {
                    message: "gate user not found".to_owned(),
                });
            }
            GateMutationAction::DisableUser
            | GateMutationAction::EnableUser
            | GateMutationAction::ResetPassword
            | GateMutationAction::RemoveUser => {}
            _ => {
                return Err(MmError::Security {
                    message: "invalid gate user mutation".to_owned(),
                });
            }
        }
        let hidden = matches!(
            action,
            GateMutationAction::AddUser | GateMutationAction::ResetPassword
        );
        plan(GateMutationPlanInput {
            action,
            policy: policy_name,
            project_id,
            environment,
            subject: normalized,
            policy_revision: Some(policy.revision),
            subject_revision,
            ingress_generation: None,
            requires_hidden_input: hidden,
            effects: vec![if hidden {
                "hash password with Argon2id and the policy pepper".to_owned()
            } else {
                "change application-access principal state and revoke active sessions".to_owned()
            }],
        })
    }

    pub async fn apply_user_reviewed(
        &self,
        request: ApplyGateUserRequest<'_>,
    ) -> Result<GateUserMetadata, MmError> {
        let mutation = request.mutation;
        let action = mutation.action();
        let preview = self.plan_user(
            action,
            request.project_id,
            request.environment,
            request.policy_name,
            request.username,
        )?;
        require_review(&preview, request.reviewed_plan_hash)?;
        let policy = self.policy(request.project_id, request.environment, request.policy_name)?;
        if preview.policy_revision != Some(policy.revision) {
            return Err(stale());
        }
        let normalized = normalize_username(request.username).map_err(secret_error)?;
        let now = self.dependencies.clock.now_unix_ms();
        let mutation = match mutation {
            GateUserMutation::Add(password) => {
                let metadata = self
                    .add_user(&policy, normalized, request.username, password, now)
                    .await?;
                self.audit(&preview)?;
                return Ok(metadata);
            }
            GateUserMutation::Disable => ExistingGateUserMutation::Disable,
            GateUserMutation::Enable => ExistingGateUserMutation::Enable,
            GateUserMutation::ResetPassword(material) => {
                ExistingGateUserMutation::ResetPassword(material)
            }
            GateUserMutation::Remove => ExistingGateUserMutation::Remove,
        };
        let mut user = self
            .dependencies
            .repository
            .user(policy.id, &normalized)?
            .ok_or_else(|| MmError::State {
                message: "gate user not found".to_owned(),
            })?;
        let expected = user.metadata.password_version;
        if preview.subject_revision != Some(expected) {
            return Err(stale());
        }
        match mutation {
            ExistingGateUserMutation::Disable => {
                user.metadata.enabled = false;
                user.metadata.password_version = expected.checked_add(1).ok_or_else(stale)?;
            }
            ExistingGateUserMutation::Enable => {
                user.metadata.enabled = true;
                user.metadata.password_version = expected.checked_add(1).ok_or_else(stale)?;
            }
            ExistingGateUserMutation::ResetPassword(material) => {
                validate_new_password(&material)?;
                let pepper = self
                    .resolve_secret(&policy, &policy.password_pepper)
                    .await?;
                user.password_hash = self.dependencies.password.hash(&material, &pepper)?;
                user.metadata.password_version = user
                    .metadata
                    .password_version
                    .checked_add(1)
                    .ok_or_else(stale)?;
            }
            ExistingGateUserMutation::Remove => {
                let removed =
                    self.dependencies
                        .repository
                        .remove_user(policy.id, &normalized, expected)?;
                if !removed {
                    return Err(stale());
                }
                self.audit(&preview)?;
                return Ok(user.metadata);
            }
        }
        user.metadata.updated_at_unix_ms = now;
        user.failed_attempts = 0;
        user.locked_until_unix_ms = None;
        self.dependencies.repository.replace_user(&user, expected)?;
        if !user.metadata.enabled || action == GateMutationAction::ResetPassword {
            self.dependencies
                .repository
                .revoke_user_sessions(user.metadata.id, now)?;
        }
        self.audit(&preview)?;
        Ok(user.metadata)
    }

    async fn add_user(
        &self,
        policy: &GatePolicy,
        normalized_username: String,
        username: &str,
        material: SecretMaterial,
        now: i64,
    ) -> Result<GateUserMetadata, MmError> {
        validate_new_password(&material)?;
        let pepper = self.resolve_secret(policy, &policy.password_pepper).await?;
        let password_hash = self.dependencies.password.hash(&material, &pepper)?;
        let metadata = GateUserMetadata {
            id: Uuid::new_v4(),
            policy_id: policy.id,
            normalized_username,
            display_username: username.trim().to_owned(),
            enabled: true,
            password_version: 1,
            created_at_unix_ms: now,
            updated_at_unix_ms: now,
        };
        self.dependencies.repository.insert_user(&GateUserRecord {
            metadata: metadata.clone(),
            password_hash,
            failed_attempts: 0,
            window_started_at_unix_ms: now,
            locked_until_unix_ms: None,
        })?;
        Ok(metadata)
    }

    pub fn list_users(
        &self,
        project_id: ProjectId,
        environment: &str,
        policy: &str,
    ) -> Result<Vec<GateUserMetadata>, MmError> {
        let policy = self.policy(project_id, environment, policy)?;
        self.dependencies.repository.list_users(policy.id)
    }
}

enum ExistingGateUserMutation {
    Disable,
    Enable,
    ResetPassword(SecretMaterial),
    Remove,
}
