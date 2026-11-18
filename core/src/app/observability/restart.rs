//! A restart on a cluster node that drops no request (`domain::leaving`).
//!
//! The node restarts its containers of the service one at a time, each
//! healthy again before the next. A running container that another
//! container of the service can stand in for leaves first: out of every
//! node's name answers and ingress proxies, its drain, then the engine's
//! restart, and it is answered again once healthy. A container nothing can
//! stand in for (the service's only healthy one) and one that does not run
//! are restarted at once: a drain would only make the gap longer.

use std::sync::Arc;

use super::RuntimeOperationsService;
use crate::app::ports::{ContainerLeaving, ServiceNames};
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::observability::{RestartOutcome, ServiceSelector};
use crate::domain::runtime::{RuntimeTarget, ServiceIdentity};

/// What a cluster node restarts a service's containers through.
#[derive(Clone)]
pub struct DrainedRestart {
    /// The node's keeper of leaving containers.
    pub leaving: Arc<dyn ContainerLeaving>,
    /// The node's name resolver: who else answers for the service.
    pub names: Arc<dyn ServiceNames>,
}

impl std::fmt::Debug for DrainedRestart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DrainedRestart")
            .finish_non_exhaustive()
    }
}

impl RuntimeOperationsService {
    /// Restarts on a cluster node go one container at a time and drain
    /// each container that another one can stand in for.
    #[must_use]
    pub fn with_drained_restart(mut self, drained: DrainedRestart) -> Self {
        self.drained = Some(drained);
        self
    }

    /// `restart` on a cluster node.
    pub(super) async fn restart_drained(
        &self,
        drained: &DrainedRestart,
        document: &ConfigDocument,
        target: &RuntimeTarget,
        selector: &ServiceSelector,
    ) -> Result<RestartOutcome, MmError> {
        let containers = self.driver.restartable(target, selector).await?;
        if containers.is_empty() && !scaled(document, &selector.service) {
            return Err(MmError::RuntimeResponse {
                message: format!(
                    "service '{}' has no containers to restart; start the stack first",
                    selector.service
                ),
            });
        }
        // A scaled service runs no container on some of its nodes: such a
        // node restarts nothing.
        for (container, running) in &containers {
            if *running && stand_in_answers(drained.names.as_ref(), target, &selector.service) {
                drained
                    .leaving
                    .restart(std::slice::from_ref(container), None)
                    .await?;
            } else {
                let one = ServiceSelector {
                    container: Some(container.clone()),
                    ..selector.clone()
                };
                self.driver.restart(target, &one).await?;
            }
            self.await_restarted(document, target, &selector.service)
                .await?;
        }
        Ok(RestartOutcome {
            runtime: self.application.runtime_kind(),
            identity: ServiceIdentity {
                project_id: target.project_id,
                environment: target.environment.clone(),
                stack: target.stack_name.clone(),
                module: selector.module.clone(),
                service: selector.service.clone(),
            },
            containers: containers.into_iter().map(|(id, _)| id).collect(),
            rolling: false,
            nodes: Vec::new(),
        })
    }
}

/// Whether the service has another healthy container that answers for it
/// while one of its containers leaves: at least two answer now, on some
/// network of the environment, on any node.
fn stand_in_answers(names: &dyn ServiceNames, target: &RuntimeTarget, service: &str) -> bool {
    let report = names.report(&crate::domain::cluster_network::network_key(
        target.project_id,
        &target.environment,
        "",
    ));
    report
        .names
        .iter()
        .filter(|name| name.name == service)
        .map(|name| name.answers().count())
        .max()
        .unwrap_or(0)
        >= 2
}

/// Whether `service` runs a count of containers of its own (`replicas`,
/// `autoscale`): a node of its placement may run none of them.
fn scaled(document: &ConfigDocument, service: &str) -> bool {
    document
        .config
        .modules
        .values()
        .filter_map(|module| module.services.get(service))
        .any(|config| config.replicas.is_some() || config.autoscale.is_some())
}
