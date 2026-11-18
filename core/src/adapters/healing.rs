//! A node's restarts of its containers that keep failing their health
//! check (`domain::healing`).
//!
//! Every few seconds it looks at the containers of each environment its last
//! deployments run here: one that has been unhealthy for its service's
//! `after` is restarted through the daemon the way `mm service restart`
//! does it, one container of a service at a time, never while a deployment
//! holds the environment or a container of the service leaves. Past its
//! limit a container is left alone and kept out of the answers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::adapters::discovery::NameResolver;
use crate::adapters::engine_api::EngineApi;
use crate::domain::error::MmError;
use crate::domain::healing::{HealthStep, RestartEvent, ServiceRestarts};
use crate::domain::remote::RemoteScope;

pub mod store;

pub use store::{WatchedEnvironment, WatchedService};

/// Where a node keeps what this needs, under the daemon's state directory.
pub const HEALTH_DIRECTORY: &str = "health";
const STEP: Duration = Duration::from_secs(5);
/// How long the engine may take to list an environment's containers.
const ENGINE_PATIENCE: Duration = Duration::from_secs(10);

/// What the restarts ask of the daemon they run in.
#[async_trait::async_trait]
pub trait HealHost: Send + Sync {
    /// Whether a deployment holds the environment of `scope` now.
    fn deploying(&self, scope: &RemoteScope) -> bool;

    /// Restarts the container `id` of a service as `mm service restart`
    /// does. `Ok(false)` when the daemon had no turn for it now.
    async fn restart(
        &self,
        watched: &WatchedEnvironment,
        service: &WatchedService,
        id: &str,
    ) -> Result<bool, MmError>;

    /// Writes a restart, or the end of restarts (`limit`), to the audit
    /// journal.
    fn audit(
        &self,
        watched: &WatchedEnvironment,
        service: &WatchedService,
        event: &RestartEvent,
        limit: bool,
    );
}

/// A container as the engine shows it.
#[derive(Debug, Clone)]
struct Seen {
    id: String,
    name: String,
    stack: String,
    service: String,
    unhealthy: bool,
}

impl Seen {
    fn from_inspected(inspected: &serde_json::Value) -> Option<Self> {
        let labels = &inspected["Config"]["Labels"];
        let label = |key: &str| labels[key].as_str().map(str::to_owned);
        if label("com.docker.compose.oneoff")
            .is_some_and(|oneoff| oneoff.eq_ignore_ascii_case("true"))
        {
            return None;
        }
        let state = &inspected["State"];
        Some(Self {
            id: inspected["Id"].as_str()?.to_owned(),
            name: inspected["Name"]
                .as_str()?
                .trim_start_matches('/')
                .to_owned(),
            stack: label(crate::domain::ownership::LABEL_STACK)?,
            service: label("com.docker.compose.service")?,
            unhealthy: state["Running"].as_bool().unwrap_or(false)
                && state["Health"]["Status"]
                    .as_str()
                    .is_some_and(|health| health.eq_ignore_ascii_case("unhealthy")),
        })
    }
}

pub struct Healer {
    directory: PathBuf,
    state_root: PathBuf,
    names: Arc<NameResolver>,
    engine: Option<EngineApi>,
    /// Since when each unhealthy container has been seen unhealthy.
    unhealthy: Mutex<BTreeMap<String, Instant>>,
    /// The services a restart runs for now.
    restarting: Mutex<BTreeSet<String>>,
    /// One change of the history files at a time.
    writing: Mutex<()>,
}

impl std::fmt::Debug for Healer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Healer")
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn service_key(scope: &RemoteScope, service: &WatchedService) -> String {
    format!(
        "{}/{}/{}/{}",
        scope.project_id.0, scope.environment, service.stack, service.service
    )
}

impl Healer {
    #[must_use]
    pub fn new(
        state_root: &Path,
        names: Arc<NameResolver>,
        engine: Option<EngineApi>,
    ) -> Arc<Self> {
        Arc::new(Self {
            directory: state_root.join(HEALTH_DIRECTORY),
            state_root: state_root.to_path_buf(),
            names,
            engine,
            unhealthy: Mutex::new(BTreeMap::new()),
            restarting: Mutex::new(BTreeSet::new()),
            writing: Mutex::new(()),
        })
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Looks again and again until the process ends.
    pub async fn run(self: Arc<Self>, host: Arc<dyn HealHost>) {
        let mut tick = tokio::time::interval(STEP);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            self.step(&host).await;
        }
    }

    /// One look at every watched environment of the node.
    pub async fn step(self: &Arc<Self>, host: &Arc<dyn HealHost>) {
        let watched = match store::watched(&self.directory) {
            Ok(watched) => watched,
            Err(error) => {
                tracing::warn!(%error, "cannot read which services the node watches");
                return;
            }
        };
        let leaving = crate::adapters::leaving::leaving_ids(&self.state_root)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let mut held = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for environment in &watched {
            if let Err(error) = self
                .look(host, environment, &leaving, (&mut held, &mut seen))
                .await
            {
                tracing::warn!(
                    environment = %environment.scope.environment,
                    %error,
                    "cannot look at the health of the environment's containers"
                );
            }
        }
        locked(&self.unhealthy).retain(|id, _| seen.contains(id));
        self.names.hold(held);
    }

    async fn containers(&self, scope: &RemoteScope) -> Result<Vec<Seen>, MmError> {
        let engine = self.engine.as_ref().ok_or_else(|| MmError::Environment {
            message: "the node's engine has no Unix socket this daemon can read: DOCKER_HOST or the Docker context of mmd's environment names another engine; remove it from mmd's environment".to_owned(),
        })?;
        let filters = serde_json::json!({ "label": [
            format!("{}={}", crate::domain::ownership::LABEL_PROJECT_ID, scope.project_id.0),
            format!("{}={}", crate::domain::ownership::LABEL_ENVIRONMENT, scope.environment),
        ]});
        let inspected = tokio::time::timeout(
            ENGINE_PATIENCE,
            crate::adapters::runtime::engine_containers::inspected_containers(engine, &filters),
        )
        .await
        .map_err(|_| MmError::RuntimeResponse {
            message: format!(
                "the engine did not list the containers of '{}' within {} s",
                scope.environment,
                ENGINE_PATIENCE.as_secs()
            ),
        })??;
        Ok(inspected.iter().filter_map(Seen::from_inspected).collect())
    }

    async fn look(
        self: &Arc<Self>,
        host: &Arc<dyn HealHost>,
        environment: &WatchedEnvironment,
        leaving: &BTreeSet<String>,
        (held, seen): (&mut BTreeSet<String>, &mut BTreeSet<String>),
    ) -> Result<(), MmError> {
        let scope = &environment.scope;
        let containers = self.containers(scope).await?;
        let now = crate::adapters::node_files::now_unix_ms();
        {
            let mut unhealthy = locked(&self.unhealthy);
            for container in &containers {
                seen.insert(container.id.clone());
                if container.unhealthy && !leaving.contains(&container.id) {
                    unhealthy
                        .entry(container.id.clone())
                        .or_insert_with(Instant::now);
                } else {
                    unhealthy.remove(&container.id);
                }
            }
        }
        let _writing = locked(&self.writing);
        let mut history = store::history(&self.directory, scope)?;
        let before = history.clone();
        history
            .containers
            .retain(|id, _| containers.iter().any(|container| container.id == *id));
        for (id, record) in &mut history.containers {
            let window = environment
                .services
                .iter()
                .find(|service| service.stack == record.stack && service.service == record.service)
                .and_then(|service| service.restart)
                .unwrap_or_default()
                .window_ms;
            let since = now.saturating_sub(i64::try_from(window).unwrap_or(i64::MAX));
            record.restarts.retain(|event| event.at_unix_ms > since);
            if record.limit.is_some() {
                held.insert(id.clone());
            }
        }
        if !host.deploying(scope) {
            for service in &environment.services {
                self.look_at_service(
                    host,
                    environment,
                    service,
                    (&containers, leaving),
                    &mut history,
                    held,
                );
            }
        }
        history
            .containers
            .retain(|_, record| !record.restarts.is_empty() || record.limit.is_some());
        if history != before {
            store::keep_history(&self.directory, scope, &history)?;
        }
        Ok(())
    }

    /// One service: at most one of its containers starts a restart.
    fn look_at_service(
        self: &Arc<Self>,
        host: &Arc<dyn HealHost>,
        environment: &WatchedEnvironment,
        service: &WatchedService,
        (containers, leaving): (&[Seen], &BTreeSet<String>),
        history: &mut store::History,
        held: &mut BTreeSet<String>,
    ) {
        let Some(policy) = service.restart else {
            return;
        };
        let key = service_key(&environment.scope, service);
        if locked(&self.restarting).contains(&key) {
            return;
        }
        let own = containers
            .iter()
            .filter(|container| {
                container.stack == service.stack && container.service == service.service
            })
            .collect::<Vec<_>>();
        if own.iter().any(|container| leaving.contains(&container.id)) {
            return;
        }
        let now = crate::adapters::node_files::now_unix_ms();
        for container in own.into_iter().filter(|container| container.unhealthy) {
            let Some(since) = locked(&self.unhealthy).get(&container.id).copied() else {
                continue;
            };
            let record = history
                .containers
                .entry(container.id.clone())
                .or_insert_with(|| store::ContainerHistory {
                    stack: service.stack.clone(),
                    service: service.service.clone(),
                    restarts: Vec::new(),
                    limit: None,
                });
            if record.limit.is_some() {
                continue;
            }
            let restarts = record
                .restarts
                .iter()
                .map(|event| event.at_unix_ms)
                .collect::<Vec<_>>();
            let unhealthy_ms = u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX);
            let event = |reason: String| RestartEvent {
                node: None,
                container: container.name.clone(),
                at_unix_ms: now,
                reason,
            };
            match policy.step(unhealthy_ms, &restarts, now) {
                HealthStep::Wait => {}
                HealthStep::Limit { reason } => {
                    let event = event(reason);
                    tracing::warn!(
                        container = %container.name,
                        reason = %event.reason,
                        code = crate::domain::healing::RESTART_LIMIT,
                        "no longer restarting a container that keeps failing its health check"
                    );
                    host.audit(environment, service, &event, true);
                    record.limit = Some(event);
                    held.insert(container.id.clone());
                }
                HealthStep::Restart { reason } => {
                    locked(&self.restarting).insert(key);
                    self.spawn_restart(host, environment, service, &container.id, event(reason));
                    return;
                }
            }
        }
    }

    fn spawn_restart(
        self: &Arc<Self>,
        host: &Arc<dyn HealHost>,
        environment: &WatchedEnvironment,
        service: &WatchedService,
        id: &str,
        event: RestartEvent,
    ) {
        let (healer, host) = (Arc::clone(self), Arc::clone(host));
        let (environment, service, id) = (environment.clone(), service.clone(), id.to_owned());
        tokio::spawn(async move {
            tracing::info!(container = %event.container, reason = %event.reason, "restarting a container that keeps failing its health check");
            let restarted = host.restart(&environment, &service, &id).await;
            if !matches!(restarted, Ok(false)) {
                host.audit(&environment, &service, &event, false);
                if let Err(error) = healer.record(&environment.scope, &id, &service, event) {
                    tracing::warn!(%error, "cannot keep a restart of an unhealthy container");
                }
            }
            if let Err(error) = restarted {
                tracing::warn!(%error, "restarting an unhealthy container failed");
            }
            locked(&healer.unhealthy).remove(&id);
            locked(&healer.restarting).remove(&service_key(&environment.scope, &service));
        });
    }

    fn record(
        &self,
        scope: &RemoteScope,
        id: &str,
        service: &WatchedService,
        event: RestartEvent,
    ) -> Result<(), MmError> {
        let _writing = locked(&self.writing);
        let mut history = store::history(&self.directory, scope)?;
        history
            .containers
            .entry(id.to_owned())
            .or_insert_with(|| store::ContainerHistory {
                stack: service.stack.clone(),
                service: service.service.clone(),
                restarts: Vec::new(),
                limit: None,
            })
            .restarts
            .push(event);
        store::keep_history(&self.directory, scope, &history)
    }

    /// What `mm service status` shows of a service: its restarts within
    /// its window and the containers past their limit.
    #[must_use]
    pub fn restarts(
        &self,
        scope: &RemoteScope,
        stack: &str,
        service: &str,
    ) -> Option<ServiceRestarts> {
        let _writing = locked(&self.writing);
        let history = store::history(&self.directory, scope).ok()?;
        let mut restarts = ServiceRestarts::default();
        for record in history
            .containers
            .values()
            .filter(|record| record.stack == stack && record.service == service)
        {
            restarts.restarts.extend(record.restarts.iter().cloned());
            restarts.limited.extend(record.limit.iter().cloned());
        }
        restarts.restarts.sort_by_key(|event| event.at_unix_ms);
        (!restarts.is_empty()).then_some(restarts)
    }

    /// Every restart and limit of the environment's services, or of
    /// `service` only: `(stack, service, event, limit)`.
    #[must_use]
    pub fn events(
        &self,
        scope: &RemoteScope,
        service: Option<&str>,
    ) -> Vec<(String, String, RestartEvent, bool)> {
        let _writing = locked(&self.writing);
        let Ok(history) = store::history(&self.directory, scope) else {
            return Vec::new();
        };
        let mut events = history
            .containers
            .into_values()
            .filter(|record| service.is_none_or(|service| record.service == service))
            .flat_map(|record| {
                let restarts = record
                    .restarts
                    .into_iter()
                    .map(|event| (record.stack.clone(), record.service.clone(), event, false));
                let limit = record
                    .limit
                    .map(|event| (record.stack.clone(), record.service.clone(), event, true));
                restarts.chain(limit).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        events.sort_by_key(|(_, _, event, _)| event.at_unix_ms);
        events
    }

    /// After `mm service restart`: the service's containers start again
    /// from no restart at all, and none is held out.
    pub fn forget(&self, scope: &RemoteScope, service: &str) -> Result<(), MmError> {
        let _writing = locked(&self.writing);
        let mut history = store::history(&self.directory, scope)?;
        history
            .containers
            .retain(|_, record| record.service != service);
        store::keep_history(&self.directory, scope, &history)
    }
}
