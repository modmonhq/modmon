// What runs on this host right now, and how it is stopped.
//
// Every request that changes the host is an operation for as long as it
// runs: `mm deploy status` and `mm doctor` list it, and `mm deploy cancel`,
// its deadline, and a client that went away stop it. Stopping is first
// asked of the operation where it waits on the engine (`watch`): it then
// ends like a failed step, and its records say why. One that has not ended
// some seconds after its deadline is not asked: the thread `mm-deadlines`,
// which does not depend on the request runtime, drops it. Dropping it ends
// its child processes (their supervisor sees the requester go) and gives
// up its turn.

/// How long past its deadline an operation may take to end by itself
/// before it is dropped.
const DEADLINE_GRACE: Duration = Duration::from_secs(5);
/// How often the deadlines are looked at.
const DEADLINES_LOOK: Duration = Duration::from_millis(500);
/// How long a rollout step runs on without a word from its client before it
/// is stopped as if the client had gone: twice the lease of the lock. The
/// client says it is still there every few seconds; a connection that
/// dropped without SSH noticing on either side says nothing.
const CLIENT_SILENCE_MS: i64 = 2 * crate::domain::rollout::LOCK_LEASE_MS;

/// Why an operation is being stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stop {
    Cancelled { by: String, at_unix_ms: i64 },
    ClientGone,
    /// A rollout step whose client was not heard from for
    /// `CLIENT_SILENCE_MS`.
    ClientSilent,
    Deadline,
}

impl Stop {
    fn reason(&self) -> String {
        match self {
            Self::Cancelled { by, at_unix_ms } => format!(
                "cancelled by {by} at {}",
                crate::domain::rollout::utc_time(*at_unix_ms)
            ),
            Self::ClientGone => "stopped because the client that started it went away".to_owned(),
            Self::ClientSilent => format!(
                "stopped because the client that started it was not heard from for {} s",
                CLIENT_SILENCE_MS / 1_000
            ),
            Self::Deadline => "stopped because it passed its deadline".to_owned(),
        }
    }

    /// The code of the error the operation ends with.
    const fn error_code(&self) -> &'static str {
        match self {
            Self::Deadline => "MM_TIMEOUT",
            Self::Cancelled { .. } | Self::ClientGone | Self::ClientSilent => "MM_CANCELLED",
        }
    }

    /// Whether the environment is free once the operation stopped: nobody
    /// is left to renew its lock. Not after a cancellation, which frees it
    /// once it has undone what the operation changed, nor after a deadline,
    /// whose client is still there to roll the step back.
    const fn frees_environment(&self) -> bool {
        matches!(self, Self::ClientGone | Self::ClientSilent)
    }

    const fn is_cancellation(&self) -> bool {
        matches!(self, Self::Cancelled { .. })
    }

    /// The error the operation ends with.
    fn error(&self, operation: &crate::domain::rollout::OperationInFlight) -> MmError {
        let what = format!("{} {}", operation.what, operation.short_id());
        match self {
            Self::Deadline => MmError::Timeout {
                message: format!(
                    "{what} on this host passed its deadline ({} s after it started) and was stopped; its child processes were ended. `mm deploy status` shows what the host runs now",
                    operation
                        .deadline_unix_ms
                        .saturating_sub(operation.started_at_unix_ms)
                        / 1_000
                ),
            },
            Self::Cancelled { .. } | Self::ClientGone | Self::ClientSilent => MmError::Cancelled {
                message: format!(
                    "{what} was {}; its child processes were ended",
                    self.reason()
                ),
            },
        }
    }
}

/// One operation in flight.
struct Running {
    info: std::sync::Mutex<crate::domain::rollout::OperationInFlight>,
    project: crate::domain::workspace::ProjectId,
    environment: String,
    stop: tokio::sync::watch::Sender<Option<Stop>>,
    abort: futures_util::future::AbortHandle,
    /// For a rollout step, the plan whose client says every few seconds
    /// that it is still there.
    rollout: Option<String>,
    /// The job runs it may start, which a cancellation stops with it.
    started_runs: Option<crate::domain::scheduler::StartedRuns>,
}

impl Running {
    fn info(&self) -> crate::domain::rollout::OperationInFlight {
        self.info
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The operation is at work: the loop that waits for it looked.
    fn touch(&self) {
        if let Ok(now) = now_unix_ms() {
            self.info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .alive_at_unix_ms = now;
        }
    }

    /// Asks the operation to stop at its next wait on the engine. The
    /// first reason given is the one it keeps.
    fn ask_to_stop(&self, stop: Stop) {
        let reason = stop.reason();
        let asked = self.stop.send_if_modified(|current| {
            if current.is_some() {
                return false;
            }
            *current = Some(stop);
            true
        });
        if asked {
            self.info
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .stopping = Some(reason);
        }
    }

    /// Why it is being stopped, once it is.
    fn stopping(&self) -> Option<Stop> {
        self.stop.borrow().clone()
    }

    /// Completes when the operation is asked to stop, with the reason.
    async fn stopped(&self) -> Stop {
        let mut asked = self.stop.subscribe();
        loop {
            if let Some(stop) = asked.borrow_and_update().clone() {
                return stop;
            }
            if asked.changed().await.is_err() {
                // The sender lives as long as this value: never here.
                std::future::pending::<()>().await;
            }
        }
    }
}

/// The operations in flight on this host.
#[derive(Default, Clone)]
struct Operations {
    running: Arc<std::sync::Mutex<Vec<Arc<Running>>>>,
}

/// An operation's place in the list; it leaves the list when dropped.
struct OperationEntry {
    running: Arc<Running>,
    list: Arc<std::sync::Mutex<Vec<Arc<Running>>>>,
}

impl Drop for OperationEntry {
    fn drop(&mut self) {
        self.list
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|running| !Arc::ptr_eq(running, &self.running));
    }
}

impl Operations {
    fn list(&self) -> Vec<Arc<Running>> {
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Puts a request that just got its turn on the list.
    fn start(
        &self,
        context: &RemoteRequestContext,
        method: &RemoteMethod,
        abort: futures_util::future::AbortHandle,
    ) -> Result<OperationEntry, MmError> {
        let now = now_unix_ms()?;
        let running = Arc::new(Running {
            info: std::sync::Mutex::new(crate::domain::rollout::OperationInFlight {
                id: method.operation_id(context.request_id),
                what: operation_name(method),
                principal: context.principal.clone(),
                started_at_unix_ms: now,
                alive_at_unix_ms: now,
                deadline_unix_ms: context.deadline_unix_ms,
                stopping: None,
            }),
            project: context.scope.project_id,
            environment: context.scope.environment.clone(),
            stop: tokio::sync::watch::channel(None).0,
            abort,
            rollout: match method {
                RemoteMethod::Rollout { plan_hash, .. } => Some(plan_hash.clone()),
                _ => None,
            },
            started_runs: started_runs(context, method),
        });
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::clone(&running));
        Ok(OperationEntry {
            running,
            list: Arc::clone(&self.running),
        })
    }

    /// Puts work of the daemon itself that changes the host, such as the
    /// autoscaler starting a container, on the list: `mm deploy status`
    /// shows it, and `mm deploy cancel` and its deadline stop it.
    fn start_own(
        &self,
        scope: &crate::domain::remote::RemoteScope,
        (what, principal): (String, &str),
        deadline_unix_ms: i64,
        abort: futures_util::future::AbortHandle,
    ) -> Result<OperationEntry, MmError> {
        let now = now_unix_ms()?;
        let running = Arc::new(Running {
            info: std::sync::Mutex::new(crate::domain::rollout::OperationInFlight {
                id: uuid::Uuid::new_v4().to_string(),
                what,
                principal: principal.to_owned(),
                started_at_unix_ms: now,
                alive_at_unix_ms: now,
                deadline_unix_ms,
                stopping: None,
            }),
            project: scope.project_id,
            environment: scope.environment.clone(),
            stop: tokio::sync::watch::channel(None).0,
            abort,
            rollout: None,
            started_runs: None,
        });
        self.running
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::clone(&running));
        Ok(OperationEntry {
            running,
            list: Arc::clone(&self.running),
        })
    }

    /// What runs for one environment.
    fn of(
        &self,
        project: crate::domain::workspace::ProjectId,
        environment: &str,
    ) -> Vec<Arc<Running>> {
        self.list()
            .into_iter()
            .filter(|running| running.project == project && running.environment == environment)
            .collect()
    }

    /// Drops every operation that is past its deadline by more than the
    /// grace, as of `now_unix_ms`. Returns how many.
    fn reap(&self, now_unix_ms: i64) -> usize {
        let late = self
            .list()
            .into_iter()
            .filter(|running| {
                let deadline = running.info().deadline_unix_ms;
                now_unix_ms
                    > deadline.saturating_add(
                        i64::try_from(DEADLINE_GRACE.as_millis()).unwrap_or(i64::MAX),
                    )
            })
            .collect::<Vec<_>>();
        for running in &late {
            let info = running.info();
            tracing::error!(
                operation = %info.id,
                what = %info.what,
                principal = %info.principal,
                "an operation did not end by its deadline; it is dropped and its child processes are ended"
            );
            running.ask_to_stop(Stop::Deadline);
            running.abort.abort();
        }
        late.len()
    }

    /// Starts the thread that drops operations past their deadline. It
    /// ends with the last copy of the list.
    fn watch_deadlines(&self) {
        let list = Arc::downgrade(&self.running);
        let started = std::thread::Builder::new()
            .name("mm-deadlines".to_owned())
            .spawn(move || {
                while let Some(running) = list.upgrade() {
                    let operations = Operations { running };
                    if let Ok(now) = now_unix_ms() {
                        operations.reap(now);
                    }
                    drop(operations);
                    std::thread::sleep(DEADLINES_LOOK);
                }
            });
        if let Err(error) = started {
            tracing::error!(%error, "could not start the thread that enforces operation deadlines");
        }
    }
}

/// The job runs an operation may start: the run a request asks for by hand
/// takes the request's ID, and a deployment's jobs name its plan.
fn started_runs(
    context: &RemoteRequestContext,
    method: &RemoteMethod,
) -> Option<crate::domain::scheduler::StartedRuns> {
    use crate::domain::scheduler::StartedRuns;
    match method {
        RemoteMethod::RunJob(_) => Some(StartedRuns::Run(context.request_id)),
        RemoteMethod::ApplyDeployment { remote_plan_hash: hash }
        | RemoteMethod::Rollout { plan_hash: hash, .. } => Some(StartedRuns::Deployment(hash.clone())),
        _ => None,
    }
}

impl RemoteDaemon {
    /// Stops a rollout step whose client was not heard from for
    /// `CLIENT_SILENCE_MS`: its connection dropped, and nothing told the
    /// relay.
    fn stop_if_client_silent(&self, scope: &crate::domain::remote::RemoteScope, running: &Running) {
        let Some(hash) = running.rollout.as_deref() else {
            return;
        };
        let (Ok(now), Ok(stored)) = (now_unix_ms(), self.access.deployments(scope.project_id, &scope.environment)) else {
            return;
        };
        let silent = stored
            .rollout
            .is_some_and(|rollout| rollout.plan_hash == hash && rollout.client_silent(now, CLIENT_SILENCE_MS));
        if silent && running.stopping().is_none() {
            let info = running.info();
            tracing::warn!(
                operation = %info.id,
                what = %info.what,
                principal = %info.principal,
                "the client of a rollout step was not heard from for {} s; the step is stopped",
                CLIENT_SILENCE_MS / 1_000
            );
            running.ask_to_stop(Stop::ClientSilent);
        }
    }

    /// The operations in flight on this host for one environment.
    fn operations_of(&self, scope: &crate::domain::remote::RemoteScope) -> Vec<crate::domain::rollout::OperationInFlight> {
        self.operations
            .of(scope.project_id, &scope.environment)
            .iter()
            .map(|running| running.info())
            .collect()
    }

    /// Every operation in flight on this host.
    fn all_operations(&self) -> Vec<crate::domain::rollout::OperationInFlight> {
        self.operations
            .list()
            .iter()
            .map(|running| running.info())
            .collect()
    }
}

/// How long the deployment status waits for the job runs.
const JOB_RUNS_PATIENCE: Duration = Duration::from_secs(5);

impl RemoteDaemon {
    /// The environment's job runs that no operation stands for.
    async fn running_job_runs(
        &self,
        context: &RemoteRequestContext,
        operations: &[crate::domain::rollout::OperationInFlight],
    ) -> Vec<crate::domain::rollout::OperationInFlight> {
        let scope = &context.scope;
        let deployed = match self.access.deployed(scope.project_id, &scope.environment) {
            Ok(Some(deployed)) => deployed,
            Ok(None) => return Vec::new(),
            Err(error) => {
                tracing::warn!(%error, "the job runs are not listed in the deployment status");
                return Vec::new();
            }
        };
        let mut reading = context.clone();
        reading.project_root = deployed.root;
        let runs = tokio::time::timeout(JOB_RUNS_PATIENCE, self.use_cases.running_job_runs(&reading)).await;
        match runs {
            Ok(Ok(runs)) => runs
                .into_iter()
                .filter(|run| operations.iter().all(|operation| operation.id != run.id))
                .collect(),
            Ok(Err(error)) => {
                tracing::warn!(%error, "the job runs are not listed in the deployment status");
                Vec::new()
            }
            Err(_) => {
                tracing::warn!("the job runs were not read in time for the deployment status");
                Vec::new()
            }
        }
    }
}
