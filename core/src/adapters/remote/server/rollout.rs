// A deployment rolled out in steps the client orders, and what this host
// remembers of it. One client at a time deploys to an environment: it
// holds a lock on every host, says "still here" while it works, and a
// client that went away loses the lock a minute later. Between `begin` and
// `finish` the host's part is in progress; `mm deploy status` shows it.

use crate::adapters::remote::access::StoredRollout;
use crate::app::{StackOutcome, StackStep};
use crate::domain::rollout::{
    DeploymentRecord, LOCK_LEASE_MS, RolloutAnswer, RolloutState, RolloutStep, StepFailure,
};

/// This host's part of a plan that is being rolled out.
struct RolloutSection {
    binding: RemotePlanBinding,
    /// `None` on a cluster node the plan places nothing on.
    plan: Option<crate::domain::deployment::DeploymentPlan>,
    receipt: Option<BuildReceipt>,
    release: Option<crate::domain::release_facts::ReleaseFacts>,
    root: std::path::PathBuf,
    staged: Vec<crate::domain::staged::StagedPlanItem>,
}

impl RolloutSection {
    fn of(daemon: &RemoteDaemon, remote: RemoteDeploymentPlan) -> Self {
        let binding = remote.binding;
        Self {
            root: daemon.repositories.checkout_path(
                binding.project_id,
                &binding.environment,
                &binding.commit,
            ),
            binding,
            plan: remote.plan,
            receipt: remote.receipt,
            staged: remote.staged,
            release: remote.release,
        }
    }

    fn context(&self, context: &RemoteRequestContext) -> RemoteRequestContext {
        let mut rolling = context.clone();
        rolling.project_root.clone_from(&self.root);
        rolling.release.clone_from(&self.release);
        rolling
    }
}

/// Whether a step was stopped because its client went away or went
/// silent: then it frees the environment at once, since nobody is left to
/// renew the lock. A cancelled step leaves it to the cancellation, which
/// frees it once it has undone the step.
fn nobody_renews(pass: &MutationPass) -> bool {
    pass.running()
        .and_then(|running| running.stopping())
        .is_some_and(|stop| stop.frees_environment())
}

/// Whether a step was asked to stop, for whatever reason.
fn was_stopped(pass: &MutationPass) -> bool {
    pass.running().is_some_and(|running| running.stopping().is_some())
}

fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

/// Why another client cannot deploy now: who is, since when.
fn deploy_running(running: &StoredRollout, environment: &str, now: i64) -> MmError {
    MmError::Remote {
        code: "MM_DEPLOY_RUNNING",
        message: format!(
            "{} started deploying plan {} to '{environment}' at {} and is still at it (this host heard of it {} s ago). Wait for it to finish; `mm deploy status` shows where it is, and anyone who may deploy here stops it with `mm deploy cancel {}`",
            running.started_by,
            short_hash(&running.plan_hash),
            crate::domain::rollout::utc_time(running.started_at_unix_ms),
            now.saturating_sub(running.heard_at_unix_ms).max(0) / 1_000,
            short_hash(&running.plan_hash),
        ),
    }
}

/// A failure on the host, for the client: its own code, bounded text.
fn host_failure(context: &RemoteRequestContext, step: &str, error: MmError) -> MmError {
    tracing::error!(
        request_id = %context.request_id,
        environment = %context.scope.environment,
        code = error.code(),
        %error,
        step,
        "rollout step failed"
    );
    MmError::Remote {
        code: error.code(),
        message: bounded_message(&redacted_error_message(&error)),
    }
}

impl RemoteDaemon {
    async fn rollout<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
        timeout: Duration,
        (hash, step): (&str, RolloutStep),
        (plan_authorization, pass): (Option<&Authorization>, Option<&MutationPass>),
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let passed = || pass.ok_or_else(|| MmError::State {
            message: "a rollout step ran without its turn".to_owned(),
        });
        let name = step.name();
        let answer = match &step {
            RolloutStep::Hold => self.hold(context, hash)?,
            RolloutStep::Release => self.release(context, hash)?,
            RolloutStep::Networks => {
                let pass = passed()?;
                self.holder(context, hash)?;
                let stored = self.store_sealed_networks(context, hash, true);
                self.watched(context, pass, timeout, stored).await?;
                RolloutAnswer::Addressed
            }
            RolloutStep::Storage => {
                let pass = passed()?;
                self.holder(context, hash)?;
                self.ready_storage(context, hash, timeout, pass).await?
            }
            RolloutStep::Pull => self.pull_rollout(context, hash, timeout, passed()?).await?,
            RolloutStep::Begin => {
                let authorization = plan_authorization.ok_or_else(|| MmError::State {
                    message: "the rollout's plan authorization was not checked".to_owned(),
                })?;
                self.begin_rollout(context, hash, timeout, authorization, passed()?).await?
            }
            RolloutStep::Start { stack } => {
                self.roll_stack(context, hash, (stack, StackStep::Start), (timeout, passed()?)).await?
            }
            RolloutStep::Apply { stack } => {
                self.roll_stack(context, hash, (stack, StackStep::Apply), (timeout, passed()?)).await?
            }
            RolloutStep::Ahead { stack } => {
                self.roll_stack(context, hash, (stack, StackStep::Ahead), (timeout, passed()?)).await?
            }
            RolloutStep::Job { stack, job } => {
                self.roll_job(context, hash, (stack, job), (timeout, passed()?)).await?
            }
            RolloutStep::Finish => self.finish_rollout(context, hash, timeout, passed()?).await?,
            RolloutStep::Rollback => self.rollback_rollout(context, hash, timeout, passed()?).await?,
        };
        if step.changes_host() {
            let scope = &context.scope;
            self.journal.audit(&serde_json::json!({
                "timestamp_unix_ms": now_unix_ms()?, "request_id": context.request_id,
                "principal": context.principal, "principal_id": context.principal_id,
                "project_id": scope.project_id, "environment": scope.environment,
                "target": scope.target, "node": scope.node, "plan_hash": hash,
                "result": format!("rollout-{name}"),
                "stack": match &step {
                    RolloutStep::Start { stack } | RolloutStep::Apply { stack } | RolloutStep::Ahead { stack } | RolloutStep::Job { stack, .. } => Some(stack),
                    _ => None,
                },
                "healthy": !matches!(answer, RolloutAnswer::Failed(_)),
                "daemon_version": env!("CARGO_PKG_VERSION"),
            }))?;
        }
        send_result(writer, context, RemoteResult::Rollout(Box::new(answer))).await
    }

    /// Takes the environment's deployment lock for plan `hash`, or renews
    /// it. Refused while another plan's client holds it.
    fn hold(&self, context: &RemoteRequestContext, hash: &str) -> Result<RolloutAnswer, MmError> {
        let scope = &context.scope;
        let now = now_unix_ms()?;
        self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
            if let Some(rollout) = stored.rollout.as_mut().filter(|rollout| rollout.plan_hash == hash) {
                require_holder(rollout, context)?;
                if matches!(rollout.state, RolloutState::Held | RolloutState::Running) {
                    // The client is still there: a daemon restart did
                    // not end its rollout after all.
                    rollout.interrupted_at_unix_ms = None;
                    rollout.heard_at_unix_ms = now;
                    rollout.expires_at_unix_ms =
                        rollout.expires_at_unix_ms.max(now.saturating_add(LOCK_LEASE_MS));
                }
                return Ok(());
            }
            if let Some(running) = stored.rollout.as_ref().filter(|rollout| rollout.holds_lock(now)) {
                return Err(deploy_running(running, &scope.environment, now));
            }
            // Only a plan this caller made and nobody applied takes the lock.
            let node_hash = self.sealed_section(scope, hash)?.unwrap_or_else(|| hash.to_owned());
            let plan = self.journal.read_plan(&node_hash, scope, now)?;
            let binding = plan.binding();
            if binding.kind != RemotePlanKind::Deployment || binding.principal_id != context.principal_id {
                return Err(MmError::Security {
                    message: "only the principal that made a deployment plan can roll it out; produce a new plan".to_owned(),
                });
            }
            // A rollout that was left unfinished here stays on record
            // until this one begins and so undoes it.
            let interrupted = stored.rollout.take().and_then(|old| match old.state {
                RolloutState::Running => Some(Box::new(StoredRollout { interrupted: None, ..old })),
                _ => old.interrupted,
            });
            stored.rollout = Some(StoredRollout {
                plan_hash: hash.to_owned(),
                commit: binding.commit.clone(),
                principal_id: context.principal_id,
                started_by: context.principal.clone(),
                started_at_unix_ms: now,
                heard_at_unix_ms: now,
                expires_at_unix_ms: now.saturating_add(LOCK_LEASE_MS),
                state: RolloutState::Held,
                stacks: Vec::new(),
                failure: None,
                interrupted,
                interrupted_at_unix_ms: None,
                changes: Some(crate::domain::rollout::RolloutChanges::default()),
                begun_under: None,
            });
            Ok(())
        })?;
        Ok(RolloutAnswer::Held)
    }

    /// Gives up the lock of plan `hash`. A host that began stays as it is.
    fn release(&self, context: &RemoteRequestContext, hash: &str) -> Result<RolloutAnswer, MmError> {
        let scope = &context.scope;
        let now = now_unix_ms()?;
        self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
            let Some(rollout) = stored.rollout.as_mut().filter(|rollout| rollout.plan_hash == hash) else {
                return Ok(());
            };
            require_holder(rollout, context)?;
            match rollout.state {
                RolloutState::Held => {
                    stored.rollout = rollout.interrupted.take().map(|interrupted| *interrupted);
                }
                RolloutState::Running => rollout.expires_at_unix_ms = now,
                RolloutState::Finished | RolloutState::RolledBack => {}
            }
            Ok(())
        })?;
        Ok(RolloutAnswer::Released)
    }

    /// Refuses a one-call apply while a rollout holds the environment.
    fn refuse_while_rolling(&self, scope: &RemoteScope) -> Result<(), MmError> {
        let now = now_unix_ms()?;
        match self.access.deployments(scope.project_id, &scope.environment)?.rollout {
            Some(running) if running.holds_lock(now) => {
                Err(deploy_running(&running, &scope.environment, now))
            }
            _ => Ok(()),
        }
    }

    /// The rollout of plan `hash`, which this caller must hold, begun or
    /// not.
    fn holder(&self, context: &RemoteRequestContext, hash: &str) -> Result<StoredRollout, MmError> {
        self.held_rollout(context, hash, RolloutState::Held)
            .or_else(|_| self.held_rollout(context, hash, RolloutState::Running))
    }

    /// The rollout of plan `hash`, which this caller must hold in `state`.
    fn held_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        state: RolloutState,
    ) -> Result<StoredRollout, MmError> {
        let scope = &context.scope;
        let rollout = self
            .access
            .deployments(scope.project_id, &scope.environment)?
            .rollout
            .filter(|rollout| rollout.plan_hash == hash && rollout.state == state)
            .ok_or_else(|| MmError::State {
                message: format!(
                    "plan {hash} {} on this host; run `mm deploy plan` again",
                    match state {
                        RolloutState::Held => "does not hold the deployment lock of the environment, or has begun already,",
                        _ => "is not being rolled out",
                    }
                ),
            })?;
        require_holder(&rollout, context)?;
        Ok(rollout)
    }

    /// Changes the record of the rollout of plan `hash`, if it is still
    /// the environment's.
    /// A step given up because access changed under it: the lock it held
    /// for its whole budget falls back to one lease, as after any step, for
    /// the client may no longer renew it or roll back (`mmd access block`
    /// of its key). Before, the environment stayed held until the step's
    /// deadline. Returns `error` for the caller to answer with.
    fn lease_ends(&self, context: &RemoteRequestContext, hash: &str, error: MmError) -> MmError {
        let ended = self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms =
                rollout.expires_at_unix_ms.min(now.saturating_add(LOCK_LEASE_MS));
        });
        if let Err(problem) = ended {
            tracing::warn!(%problem, "could not end the lease of a step given up for an access change");
        }
        error
    }

    fn note_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        change: impl FnOnce(&mut StoredRollout, i64),
    ) -> Result<(), MmError> {
        let scope = &context.scope;
        let now = now_unix_ms()?;
        self.access.update_deployments(scope.project_id, &scope.environment, |stored| {
            if let Some(rollout) = stored.rollout.as_mut().filter(|rollout| rollout.plan_hash == hash) {
                rollout.heard_at_unix_ms = now;
                rollout.interrupted_at_unix_ms = None;
                change(rollout, now);
            }
            Ok(())
        })
    }

    /// This host's section of plan `hash`, once `begin` claimed it.
    fn rollout_section(&self, scope: &RemoteScope, hash: &str) -> Result<RolloutSection, MmError> {
        let node_hash = self.sealed_section(scope, hash)?.unwrap_or_else(|| hash.to_owned());
        match self.journal.applied_plan(&node_hash, scope)? {
            Some(StoredRemotePlan::Deployment(remote)) => Ok(RolloutSection::of(self, *remote)),
            Some(StoredRemotePlan::TeamSync(_)) => Err(MmError::Security {
                message: format!("plan {hash} is not a deployment plan"),
            }),
            None => Err(MmError::State {
                message: format!("this host has not begun rolling out plan {hash}"),
            }),
        }
    }

    /// Claims this host's section, applies its access change (first, so
    /// revocations hold even if the stacks fail), stores the cluster's
    /// network addresses, and opens the deployment. No workload changes.
    async fn begin_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        timeout: Duration,
        plan_authorization: &Authorization,
        pass: &MutationPass,
    ) -> Result<RolloutAnswer, MmError> {
        let held = self.held_rollout(context, hash, RolloutState::Held)?;
        let reviewed_at = held.reviewed_at(now_unix_ms()?);
        let stored = self.claim_reviewed(
            context,
            (hash, RemotePlanKind::Deployment),
            (plan_authorization, reviewed_at),
        )?;
        let StoredRemotePlan::Deployment(remote) = stored else {
            return Err(MmError::Security {
                message: format!("plan {hash} is not a deployment plan"),
            });
        };
        let section = RolloutSection::of(self, *remote.clone());
        self.change_access(context, pass, &remote.access).await?;
        // The rest of this rollout stays its principal's to run under the
        // access it began under, also when this change lowered them
        // (`domain::rollout::BegunUnder`).
        let scope = &context.scope;
        let environment_access = self
            .access
            .load()?
            .environment_access(scope.project_id, &scope.environment);
        self.note_rollout(context, hash, |rollout, _| {
            rollout.begun_under = Some(crate::domain::rollout::BegunUnder {
                authorization: context.authorization.clone(),
                environment_access,
            });
        })?;
        let stored = self.store_sealed_networks(context, hash, section.plan.is_none());
        self.watched(context, pass, timeout, stored).await?;
        let begun = match &section.plan {
            Some(plan) => {
                let rolling = section.context(context);
                let begin = self.use_cases.begin_rollout(&rolling, &plan.plan_hash, section.receipt.as_ref());
                self.watched(context, pass, timeout, begin).await
            }
            None => Ok(()),
        };
        let failure = begun.as_ref().err().map(ToString::to_string);
        self.note_rollout(context, hash, |rollout, now| {
            rollout.interrupted = None;
            rollout.expires_at_unix_ms = now.saturating_add(LOCK_LEASE_MS);
            match failure {
                None => rollout.state = RolloutState::Running,
                Some(reason) => {
                    rollout.state = RolloutState::RolledBack;
                    rollout.failure = Some(StepFailure {
                        stack: String::new(),
                        service: None,
                        reason: bounded_message(&reason),
                        log: Vec::new(),
                        code: None,
                    });
                }
            }
        })?;
        begun.map_err(|error| host_failure(context, "begin", error))?;
        Ok(RolloutAnswer::Begun)
    }

    /// A job of the deployment, before its stack rolls out. A job that does
    /// not succeed is an answer, like a stack that is not healthy.
    async fn roll_job(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        (stack, job): (&str, &str),
        (timeout, pass): (Duration, &MutationPass),
    ) -> Result<RolloutAnswer, MmError> {
        self.held_rollout(context, hash, RolloutState::Running)?;
        let section = self.rollout_section(&context.scope, hash)?;
        let plan = section.plan.as_ref().ok_or_else(|| MmError::State {
            message: format!("plan {hash} places nothing on this host, so it runs no job here"),
        })?;
        let budget = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
        self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms = now.saturating_add(budget).saturating_add(LOCK_LEASE_MS);
        })?;
        let rolling = section.context(context);
        let running = self.use_cases.roll_job(&rolling, &plan.plan_hash, stack, job);
        let answer = match self.watched(context, pass, timeout, running).await {
            Err(error @ MmError::Security { .. }) => return Err(self.lease_ends(context, hash, error)),
            Ok(run) if run.state == crate::domain::scheduler::JobRunState::Succeeded => {
                RolloutAnswer::JobRan(Box::new(run))
            }
            Ok(run) => RolloutAnswer::Failed(crate::domain::rollout::job_failure(
                stack,
                &run,
                plan.rollout.failure_log_lines(),
            )),
            Err(error) => {
                let error = host_failure(context, "job", error);
                RolloutAnswer::Failed(StepFailure {
                    stack: stack.to_owned(),
                    service: None,
                    reason: format!("job '{job}' could not run: {error}"),
                    log: Vec::new(),
                    code: Some(error.code().to_owned()),
                })
            }
        };
        let nobody = nobody_renews(pass);
        self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms = if nobody {
                now
            } else {
                now.saturating_add(LOCK_LEASE_MS)
            };
            if let RolloutAnswer::Failed(failure) = &answer {
                rollout.failure = Some(failure.clone());
            }
        })?;
        Ok(answer)
    }

    /// One step of one stack. A stack that does not become healthy is an
    /// answer, not an error: the host stays as it is and the client
    /// decides what happens next.
    async fn roll_stack(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        (stack, step): (&str, StackStep),
        (timeout, pass): (Duration, &MutationPass),
    ) -> Result<RolloutAnswer, MmError> {
        self.held_rollout(context, hash, RolloutState::Running)?;
        let section = self.rollout_section(&context.scope, hash)?;
        let plan = section.plan.as_ref().ok_or_else(|| MmError::State {
            message: format!("plan {hash} places nothing on this host, so it has no stack '{stack}' here"),
        })?;
        // The lock outlives the step, however long the client lets it run.
        let budget = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
        let mut noted_before = false;
        self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms = now.saturating_add(budget).saturating_add(LOCK_LEASE_MS);
            if let Some(changes) = &mut rollout.changes {
                noted_before = changes.whole(stack);
                changes.note(stack, step == StackStep::Ahead);
            }
        })?;
        let rolling = section.context(context);
        let rolling_stack = self.use_cases.roll_stack(&rolling, &plan.plan_hash, stack, step);
        let rolled = self.watched(context, pass, timeout, rolling_stack).await;
        // A leave the step began goes on without it: taken back before the
        // environment is free, it cannot change the host under a plan made
        // after this step.
        if was_stopped(pass) {
            self.take_back_leaves(&rolling, (hash, &plan.plan_hash), stack).await;
        }
        let answer = match rolled {
            // Given up because access changed: a refusal, not a stack
            // that failed.
            Err(error @ MmError::Security { .. }) => return Err(self.lease_ends(context, hash, error)),
            Ok(StackOutcome::Done { started, unchanged, addresses }) => {
                if unchanged && !noted_before {
                    self.note_rollout(context, hash, |rollout, _| {
                        if let Some(changes) = &mut rollout.changes {
                            changes.forget(stack);
                        }
                    })?;
                }
                match step {
                    StackStep::Start => RolloutAnswer::Started { services: started, addresses },
                    StackStep::Apply | StackStep::Ahead => RolloutAnswer::Applied { addresses },
                }
            }
            Ok(StackOutcome::Failed(failure)) => RolloutAnswer::Failed(failure),
            Err(error) => {
                let error = host_failure(context, "stack", error);
                RolloutAnswer::Failed(StepFailure {
                    stack: stack.to_owned(),
                    service: None,
                    reason: error.to_string(),
                    log: Vec::new(),
                    code: Some(error.code().to_owned()),
                })
            }
        };
        let nobody = nobody_renews(pass);
        self.note_rollout(context, hash, |rollout, now| {
            rollout.expires_at_unix_ms = if nobody {
                now
            } else {
                now.saturating_add(LOCK_LEASE_MS)
            };
            match &answer {
                RolloutAnswer::Applied { .. } => rollout.stacks.push(stack.to_owned()),
                RolloutAnswer::Failed(failure) => rollout.failure = Some(failure.clone()),
                _ => {}
            }
        })?;
        Ok(answer)
    }

    /// After a stack step that was stopped: the containers it had begun to
    /// take out are routed to again, and a stop that had begun ends first.
    /// One that cannot be taken back is logged and goes on.
    async fn take_back_leaves(
        &self,
        rolling: &RemoteRequestContext,
        (hash, plan_hash): (&str, &str),
        stack: &str,
    ) {
        // A step stopped at its deadline has only the grace before it is
        // dropped (`DEADLINE_GRACE`).
        let patience = remaining(rolling).unwrap_or_default().max(Duration::from_secs(4));
        match timed(patience, self.use_cases.take_back_leaves(rolling, plan_hash, stack)).await {
            Ok(taken) if taken.is_empty() => {}
            Ok(taken) => tracing::warn!(
                plan = %hash, stack, containers = ?taken,
                "a stopped step had begun to take these containers out; they are routed to again"
            ),
            Err(error) => tracing::warn!(
                plan = %hash, stack, %error,
                "the containers a stopped step had begun to take out could not be taken back; their leave goes on"
            ),
        }
    }

    /// Every stack is healthy everywhere: removes what the plan no longer
    /// runs here, records the deployment, and gives up the lock.
    async fn finish_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        timeout: Duration,
        pass: &MutationPass,
    ) -> Result<RolloutAnswer, MmError> {
        let held = self.held_rollout(context, hash, RolloutState::Running)?;
        let section = self.rollout_section(&context.scope, hash)?;
        // Every stack is healthy everywhere: the routes go live before the
        // node closes its part. When they cannot, the node puts back what
        // the rollout changed here, as a rollback step does.
        let rolling = section.context(context);
        let served = self.serve_routes(&rolling, hash, timeout, pass).await;
        let finished = match (served, &section.plan) {
            (Ok(()), Some(plan)) => {
                let finishing = self.use_cases.finish_rollout(&rolling, &plan.plan_hash);
                self.watched(context, pass, timeout, finishing).await.map(Some)
            }
            (Ok(()), None) => Ok(None),
            (Err(error), Some(plan)) => {
                let undo = self.use_cases.rollback_rollout(&rolling, &plan.plan_hash, held.changes.as_ref());
                Err(match self.watched(context, pass, timeout, undo).await {
                    Ok(_) => error,
                    Err(undo) => MmError::State {
                        message: format!("{error}; returning this node to what it ran before failed too: {undo}"),
                    },
                })
            }
            (Err(error), None) => Err(error),
        };
        let mut result = match finished {
            Ok(result) => result,
            Err(error @ MmError::Security { .. }) => return Err(error),
            Err(error) => {
                // The engine returned the host to what it ran before.
                let reason = bounded_message(&redacted_error_message(&error));
                self.note_rollout(context, hash, |rollout, now| {
                    rollout.state = RolloutState::RolledBack;
                    rollout.expires_at_unix_ms = now;
                    rollout.failure = Some(StepFailure {
                        stack: String::new(),
                        service: None,
                        reason,
                        log: Vec::new(),
                        code: None,
                    });
                })?;
                return Err(host_failure(context, "finish", error));
            }
        };
        // A node still runs containers of the environment while a service
        // that moved away waits to be stopped.
        let runs_services = section
            .plan
            .as_ref()
            .is_some_and(|plan| plan.node.as_ref().is_none_or(|node| node.keeps_containers()));
        self.record_deployed(
            context,
            &section.binding,
            section.root.clone(),
            (runs_services, hash, section.receipt.as_ref(), section.release.as_ref()),
        )
        .await?;
        self.go_live(&section.context(context), &section.staged).await;
        self.keep_scaling(&context.scope, &section);
        self.keep_watching(&context.scope, &section);
        self.note_rollout(context, hash, |rollout, now| {
            rollout.state = RolloutState::Finished;
            rollout.expires_at_unix_ms = now;
        })?;
        // Every node runs the new stacks: networks no service of the
        // environment uses any more leave this node.
        self.retire_networks(context, hash, (timeout, pass)).await;
        // What the deployment does once it is in place here: its jobs, and
        // the records of the `once` jobs the cluster plan says ran.
        let cluster = self.journal.read_seal(hash)?.and_then(|seal| seal.rollout);
        if let (Some(result), Some(plan)) = (result.as_mut(), section.plan.as_ref()) {
            let rolling = section.context(context);
            result.jobs = self.after_deployment(&rolling, plan, cluster.as_ref(), (timeout, pass)).await?;
        } else {
            self.keep_once_records(context, hash, cluster.as_ref())?;
        }
        Ok(RolloutAnswer::Finished(result.map(Box::new)))
    }

    /// Returns the host to what it ran before plan `hash` began here, and
    /// gives up the lock. A host that never began only gives up the lock.
    async fn rollback_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        timeout: Duration,
        pass: &MutationPass,
    ) -> Result<RolloutAnswer, MmError> {
        let scope = &context.scope;
        let rollout = self
            .access
            .deployments(scope.project_id, &scope.environment)?
            .rollout
            .filter(|rollout| rollout.plan_hash == hash);
        let Some(rollout) = rollout else {
            return Ok(RolloutAnswer::RolledBack);
        };
        require_holder(&rollout, context)?;
        match rollout.state {
            RolloutState::Running => {
                self.undo_rollout(context, hash, (timeout, pass), None)
                    .await
                    .map_err(|error| host_failure(context, "rollback", error))?;
            }
            // Nothing changed here: the node only gives up the lock, and
            // says so, so that the client does not count it as rolled back.
            RolloutState::Held => {
                self.release(context, hash)?;
                return Ok(RolloutAnswer::Released);
            }
            RolloutState::Finished | RolloutState::RolledBack => {}
        }
        Ok(RolloutAnswer::RolledBack)
    }

    /// Returns the host to what it ran before the rollout of plan `hash`
    /// began here, and frees the environment. `why`, when given, becomes
    /// the rollout's recorded failure (a cancellation).
    async fn undo_rollout(
        &self,
        context: &RemoteRequestContext,
        hash: &str,
        (timeout, pass): (Duration, &MutationPass),
        why: Option<String>,
    ) -> Result<(), MmError> {
        let section = self.rollout_section(&context.scope, hash)?;
        if let Some(plan) = &section.plan {
            let rolling = section.context(context);
            // Only what the rollout's steps changed here is put back.
            let changes = self
                .access
                .deployments(context.scope.project_id, &context.scope.environment)?
                .rollout
                .filter(|rollout| rollout.plan_hash == hash)
                .and_then(|rollout| rollout.changes);
            let undoing = self.use_cases.rollback_rollout(&rolling, &plan.plan_hash, changes.as_ref());
            self.watched(context, pass, timeout, undoing).await?;
        }
        self.note_rollout(context, hash, |rollout, now| {
            rollout.state = RolloutState::RolledBack;
            rollout.expires_at_unix_ms = now;
            if let Some(why) = why {
                let earlier = rollout.failure.take();
                rollout.failure = Some(StepFailure {
                    stack: String::new(),
                    service: None,
                    reason: match &earlier {
                        Some(failure) => format!("{why}; {failure}"),
                        None => why,
                    },
                    log: earlier.map(|failure| failure.log).unwrap_or_default(),
                    code: None,
                });
            }
        })
    }

    /// The deployments this host remembers for the environment.
    async fn deployment_status<W>(
        &self,
        writer: &mut W,
        context: &RemoteRequestContext,
    ) -> Result<(), MmError>
    where W: AsyncWrite + Unpin + ?Sized {
        let scope = &context.scope;
        let mut status = self
            .access
            .deployments(scope.project_id, &scope.environment)?
            .report(now_unix_ms()?);
        status.operations = self.operations_of(scope);
        status.operations.extend(self.running_job_runs(context, &status.operations).await);
        // A host deployed to before rollouts existed knows what it runs,
        // and nothing of what ran before.
        if status.current.is_none() {
            status.current = self
                .access
                .deployed(scope.project_id, &scope.environment)?
                .map(|deployed| DeploymentRecord {
                    commit: deployed.commit,
                    deployed_at_unix_ms: deployed.deployed_at_unix_ms,
                    deployed_by: deployed.deployed_by,
                    runs_services: deployed.runs_services,
                    plan_hash: None,
                    receipt: None,
                    release: None,
                });
        }
        send_result(writer, context, RemoteResult::DeploymentStatus(Box::new(status))).await
    }
}

/// Only the principal that started a rollout continues it.
fn require_holder(rollout: &StoredRollout, context: &RemoteRequestContext) -> Result<(), MmError> {
    if rollout.principal_id == context.principal_id {
        return Ok(());
    }
    Err(MmError::Security {
        message: format!(
            "{} started this rollout; only they can continue it",
            rollout.started_by
        ),
    })
}
