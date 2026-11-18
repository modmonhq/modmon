// How a request that changes the host is let in, and how it stays allowed
// while it runs.
//
// One such request at a time takes its turn at the gate; the others wait
// without stopping a thread, at most until their own deadline, and are then
// told who is in their way. A command in a container takes no turn: it
// changes nothing the host records.
//
// Access state is locked on disk only for short sections that never await:
// the check of a request's authorization before it starts, and each write
// of access state. So `mmd access block` run as root takes effect at once,
// also while a deployment runs. What a revoked principal could gain from
// that is closed here: while an operation waits on the engine, the daemon
// looks every `ACCESS_RECHECK` whether access changed, and when the
// principal could not start the same operation now, the operation is given
// up where it is.

use std::sync::atomic::{AtomicU64, Ordering};

/// How often a running operation looks whether access changed under it.
const ACCESS_RECHECK: Duration = Duration::from_secs(2);

/// How long before a request's own deadline it stops waiting for its
/// turn, so that the answer naming what is in its way still reaches a
/// client that stops listening at the deadline.
const TURN_MARGIN: Duration = Duration::from_secs(2);

/// What is changing the host, for a request that waits behind it.
#[derive(Debug, Clone)]
struct Holder {
    principal: String,
    operation: String,
    since_unix_ms: i64,
}

/// The turn every host-changing request takes.
#[derive(Default)]
struct MutationGate {
    turn: Arc<tokio::sync::Mutex<()>>,
    holder: Arc<std::sync::Mutex<Option<Holder>>>,
}

/// The gate, held. Dropping it lets the next request in.
struct Turn {
    _turn: tokio::sync::OwnedMutexGuard<()>,
    holder: Arc<std::sync::Mutex<Option<Holder>>>,
}

impl Drop for Turn {
    fn drop(&mut self) {
        *self.holder.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

impl MutationGate {
    /// Waits for the turn, at most `patience`.
    async fn enter(&self, holder: Holder, patience: Duration) -> Result<Turn, MmError> {
        let waiting = Arc::clone(&self.turn).lock_owned();
        let Ok(turn) = tokio::time::timeout(patience, waiting).await else {
            return Err(self.busy(patience));
        };
        *self.holder.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(holder);
        Ok(Turn { _turn: turn, holder: Arc::clone(&self.holder) })
    }

    /// Why a request did not get its turn: who has it, since when.
    fn busy(&self, waited: Duration) -> MmError {
        let holder = self.holder.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        let who = holder.map_or_else(
            || "Another operation is changing this host".to_owned(),
            |holder| {
                format!(
                    "{}'s {} has been changing this host since {}",
                    holder.principal,
                    holder.operation,
                    crate::domain::rollout::utc_time(holder.since_unix_ms)
                )
            },
        );
        MmError::Remote {
            code: "MM_HOST_BUSY",
            message: format!(
                "{who}. This request waited {} s for it and changed nothing. Run it again when that operation is done; `mm deploy status` shows a deployment in progress",
                waited.as_secs()
            ),
        }
    }
}

/// What lets a request change the host: its turn at the gate, and the
/// access state its principal was last checked against.
struct MutationPass {
    _turn: Option<Turn>,
    /// The policy revision the principal was last allowed under, or that
    /// this operation wrote itself.
    revision: AtomicU64,
    /// Everything the principal had to be allowed to start the operation.
    needs: Vec<(Capability, Option<String>)>,
    /// Its place among the operations in flight, for one that is listed.
    operation: Option<OperationEntry>,
    /// A step of a rollout let in under the access the rollout began under
    /// (`continued_rollout`), which a later look takes into account too.
    continues: Option<RemoteMethod>,
}

impl MutationPass {
    /// Takes over the revision this operation's own access change made.
    fn adopt(&self, revision: u64) {
        self.revision.store(revision, Ordering::SeqCst);
    }

    /// The operation in flight this pass belongs to.
    fn running(&self) -> Option<Arc<Running>> {
        self.operation.as_ref().map(|entry| Arc::clone(&entry.running))
    }

    /// Completes when the operation is asked to stop, with the error it
    /// ends with; never for a request that is no operation.
    async fn stopped(&self) -> MmError {
        match self.running() {
            Some(running) => {
                let stop = running.stopped().await;
                stop.error(&running.info())
            }
            None => std::future::pending().await,
        }
    }
}

/// What a holder of the gate is doing, in a waiting request's words.
fn operation_name(method: &RemoteMethod) -> String {
    use crate::domain::rollout::RolloutStep;
    match method {
        RemoteMethod::ApplyDeployment { .. } => "deployment".to_owned(),
        RemoteMethod::ApplyTeamSync { .. } => "access sync".to_owned(),
        RemoteMethod::FinishMoves { .. } => "deployment (stopping services that moved)".to_owned(),
        RemoteMethod::DeleteVolume { volume, .. } => format!("deletion of shared volume '{volume}'"),
        RemoteMethod::Rollout {
            step: RolloutStep::Start { stack } | RolloutStep::Apply { stack } | RolloutStep::Ahead { stack },
            ..
        } => format!("deployment (stack '{stack}')"),
        RemoteMethod::Rollout { step: RolloutStep::Job { job, .. }, .. } => format!("deployment (job '{job}')"),
        RemoteMethod::Rollout { step, .. } => format!("deployment (step {})", step.name()),
        RemoteMethod::Deliver(offer) => {
            format!("delivery of commit {}", offer.commit.get(..12).unwrap_or(&offer.commit))
        }
        RemoteMethod::Restart(request) => format!("restart of service '{}'", request.selector.service),
        RemoteMethod::RunJob(request) => format!("run of job '{}'", request.job),
        RemoteMethod::Cancel { .. } => "cancellation".to_owned(),
        RemoteMethod::StageValues(_) => "change of secrets and variables".to_owned(),
        RemoteMethod::StoreRestore(_) => "restore of the secret store".to_owned(),
        RemoteMethod::StoreMigrate { .. } => "move of secrets into the secret store".to_owned(),
        other => other.name().replace('_', " "),
    }
}

impl RemoteDaemon {
    /// For a request that changes the host: its turn, and under the access
    /// lock the check that its authorization is still the reviewed one,
    /// plus, for an apply, the capability its plan was made with. The lock
    /// is released before this returns.
    async fn enter_mutation(
        &self,
        context: &RemoteRequestContext,
        method: &RemoteMethod,
        (timeout, abort): (Duration, futures_util::future::AbortHandle),
    ) -> Result<Option<(MutationPass, Option<Authorization>)>, MmError> {
        if !method.is_mutation() {
            return Ok(None);
        }
        let turn = if !method.takes_turn() {
            None
        } else {
            let holder = Holder {
                principal: context.principal.clone(),
                operation: operation_name(method),
                since_unix_ms: now_unix_ms()?,
            };
            let patience = timeout.saturating_sub(TURN_MARGIN).max(timeout / 2);
            Some(self.gate.enter(holder, patience).await?)
        };
        let (capability, service) = method_capability(method);
        let mut needs = vec![(capability, service.map(str::to_owned))];
        let plan_capability = applied_plan_kind(method).map(plan_capability);
        needs.extend(plan_capability.map(|capability| (capability, None)));
        let mut continues = None;
        let (revision, plan_authorization) = self
            .access
            .locked("mmd, checking a request before it changes the host", |guard| {
                let current = self.reauthorize_with_policy(context, method, guard).or_else(|error| {
                    let begun = self.continued_rollout(
                        guard.snapshot(),
                        (&context.key_fingerprint, context.principal_id),
                        &context.scope,
                        method,
                    );
                    continues = begun.is_some().then(|| method.clone());
                    begun.ok_or(error)
                })?;
                if current.authorization_hash != context.authorization.authorization_hash {
                    return Err(MmError::Security {
                        message: "authorization changed after request review; submit a new request"
                            .to_owned(),
                    });
                }
                let plan_authorization = plan_capability
                    .map(|capability| {
                        self.reauthorize_capability_with_policy(context, capability, None, guard)
                    })
                    .transpose()?;
                Ok((guard.revision, plan_authorization))
            })
            .await?;
        let operation = method
            .is_operation()
            .then(|| self.operations.start(context, method, abort))
            .transpose()?;
        let pass = MutationPass {
            _turn: turn,
            revision: AtomicU64::new(revision),
            needs,
            operation,
            continues,
        };
        Ok(Some((pass, plan_authorization)))
    }

    /// Whether the principal of a running operation could start it under
    /// `policy`. Nothing is looked up while access state is what it was at
    /// the last check.
    fn still_allowed(
        &self,
        context: &RemoteRequestContext,
        pass: &MutationPass,
        snapshot: &crate::adapters::remote::access::AccessSnapshot,
    ) -> Result<(), MmError> {
        let policy = &snapshot.policy;
        if policy.revision == pass.revision.load(Ordering::SeqCst) {
            return Ok(());
        }
        let scope = &context.scope;
        let allowed = policy
            .authenticate(&context.key_fingerprint, scope.project_id, &scope.environment, now_unix_ms()?)
            .and_then(|principal| {
                if principal.id == context.principal_id {
                    Ok(())
                } else {
                    Err("the SSH key now belongs to another principal".to_owned())
                }
            })
            .and_then(|()| {
                pass.needs.iter().try_for_each(|(capability, service)| {
                    policy
                        .authorize(&AccessRequest {
                            principal_id: context.principal_id,
                            capability: *capability,
                            project_id: scope.project_id,
                            environment: &scope.environment,
                            service: service.as_deref(),
                            job: None,
                            gate: None,
                        })
                        .map(|_| ())
                })
            })
            .or_else(|reason| {
                pass.continues
                    .as_ref()
                    .and_then(|method| {
                        self.continued_rollout(
                            snapshot,
                            (&context.key_fingerprint, context.principal_id),
                            scope,
                            method,
                        )
                    })
                    .map(drop)
                    .ok_or(reason)
            });
        match allowed {
            Ok(()) => {
                pass.adopt(policy.revision);
                Ok(())
            }
            Err(reason) => {
                tracing::warn!(
                    request_id = %context.request_id,
                    principal = %context.principal,
                    environment = %scope.environment,
                    reason,
                    "access changed under a running operation; giving it up"
                );
                Err(MmError::Security {
                    message: format!(
                        "access to '{}' changed while this operation ran, and {} may no longer run it ({reason}). The operation was stopped where it was, so the host may be partly changed; `mm deploy status` shows what it runs",
                        scope.environment, context.principal
                    ),
                })
            }
        }
    }

    /// Runs the part of a host-changing operation that waits on the engine:
    /// bounded by `timeout`, and given up when access changes so that the
    /// principal could not start the operation now. Giving up drops the
    /// work, which ends its child processes.
    async fn watched<T: Send>(
        &self,
        context: &RemoteRequestContext,
        pass: &MutationPass,
        timeout: Duration,
        work: impl std::future::Future<Output = Result<T, MmError>> + Send,
    ) -> Result<T, MmError> {
        // The watch itself is one function for every kind of work.
        let mut outcome = None;
        let finished = Box::pin(async {
            outcome = Some(work.await);
        });
        self.watch(context, pass, timeout, finished).await?;
        outcome.ok_or_else(|| MmError::State {
            message: "a watched operation ended without an outcome".to_owned(),
        })?
    }

    /// Waits for `work`, looking at access state as it goes, and gives it
    /// up when its operation is asked to stop: cancelled, its client gone
    /// or silent, or past its deadline.
    async fn watch(
        &self,
        context: &RemoteRequestContext,
        pass: &MutationPass,
        timeout: Duration,
        mut work: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>>,
    ) -> Result<(), MmError> {
        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);
        let running = pass.running();
        let stopped = async {
            match &running {
                Some(running) => running.stopped().await,
                None => std::future::pending().await,
            }
        };
        tokio::pin!(stopped);
        let mut look = tokio::time::interval(ACCESS_RECHECK);
        look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                () = &mut work => return Ok(()),
                () = &mut deadline => {
                    return Err(match &running {
                        Some(running) => {
                            running.ask_to_stop(Stop::Deadline);
                            Stop::Deadline.error(&running.info())
                        }
                        None => deadline_passed(timeout),
                    });
                }
                stop = &mut stopped => {
                    let operation = running.as_ref().map(|running| running.info());
                    return Err(operation.map_or_else(|| deadline_passed(timeout), |operation| stop.error(&operation)));
                }
                _ = look.tick() => self.look_at(context, pass)?,
            }
        }
    }

    /// One look at a running operation from the loop that waits for it: it
    /// is at work, its client was heard from, and its principal may still
    /// run it.
    fn look_at(&self, context: &RemoteRequestContext, pass: &MutationPass) -> Result<(), MmError> {
        if let Some(running) = pass.running() {
            running.touch();
            self.stop_if_client_silent(&context.scope, &running);
        }
        self.recheck(context, pass)
    }

    /// One look at access state for a running operation. State that cannot
    /// be read right now refuses every new request; the running one is
    /// looked at again at the next tick.
    fn recheck(&self, context: &RemoteRequestContext, pass: &MutationPass) -> Result<(), MmError> {
        match self.access.load() {
            Ok(snapshot) => self.still_allowed(context, pass, &snapshot),
            Err(error) => {
                tracing::warn!(%error, "could not read access state for a running operation");
                Ok(())
            }
        }
    }

    /// Applies an operation's reviewed access change in one short section
    /// under the access lock, after checking that its principal may still
    /// run the operation. The operation continues under the state it wrote.
    async fn change_access(
        &self,
        context: &RemoteRequestContext,
        pass: &MutationPass,
        access: &AccessChange,
    ) -> Result<(), MmError> {
        let revision = self
            .access
            .locked("mmd, applying a reviewed access change", |guard| {
                self.still_allowed(context, pass, guard.snapshot())?;
                self.apply_access(&context.principal, guard, access)
            })
            .await?;
        pass.adopt(revision);
        Ok(())
    }
}

impl RemoteDaemon {
    /// The authorization a rollout began under, for one of its own steps
    /// (or its services' last stop, `FinishMoves`) that the principal who
    /// began it may no longer start under access as it is now: the
    /// rollout's own access change lowered them, and nothing else changed
    /// access to the environment since. `None` for anything else. The key
    /// must still authenticate as that principal.
    fn continued_rollout(
        &self,
        snapshot: &crate::adapters::remote::access::AccessSnapshot,
        (fingerprint, principal): (&str, crate::domain::access::PrincipalId),
        scope: &RemoteScope,
        method: &RemoteMethod,
    ) -> Option<Authorization> {
        let (plan_hash, begins) = match method {
            RemoteMethod::Rollout { plan_hash, step } => (plan_hash, matches!(step, RolloutStep::Begin)),
            RemoteMethod::FinishMoves { cluster_plan_hash } => (cluster_plan_hash, false),
            _ => return None,
        };
        let now = now_unix_ms().ok()?;
        let authenticated = snapshot
            .policy
            .authenticate(fingerprint, scope.project_id, &scope.environment, now)
            .ok()?;
        if authenticated.id != principal {
            return None;
        }
        let rollout = self.access.deployments(scope.project_id, &scope.environment).ok()?.rollout?;
        let begun = rollout.begun_under.as_ref()?;
        let request = crate::domain::rollout::RolloutRequest { plan_hash, principal, begins };
        let access = snapshot.environment_access(scope.project_id, &scope.environment);
        begun
            .lets((&rollout.plan_hash, rollout.principal_id), request, &access)
            .then(|| begun.authorization.clone())
    }
}
