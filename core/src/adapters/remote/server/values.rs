
impl RemoteDaemon {
    async fn values<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        method: RemoteMethod,
        pass: Option<&MutationPass>,
        timeout: Duration,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let result = match method {
            RemoteMethod::SecretRecipient => {
                RemoteResult::SecretRecipient(timed(timeout, self.use_cases.secret_recipient(context)).await?)
            }
            RemoteMethod::HostValues => {
                let mut values = timed(timeout, self.use_cases.host_values(context)).await?;
                let scope = &context.scope;
                if let Some(deployed) = self.access.deployed(scope.project_id, &scope.environment)? {
                    values.commit = deployed.commit;
                }
                RemoteResult::HostValues(Box::new(values))
            }
            RemoteMethod::StageValues(stage) => {
                let pass = pass.ok_or_else(|| MmError::State {
                    message: "a stage ran without its turn".to_owned(),
                })?;
                stage.validate().map_err(|message| MmError::Secret { message })?;
                if !matches!(stage.step, crate::domain::staged::StageStep::Revert) {
                    self.refuse_while_rolling(&context.scope)?;
                }
                let staging = self.use_cases.stage_values(context, *stage);
                let answer = self.watched(context, pass, timeout, staging).await?;
                self.audit_operation(context, "values-staged", serde_json::json!({
                    "stage": answer.stage, "state": answer.state, "changes": answer.changes,
                }))?;
                RemoteResult::Staged(Box::new(answer))
            }
            RemoteMethod::StoreSnapshot { recipient } => {
                let sealed = timed(timeout, self.use_cases.store_snapshot(context, &recipient)).await?;
                self.audit_operation(context, "store-snapshot", serde_json::json!({}))?;
                RemoteResult::StoreSnapshot(sealed)
            }
            RemoteMethod::StoreRestore(step) => {
                let pass = pass.ok_or_else(|| MmError::State {
                    message: "a restore ran without its turn".to_owned(),
                })?;
                let kind = match step.as_ref() {
                    crate::domain::secret_store::StoreRestoreStep::Snapshot { .. } => "snapshot",
                    crate::domain::secret_store::StoreRestoreStep::Keys { .. } => "keys",
                };
                let restoring = self.use_cases.store_restore(context, *step);
                let restored = self.watched(context, pass, timeout, restoring).await?;
                self.audit_operation(context, "store-restored", serde_json::json!({ "step": kind }))?;
                RemoteResult::StoreRestored(Box::new(restored))
            }
            RemoteMethod::StoreMigrate { provider } => {
                let pass = pass.ok_or_else(|| MmError::State {
                    message: "a move of secrets ran without its turn".to_owned(),
                })?;
                let moving = self.use_cases.store_migrate(context, &provider);
                let moved = self.watched(context, pass, timeout, moving).await?;
                let keys = moved.secrets.iter().filter(|secret| secret.written).map(|secret| &secret.key).collect::<Vec<_>>();
                self.audit_operation(context, "store-migrated", serde_json::json!({ "provider": provider, "written": keys }))?;
                RemoteResult::StoreMigrated(Box::new(moved))
            }
            _ => {
                return Err(MmError::State {
                    message: "this is no request about secrets or variables".to_owned(),
                });
            }
        };
        send_result(writer, context, result).await
    }

    /// After a deployment succeeded here: the staged changes its plan
    /// listed go live. A failure is logged; the deployment stands, and what
    /// stayed staged goes live with the next one.
    async fn go_live(
        &self,
        context: &RemoteRequestContext,
        staged: &[crate::domain::staged::StagedPlanItem],
    ) {
        if staged.is_empty() {
            return;
        }
        match self.use_cases.go_live(context, staged).await {
            Ok(()) => {
                let changes = staged
                    .iter()
                    .map(|item| serde_json::json!({
                        "key": item.key, "kind": item.kind, "change": item.change,
                        "revision": item.revision, "staged_by": item.staged_by,
                    }))
                    .collect::<Vec<_>>();
                if let Err(error) = self.audit_operation(context, "values-live", serde_json::json!({ "changes": changes })) {
                    tracing::warn!(%error, "could not audit the staged values that went live");
                }
            }
            Err(error) => {
                tracing::error!(code = error.code(), %error, "staged values did not go live");
            }
        }
    }
}
