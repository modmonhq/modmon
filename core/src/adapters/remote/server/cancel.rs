// `mm deploy cancel`: what runs for an environment on this host stops, a
// deployment that changed the host returns to what it ran before, and the
// environment is free. Anyone who may deploy to the environment may
// cancel. The audit log records who started what was cancelled and who
// cancelled it.

/// How long a cancellation waits for what it stopped to end by itself,
/// before it drops it.
const CANCEL_PATIENCE: Duration = Duration::from_secs(10);
/// How long it waits for the environment's turn to undo what was stopped.
const UNDO_TURN_PATIENCE: Duration = Duration::from_secs(30);

impl RemoteDaemon {
    async fn cancel<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        wanted: Option<&str>,
        timeout: Duration,
    ) -> Result<(), MmError>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let scope = &context.scope;
        let names = |id: &str| wanted.is_none_or(|wanted| wanted.len() >= 6 && id.starts_with(wanted));
        let stop = Stop::Cancelled {
            by: context.principal.clone(),
            at_unix_ms: now_unix_ms()?,
        };
        let running = self
            .operations
            .of(scope.project_id, &scope.environment)
            .into_iter()
            .filter(|running| names(&running.info().id))
            .collect::<Vec<_>>();
        let mut outcome = crate::domain::rollout::CancelOutcome {
            stopped: running.iter().map(|running| running.info()).collect(),
            ..Default::default()
        };
        for operation in &running {
            operation.ask_to_stop(stop.clone());
        }
        self.until_ended(&running, CANCEL_PATIENCE).await;
        for late in running.iter().filter(|operation| self.still_runs(operation)) {
            late.abort.abort();
        }
        self.until_ended(&running, CANCEL_PATIENCE).await;
        let deployment = self
            .access
            .deployments(scope.project_id, &scope.environment)?
            .rollout
            .filter(|rollout| {
                matches!(rollout.state, RolloutState::Held | RolloutState::Running)
                    && names(&rollout.plan_hash)
            });
        // The jobs they started stop before the host is returned to what it
        // ran before: a migration does not go on under the old version.
        let mut started = running
            .iter()
            .filter_map(|operation| operation.started_runs.clone())
            .collect::<Vec<_>>();
        started.extend(deployment.as_ref().map(|deployment| {
            crate::domain::scheduler::StartedRuns::Deployment(deployment.plan_hash.clone())
        }));
        if let Some(wanted) = wanted.filter(|_| running.is_empty() && deployment.is_none()) {
            started.push(crate::domain::scheduler::StartedRuns::Named(wanted.to_owned()));
        }
        started.dedup();
        match self.cancel_job_runs(context, &started, deployment.as_ref(), timeout).await {
            Ok(runs) => outcome.runs = runs,
            Err(error) => {
                outcome.problem = Some(format!(
                    "could not stop the job runs it started: {}. `mm job history` shows them",
                    bounded_message(&error.to_string())
                ));
            }
        }
        if let Some(deployment) = &deployment {
            // What was stopped left the environment to this cancellation:
            // free once it is undone, and free when that fails too.
            let undone = self
                .undo_cancelled(context, deployment, &stop, timeout)
                .await;
            if undone.is_err()
                && let Err(error) = self.free_cancelled(context, &deployment.plan_hash)
            {
                tracing::warn!(%error, "could not free the environment of a cancelled deployment");
            }
            match (undone, deployment.state) {
                (Ok(()), RolloutState::Held) => outcome.released = Some(deployment.plan_hash.clone()),
                (Ok(()), _) => outcome.rolled_back = Some(deployment.plan_hash.clone()),
                (Err(error), _) => {
                    let undone = format!(
                        "could not return to what it ran before: {}. `mm deploy rollback` tries again",
                        bounded_message(&error.to_string())
                    );
                    outcome.problem = Some(match outcome.problem.take() {
                        Some(runs) => format!("{runs}; {undone}"),
                        None => undone,
                    });
                }
            }
        }
        self.audit_cancelled(context, &outcome, deployment.as_ref())?;
        send_result(writer, context, RemoteResult::Cancelled(Box::new(outcome))).await
    }

    /// Stops the job runs of what was cancelled that have not ended, in
    /// the deployment's checkout or else the deployed one.
    async fn cancel_job_runs(
        &self,
        context: &RemoteRequestContext,
        started: &[crate::domain::scheduler::StartedRuns],
        deployment: Option<&crate::adapters::remote::access::StoredRollout>,
        timeout: Duration,
    ) -> Result<Vec<crate::domain::rollout::CancelledRun>, MmError> {
        if started.is_empty() {
            return Ok(Vec::new());
        }
        let scope = &context.scope;
        let section = deployment.and_then(|deployment| self.rollout_section(scope, &deployment.plan_hash).ok());
        let root = match section {
            Some(section) => Some(section.root),
            None => self.access.deployed(scope.project_id, &scope.environment)?.map(|deployed| deployed.root),
        };
        let Some(root) = root else {
            return Ok(Vec::new());
        };
        let mut stopping = context.clone();
        stopping.project_root = root;
        let runs = timed(timeout, self.use_cases.cancel_job_runs(&stopping, started)).await?;
        Ok(runs
            .into_iter()
            .map(|run| crate::domain::rollout::CancelledRun { job: run.job, run: run.id, state: run.state })
            .collect())
    }

    fn still_runs(&self, operation: &Arc<Running>) -> bool {
        self.operations
            .list()
            .iter()
            .any(|listed| Arc::ptr_eq(listed, operation))
    }

    /// Waits until none of `operations` runs any more, at most `patience`.
    async fn until_ended(&self, operations: &[Arc<Running>], patience: Duration) {
        let until = tokio::time::Instant::now() + patience;
        while operations.iter().any(|operation| self.still_runs(operation))
            && tokio::time::Instant::now() < until
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Frees the environment from a deployment that was cancelled: one that
    /// changed nothing here yet gives up the lock, and one that began
    /// returns the host to what it ran before, as a failed rollout does.
    async fn undo_cancelled(
        &self,
        context: &RemoteRequestContext,
        deployment: &crate::adapters::remote::access::StoredRollout,
        stop: &Stop,
        timeout: Duration,
    ) -> Result<(), MmError> {
        let why = format!(
            "the deployment {} started was {}",
            deployment.started_by,
            stop.reason()
        );
        let turn = self
            .gate
            .enter(
                Holder {
                    principal: context.principal.clone(),
                    operation: "cancellation".to_owned(),
                    since_unix_ms: now_unix_ms()?,
                },
                UNDO_TURN_PATIENCE,
            )
            .await?;
        let hash = deployment.plan_hash.as_str();
        if deployment.state == RolloutState::Held {
            let scope = &context.scope;
            return self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
                if let Some(rollout) = stored
                    .rollout
                    .as_mut()
                    .filter(|rollout| rollout.plan_hash == hash && rollout.state == RolloutState::Held)
                {
                    stored.rollout = rollout.interrupted.take().map(|interrupted| *interrupted);
                }
                Ok(())
            });
        }
        let revision = self.access.load()?.policy.revision;
        let pass = MutationPass {
            _turn: Some(turn),
            revision: AtomicU64::new(revision),
            needs: vec![(method_capability(&RemoteMethod::Cancel { operation: None }).0, None)],
            operation: None,
            continues: None,
        };
        self.undo_rollout(context, hash, (timeout, &pass), Some(why)).await
    }

    /// Frees the environment from the cancelled deployment `hash` whose
    /// undo failed: nothing is left that would finish it.
    fn free_cancelled(&self, context: &RemoteRequestContext, hash: &str) -> Result<(), MmError> {
        let scope = &context.scope;
        let now = now_unix_ms()?;
        self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
            if let Some(rollout) = stored.rollout.as_mut().filter(|rollout| rollout.plan_hash == hash) {
                rollout.expires_at_unix_ms = rollout.expires_at_unix_ms.min(now);
            }
            Ok(())
        })
    }

    fn audit_cancelled(
        &self,
        context: &RemoteRequestContext,
        outcome: &crate::domain::rollout::CancelOutcome,
        deployment: Option<&crate::adapters::remote::access::StoredRollout>,
    ) -> Result<(), MmError> {
        if outcome.is_empty() {
            return Ok(());
        }
        let scope = &context.scope;
        self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
            "principal": context.principal, "principal_id": context.principal_id,
            "project_id": scope.project_id, "environment": scope.environment,
            "target": scope.target, "node": scope.node, "result": "cancelled",
            "cancelled_by": context.principal,
            "operations": outcome.stopped.iter().map(|operation| serde_json::json!({
                "id": operation.id, "what": operation.what, "started_by": operation.principal,
                "started_at_unix_ms": operation.started_at_unix_ms,
            })).collect::<Vec<_>>(),
            "plan_hash": deployment.map(|deployment| deployment.plan_hash.clone()),
            "started_by": deployment.map(|deployment| deployment.started_by.clone()),
            "job_runs": outcome.runs.iter().map(|run| serde_json::json!({
                "job": run.job, "run_id": run.run, "state": run.state,
            })).collect::<Vec<_>>(),
            "rolled_back": outcome.rolled_back.is_some(),
            "released": outcome.released.is_some(),
            "problem": outcome.problem,
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }))
    }
}
