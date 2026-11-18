// The node's autoscaler (`adapters::autoscale`) works through the daemon:
// a finished rollout tells it what to scale, a container it starts takes
// the daemon's turn like any request that changes the host, its changes
// go to the audit journal, and a status shows what it decided.

/// How long the autoscaler waits for the daemon's turn before it tries
/// again at its next step.
const SCALE_TURN: Duration = Duration::from_secs(5);
/// How long starting one container may take: the environment is compiled
/// and Compose starts it.
const SCALE_START: Duration = Duration::from_secs(10 * 60);

impl RemoteDaemon {
    /// The node's autoscaler, which runs beside the requests once
    /// [`RemoteDaemon::run_reporting`] serves.
    #[must_use]
    pub fn with_autoscale(mut self, autoscale: Arc<crate::adapters::autoscale::Autoscaler>) -> Self {
        self.autoscale = Some(autoscale);
        self
    }

    /// After a finished rollout: what the node's part of it scales, for the
    /// autoscaler. A failure here leaves the deployment as it is and is
    /// logged: the next deployment writes it again.
    fn keep_scaling(&self, scope: &RemoteScope, section: &RolloutSection) {
        let Some(autoscale) = &self.autoscale else {
            return;
        };
        // A node the deployment places nothing on scales nothing of it.
        let plan = section.plan.as_ref();
        let deployed = crate::adapters::autoscale::DeployedScaling {
            scope: scope.clone(),
            root: section.root.clone(),
            plan_hash: plan.map(|plan| plan.plan_hash.clone()).unwrap_or_default(),
            services: plan
                .and_then(|plan| plan.node.as_ref())
                .map(|node| node.scaled.clone())
                .unwrap_or_default(),
        };
        if let Err(error) = crate::adapters::autoscale::store::keep_deployed(autoscale.directory(), &deployed) {
            tracing::warn!(%error, environment = %scope.environment, "cannot keep what the deployment autoscales");
        }
    }

    /// Adds to a node's status what its autoscaler decided for each
    /// autoscaled service.
    fn add_scaling(&self, scope: &RemoteScope, snapshot: &mut crate::domain::workspace::WorkspaceSnapshot) {
        let Some(autoscale) = &self.autoscale else {
            return;
        };
        let Ok(deployed) = crate::adapters::autoscale::store::deployed(autoscale.directory()) else {
            return;
        };
        let Some(record) = deployed
            .into_iter()
            .find(|record| record.scope.project_id == scope.project_id && record.scope.environment == scope.environment)
        else {
            return;
        };
        for stack in &mut snapshot.stacks {
            for service in &mut stack.services {
                let scaled = record
                    .services
                    .iter()
                    .find(|scaled| scaled.stack == stack.name && scaled.service == service.name);
                if let Some(scaled) = scaled {
                    service.scale = autoscale.status(&scaled.key());
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl crate::adapters::autoscale::ScaleHost for RemoteDaemon {
    fn deploying(&self, scope: &RemoteScope) -> bool {
        let Ok(now) = now_unix_ms() else {
            return true;
        };
        match self.access.deployments(scope.project_id, &scope.environment) {
            Ok(stored) => stored.rollout.is_some_and(|rollout| rollout.holds_lock(now)),
            // Unknown: nothing changes until it is known.
            Err(_) => true,
        }
    }

    async fn start(
        &self,
        deployed: &crate::adapters::autoscale::DeployedScaling,
        service: &crate::domain::autoscale::ScaledService,
        count: u32,
    ) -> Result<(), MmError> {
        let holder = Holder {
            principal: "mmd autoscale".to_owned(),
            operation: format!("autoscale of service '{}'", service.service),
            since_unix_ms: now_unix_ms()?,
        };
        let what = holder.operation.clone();
        let _turn = self.gate.enter(holder, SCALE_TURN).await?;
        let starting = self.use_cases.scale_service(
            &deployed.scope,
            &deployed.root,
            &deployed.plan_hash,
            (&service.stack, &service.service),
            count,
        );
        // An operation like a request's for as long as it runs: listed by
        // `mm deploy status`, stopped by `mm deploy cancel` and its deadline.
        let (starting, abort) = futures_util::future::abortable(starting);
        let deadline = now_unix_ms()?
            .saturating_add(i64::try_from(SCALE_START.as_millis()).unwrap_or(i64::MAX));
        let listed =
            self.operations
                .start_own(&deployed.scope, (what, "mmd autoscale"), deadline, abort)?;
        let stopped = |why: String| MmError::Cancelled {
            message: format!("starting a container of '{}' {why}", service.service),
        };
        tokio::select! {
            started = tokio::time::timeout(SCALE_START, starting) => started
                .map_err(|_| MmError::RuntimeResponse {
                    message: format!(
                        "starting a container of '{}' took longer than {} s",
                        service.service,
                        SCALE_START.as_secs()
                    ),
                })?
                .map_err(|_| stopped("was stopped".to_owned()))?,
            stop = listed.running.stopped() => Err(stopped(format!("was {}", stop.reason()))),
        }
    }

    fn audit(
        &self,
        deployed: &crate::adapters::autoscale::DeployedScaling,
        service: &crate::domain::autoscale::ScaledService,
        change: &crate::domain::autoscale::ScaleChange,
    ) {
        let scope = &deployed.scope;
        let written = self.journal.audit(&serde_json::json!({
            "timestamp_unix_ms": change.at_unix_ms,
            "principal": "mmd autoscale",
            "project_id": scope.project_id, "environment": scope.environment,
            "target": scope.target, "node": scope.node, "plan_hash": deployed.plan_hash,
            "result": "autoscale",
            "stack": service.stack, "service": service.service,
            "from": change.from, "to": change.to, "reason": change.reason,
            "daemon_version": env!("CARGO_PKG_VERSION"),
        }));
        if let Err(error) = written {
            tracing::warn!(%error, "cannot write an autoscale change to the audit journal");
        }
    }
}
