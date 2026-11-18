
/// How long a restart waits for the daemon's turn before it tries again at
/// the next look.
const HEAL_TURN: Duration = Duration::from_secs(5);
/// How long one restart may take: the longest drain, the restart, and the
/// wait for the container to be healthy again.
const HEAL_RESTART: Duration = Duration::from_secs(25 * 60);
/// Who the audit journal and `mm deploy status` name for a restart.
const HEAL_PRINCIPAL: &str = "mmd health";

impl RemoteDaemon {
    /// The node's restarts of unhealthy containers, which run beside the
    /// requests once [`RemoteDaemon::run_reporting`] serves.
    #[must_use]
    pub fn with_healing(mut self, healing: Arc<crate::adapters::healing::Healer>) -> Self {
        self.healing = Some(healing);
        self
    }

    /// After a finished rollout: the services the node runs and what is
    /// done when one keeps failing its health check.
    fn keep_watching(&self, scope: &RemoteScope, section: &RolloutSection) {
        let Some(healing) = &self.healing else {
            return;
        };
        let node = section.plan.as_ref().and_then(|plan| plan.node.as_ref());
        let services = node
            .map(|node| {
                node.services
                    .iter()
                    .map(|placed| crate::adapters::healing::WatchedService {
                        stack: placed.stack.clone(),
                        service: placed.service.clone(),
                        restart: node.restart_policy(&placed.stack, &placed.service),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let watched = crate::adapters::healing::WatchedEnvironment {
            scope: scope.clone(),
            root: section.root.clone(),
            services,
        };
        if let Err(error) = crate::adapters::healing::store::keep_watched(healing.directory(), &watched) {
            tracing::warn!(%error, environment = %scope.environment, "cannot keep which services the node watches");
        }
    }

    /// Adds to a node's status the restarts of each service.
    fn add_restarts(&self, scope: &RemoteScope, snapshot: &mut crate::domain::workspace::WorkspaceSnapshot) {
        let Some(healing) = &self.healing else {
            return;
        };
        let node = self.config.node_env().ok().flatten().map(|node| node.name);
        for stack in &mut snapshot.stacks {
            for service in &mut stack.services {
                service.restarts = healing.restarts(scope, &stack.name, &service.name).map(|mut restarts| {
                    for event in restarts.restarts.iter_mut().chain(restarts.limited.iter_mut()) {
                        event.node.clone_from(&node);
                    }
                    restarts
                });
            }
        }
    }

    /// After `mm service restart`: the service starts again from no restart.
    fn forget_restarts(&self, scope: &RemoteScope, service: &str) {
        if let Some(healing) = &self.healing
            && let Err(error) = healing.forget(scope, service)
        {
            tracing::warn!(%error, service, "cannot forget the restarts of a service");
        }
    }

    /// A read of logs with the restarts mm made first, each as a line of
    /// the container it restarted.
    fn with_restart_lines(
        &self,
        context: &RemoteRequestContext,
        request: &crate::domain::observability::LogRequest,
        mut stream: LogStream,
        cancellation: OperationCancellation,
    ) -> LogStream {
        let Some(healing) = &self.healing else {
            return stream;
        };
        let scope = &context.scope;
        let events = healing.events(scope, request.selector.as_ref().map(|selector| selector.service.as_str()));
        if events.is_empty() {
            return stream;
        }
        let node = self.config.node_env().ok().flatten().map(|node| node.name);
        let records = events
            .into_iter()
            .filter(|(_, _, event, _)| {
                request.since_unix_ms.is_none_or(|since| event.at_unix_ms >= since)
                    && request.until_unix_ms.is_none_or(|until| event.at_unix_ms <= until)
            })
            .map(|(stack, service, event, limit)| crate::domain::observability::LogRecord {
                identity: crate::domain::runtime::ServiceIdentity {
                    project_id: scope.project_id,
                    environment: scope.environment.clone(),
                    stack,
                    module: None,
                    service,
                },
                sequence: 0,
                source: crate::domain::observability::LogSource::Runtime,
                node: node.clone(),
                container: Some(event.container.clone()),
                timestamp: chrono::DateTime::from_timestamp_millis(event.at_unix_ms)
                    .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)),
                message: crate::domain::healing::log_line(&event, limit),
                lossy_utf8: false,
                truncated: false,
                reconnect: 0,
            })
            .collect::<Vec<_>>();
        let (sender, receiver) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move {
            for record in records {
                if sender.send(Ok(record)).await.is_err() {
                    return;
                }
            }
            while let Some(item) = stream.recv().await {
                if sender.send(item).await.is_err() {
                    return;
                }
            }
        });
        LogStream::new(receiver, cancellation)
    }
}

#[async_trait::async_trait]
impl crate::adapters::healing::HealHost for RemoteDaemon {
    fn deploying(&self, scope: &RemoteScope) -> bool {
        crate::adapters::autoscale::ScaleHost::deploying(self, scope)
    }

    async fn restart(
        &self,
        watched: &crate::adapters::healing::WatchedEnvironment,
        service: &crate::adapters::healing::WatchedService,
        id: &str,
    ) -> Result<bool, MmError> {
        let holder = Holder {
            principal: HEAL_PRINCIPAL.to_owned(),
            operation: format!("restart of an unhealthy container of service '{}'", service.service),
            since_unix_ms: now_unix_ms()?,
        };
        let what = holder.operation.clone();
        let Ok(_turn) = self.gate.enter(holder, HEAL_TURN).await else {
            return Ok(false);
        };
        let request = crate::domain::observability::RestartRequest {
            selector: crate::domain::observability::ServiceSelector {
                stack: Some(service.stack.clone()),
                module: None,
                service: service.service.clone(),
                container: Some(id.to_owned()),
            },
            approval: crate::domain::observability::ApprovalContext {
                principal: HEAL_PRINCIPAL.to_owned(),
                source: "mmd".to_owned(),
                approved_high_risk: false,
            },
        };
        let restarting = self.use_cases.restart_at(&watched.scope, &watched.root, request);
        let (restarting, abort) = futures_util::future::abortable(restarting);
        let deadline = now_unix_ms()?
            .saturating_add(i64::try_from(HEAL_RESTART.as_millis()).unwrap_or(i64::MAX));
        let listed = self
            .operations
            .start_own(&watched.scope, (what, HEAL_PRINCIPAL), deadline, abort)?;
        let stopped = |why: String| MmError::Cancelled {
            message: format!("restarting a container of '{}' {why}", service.service),
        };
        tokio::select! {
            restarted = tokio::time::timeout(HEAL_RESTART, restarting) => restarted
                .map_err(|_| MmError::RuntimeResponse {
                    message: format!(
                        "restarting a container of '{}' took longer than {} s",
                        service.service,
                        HEAL_RESTART.as_secs()
                    ),
                })?
                .map_err(|_| stopped("was stopped".to_owned()))?
                .map(|_| true),
            stop = listed.running.stopped() => Err(stopped(format!("was {}", stop.reason()))),
        }
    }

    fn audit(
        &self,
        watched: &crate::adapters::healing::WatchedEnvironment,
        service: &crate::adapters::healing::WatchedService,
        event: &crate::domain::healing::RestartEvent,
        limit: bool,
    ) {
        let scope = &watched.scope;
        let written = self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": event.at_unix_ms,
            "principal": HEAL_PRINCIPAL,
            "project_id": scope.project_id, "environment": scope.environment,
            "target": scope.target, "node": scope.node,
            "result": if limit { "health-restart-limit" } else { "health-restart" },
            "code": limit.then_some(crate::domain::healing::RESTART_LIMIT),
            "stack": service.stack, "service": service.service,
            "container": event.container, "reason": event.reason,
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }));
        if let Err(error) = written {
            tracing::warn!(%error, "cannot write a restart to the audit journal");
        }
    }
}
