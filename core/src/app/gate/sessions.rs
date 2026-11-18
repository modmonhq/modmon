use super::*;

impl GateManager {
    pub async fn login(&self, request: GateLoginRequest) -> Result<GateLoginResult, MmError> {
        let policy = self
            .dependencies
            .repository
            .policy_by_id(request.policy_id)?
            .ok_or_else(authentication_failed)?;
        let now = self.dependencies.clock.now_unix_ms();
        let principal = match self
            .dependencies
            .drivers
            .authenticate(
                &policy,
                &GateAuthenticationInput {
                    username: &request.username,
                    password: &request.password,
                    cap_token: &request.cap_token,
                },
            )
            .await?
        {
            GateEvaluation::Authorized(principal) => principal,
            GateEvaluation::Challenge(_) | GateEvaluation::Denied | GateEvaluation::Unavailable => {
                return Err(authentication_failed());
            }
        };
        let user = self
            .dependencies
            .repository
            .user_by_id(principal.user_id)?
            .ok_or_else(authentication_failed)?;
        if !user.metadata.enabled {
            return Err(authentication_failed());
        }
        let csrf = random_token(32)?;
        let id = Uuid::new_v4();
        let expires = now.saturating_add(
            i64::try_from(policy.session_ttl_seconds)
                .unwrap_or(i64::MAX)
                .saturating_mul(1000),
        );
        let claims = GateSessionClaims {
            session_id: id,
            policy_id: policy.id,
            user_id: user.metadata.id,
            password_version: user.metadata.password_version,
            authenticated_drivers: principal.authenticated_drivers.clone(),
            issued_at_unix_ms: now,
            expires_at_unix_ms: expires,
            csrf_binding: blake3::hash(csrf.as_bytes()).to_hex().to_string(),
        };
        let key = self
            .resolve_secret(&policy, &policy.session_keys.current)
            .await?;
        let cookie =
            self.dependencies
                .sessions
                .seal(&claims, policy.session_keys.current_version, &key)?;
        let session = GateSessionMetadata {
            id,
            policy_id: policy.id,
            user_id: user.metadata.id,
            username: user.metadata.display_username,
            issued_at_unix_ms: now,
            expires_at_unix_ms: expires,
            last_seen_at_unix_ms: now,
            key_version: policy.session_keys.current_version,
            revoked_at_unix_ms: None,
            replaced_by: None,
        };
        if let Some(replaced) = request.replace_session_id {
            self.dependencies
                .repository
                .insert_session_replacing(&session, replaced, now)?;
        } else {
            self.dependencies.repository.insert_session(&session)?;
        }
        Ok(GateLoginResult {
            principal,
            session,
            cookie_value: zeroize::Zeroizing::new(cookie),
            csrf_token: zeroize::Zeroizing::new(csrf),
        })
    }

    pub async fn validate_session(
        &self,
        policy_id: Uuid,
        cookie: &str,
    ) -> Result<GateSessionValidation, MmError> {
        let policy = self
            .dependencies
            .repository
            .policy_by_id(policy_id)?
            .ok_or_else(authentication_failed)?;
        let version = cookie.split('.').nth(1).ok_or_else(authentication_failed)?;
        let version = version
            .parse::<u32>()
            .map_err(|_| authentication_failed())?;
        let (reference, requires_rotation) = if version == policy.session_keys.current_version {
            (&policy.session_keys.current, false)
        } else if let Some(previous) = &policy.session_keys.previous {
            if version != previous.version
                || previous.accept_until_unix_ms < self.dependencies.clock.now_unix_ms()
            {
                return Err(authentication_failed());
            }
            (&previous.reference, true)
        } else {
            return Err(authentication_failed());
        };
        let key = self.resolve_secret(&policy, reference).await?;
        let claims = self
            .dependencies
            .sessions
            .open(cookie, version, &key)
            .map_err(|_| authentication_failed())?;
        let now = self.dependencies.clock.now_unix_ms();
        if claims.policy_id != policy.id
            || claims.issued_at_unix_ms > now.saturating_add(SESSION_CLOCK_SKEW_SECONDS * 1000)
            || claims.expires_at_unix_ms < now.saturating_sub(SESSION_CLOCK_SKEW_SECONDS * 1000)
            || claims.authenticated_drivers.is_empty()
            || !claims
                .authenticated_drivers
                .windows(2)
                .all(|drivers| drivers[0] < drivers[1])
            || claims
                .authenticated_drivers
                .iter()
                .any(|driver| !policy.drivers.contains(driver))
            || (policy.mode == GatePolicyMode::All
                && claims.authenticated_drivers != policy.drivers)
        {
            return Err(authentication_failed());
        }
        let session = self
            .dependencies
            .repository
            .session(claims.session_id)?
            .ok_or_else(authentication_failed)?;
        if session.policy_id != claims.policy_id
            || session.user_id != claims.user_id
            || session.key_version != version
            || session.revoked_at_unix_ms.is_some()
            || session.expires_at_unix_ms != claims.expires_at_unix_ms
        {
            return Err(authentication_failed());
        }
        let user = self
            .dependencies
            .repository
            .user_by_id(claims.user_id)?
            .ok_or_else(authentication_failed)?;
        if !user.metadata.enabled || user.metadata.password_version != claims.password_version {
            return Err(authentication_failed());
        }
        Ok(GateSessionValidation {
            principal: GatePrincipal {
                user_id: user.metadata.id,
                normalized_username: user.metadata.normalized_username,
                display_username: user.metadata.display_username,
                authenticated_drivers: claims.authenticated_drivers,
            },
            session,
            csrf_binding: claims.csrf_binding,
            requires_rotation,
        })
    }

    pub async fn rotate_validated_session(
        &self,
        validation: &GateSessionValidation,
    ) -> Result<GateSessionRotation, MmError> {
        if !validation.requires_rotation {
            return Err(MmError::State {
                message: "the gate session already uses the current key".to_owned(),
            });
        }
        let policy = self
            .dependencies
            .repository
            .policy_by_id(validation.session.policy_id)?
            .ok_or_else(authentication_failed)?;
        let user = self
            .dependencies
            .repository
            .user_by_id(validation.session.user_id)?
            .ok_or_else(authentication_failed)?;
        if !user.metadata.enabled {
            return Err(authentication_failed());
        }
        let now = self.dependencies.clock.now_unix_ms();
        if validation.session.expires_at_unix_ms <= now {
            return Err(authentication_failed());
        }
        let id = Uuid::new_v4();
        let claims = GateSessionClaims {
            session_id: id,
            policy_id: policy.id,
            user_id: user.metadata.id,
            password_version: user.metadata.password_version,
            authenticated_drivers: validation.principal.authenticated_drivers.clone(),
            issued_at_unix_ms: now,
            expires_at_unix_ms: validation.session.expires_at_unix_ms,
            csrf_binding: validation.csrf_binding.clone(),
        };
        let key = self
            .resolve_secret(&policy, &policy.session_keys.current)
            .await?;
        let cookie =
            self.dependencies
                .sessions
                .seal(&claims, policy.session_keys.current_version, &key)?;
        let replacement = GateSessionMetadata {
            id,
            policy_id: policy.id,
            user_id: user.metadata.id,
            username: user.metadata.display_username,
            issued_at_unix_ms: now,
            expires_at_unix_ms: validation.session.expires_at_unix_ms,
            last_seen_at_unix_ms: now,
            key_version: policy.session_keys.current_version,
            revoked_at_unix_ms: None,
            replaced_by: None,
        };
        self.dependencies.repository.insert_session_replacing(
            &replacement,
            validation.session.id,
            now,
        )?;
        Ok(GateSessionRotation {
            session: replacement,
            cookie_value: zeroize::Zeroizing::new(cookie),
        })
    }

    pub fn list_sessions(
        &self,
        project_id: ProjectId,
        environment: &str,
        policy: &str,
    ) -> Result<Vec<GateSessionMetadata>, MmError> {
        let policy = self.policy(project_id, environment, policy)?;
        self.dependencies
            .repository
            .list_sessions(policy.id, self.dependencies.clock.now_unix_ms())
    }
    pub fn plan_revoke_session(
        &self,
        project_id: ProjectId,
        environment: &str,
        policy: &str,
        id: Uuid,
    ) -> Result<GateMutationPlan, MmError> {
        let policy_value = self.policy(project_id, environment, policy)?;
        let session = self
            .dependencies
            .repository
            .session(id)?
            .ok_or_else(|| MmError::State {
                message: "gate session not found".to_owned(),
            })?;
        if session.policy_id != policy_value.id {
            return Err(MmError::Security {
                message: "session is not owned by the selected gate policy".to_owned(),
            });
        }
        if session.revoked_at_unix_ms.is_some() {
            return Err(MmError::State {
                message: "gate session is already revoked".to_owned(),
            });
        }
        plan(GateMutationPlanInput {
            action: GateMutationAction::RevokeSession,
            policy,
            project_id,
            environment,
            subject: id.to_string(),
            policy_revision: Some(policy_value.revision),
            subject_revision: Some(u64::from(session.key_version)),
            ingress_generation: None,
            requires_hidden_input: false,
            effects: vec!["revoke the selected opaque session immediately".to_owned()],
        })
    }
    pub fn revoke_session_reviewed(
        &self,
        project_id: ProjectId,
        environment: &str,
        policy: &str,
        id: Uuid,
        reviewed: &str,
    ) -> Result<bool, MmError> {
        let preview = self.plan_revoke_session(project_id, environment, policy, id)?;
        require_review(&preview, reviewed)?;
        let revoked = self.dependencies.repository.revoke_session(
            id,
            self.dependencies.clock.now_unix_ms(),
            None,
        )?;
        if revoked {
            self.audit(&preview)?;
            Ok(true)
        } else {
            Err(stale())
        }
    }
    pub fn revoke_authenticated_session(&self, id: Uuid) -> Result<bool, MmError> {
        self.dependencies
            .repository
            .revoke_session(id, self.dependencies.clock.now_unix_ms(), None)
    }

    pub fn plan_rotate_session_key(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
    ) -> Result<GateMutationPlan, MmError> {
        let policy = self.policy(project_id, environment, name)?;
        plan(GateMutationPlanInput {
            action: GateMutationAction::RotateSessionKey,
            policy: name,
            project_id,
            environment,
            subject: name.to_owned(),
            policy_revision: Some(policy.revision),
            subject_revision: None,
            ingress_generation: None,
            requires_hidden_input: false,
            effects: vec![
                "create a new session key and retain the prior key for existing sessions"
                    .to_owned(),
            ],
        })
    }

    pub async fn rotate_session_key(
        &self,
        project_id: ProjectId,
        environment: &str,
        name: &str,
        reviewed: &str,
    ) -> Result<GatePolicy, MmError> {
        let policy = self.policy(project_id, environment, name)?;
        let preview = self.plan_rotate_session_key(project_id, environment, name)?;
        require_review(&preview, reviewed)?;
        let mut current = self.policy(project_id, environment, name)?;
        if current.revision != policy.revision {
            return Err(stale());
        }
        let now = self.dependencies.clock.now_unix_ms();
        if current
            .session_keys
            .previous
            .as_ref()
            .is_some_and(|previous| previous.accept_until_unix_ms >= now)
        {
            return Err(MmError::State { message: "the prior session key is still in its overlap window; rotate after that window expires".to_owned() });
        }
        let expired_previous = current.session_keys.previous.take();
        let next = current
            .session_keys
            .current_version
            .checked_add(1)
            .ok_or_else(stale)?;
        let reference = SecretReference::new(
            StorageProviderId::new(current.secret_provider.clone()).map_err(secret_error)?,
            format!(
                "gate/{}/{}/{}/session-key/{next}",
                project_id.0, environment, name
            ),
        )
        .map_err(secret_error)?;
        let scope = SecretScope::new(environment);
        let created = self
            .dependencies
            .secret_creator
            .create(&scope, &reference, random_secret()?, now)
            .await?;
        if !created.changed {
            return Err(MmError::State {
                message: "owned gate session key already exists; retry after the concurrent rotation completes"
                    .to_owned(),
            });
        }
        current.session_keys.previous = Some(PreviousSecretReference {
            version: current.session_keys.current_version,
            reference: current.session_keys.current,
            accept_until_unix_ms: now.saturating_add(
                i64::try_from(current.session_ttl_seconds)
                    .unwrap_or(i64::MAX)
                    .saturating_mul(1000),
            ),
        });
        current.session_keys.current = reference.clone();
        current.session_keys.current_version = next;
        current.revision = current.revision.checked_add(1).ok_or_else(stale)?;
        current.updated_at_unix_ms = now;
        if let Err(error) = self
            .dependencies
            .repository
            .replace_policy(&current, policy.revision)
        {
            if let Err(cleanup) = self
                .dependencies
                .secret_remover
                .unset(
                    &scope,
                    &reference,
                    SecretExpectedState {
                        active_revision: Some(created.revision),
                        candidate_revision: None,
                    },
                )
                .await
            {
                return Err(MmError::operation_failure(
                    preview.plan_hash,
                    crate::domain::error::MutationStatus::Unknown,
                    error,
                    vec![cleanup.to_string()],
                    "remove the unreferenced session key before retrying rotation",
                ));
            }
            return Err(error);
        }
        self.cleanup_expired_session_key(&scope, expired_previous)
            .await;
        self.audit(&preview)?;
        Ok(current)
    }

    async fn cleanup_expired_session_key(
        &self,
        scope: &SecretScope,
        previous: Option<PreviousSecretReference>,
    ) {
        let Some(previous) = previous else {
            return;
        };
        match self
            .dependencies
            .secret_inspector
            .inspect(scope, &previous.reference)
            .await
        {
            Ok(metadata) => {
                if let Err(error) = self
                    .dependencies
                    .secret_remover
                    .unset(scope, &previous.reference, metadata.expected_state())
                    .await
                {
                    tracing::warn!(%error, "expired gate session key cleanup failed");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "expired gate session key could not be inspected");
            }
        }
    }
}
