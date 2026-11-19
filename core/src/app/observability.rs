use std::collections::BTreeMap;
use std::sync::Arc;

use crate::app::ports::{
    Clock, ExecAuditJournal, ExecStream, RuntimeOperationAuthorizer, RuntimeOperationDriver,
    SecretMaterialResolver,
};
use crate::app::{Application, WorkspaceSelection};
use crate::domain::error::MmError;
use crate::domain::observability::{
    AuthorizationRequest, ExecAuditRecord, ExecEvent, ExecRequest, OperationCancellation,
    ResolvedExecRequest, RestartOutcome, RestartRequest, RuntimeOperation, argv_hash,
    environment_values, validate_exec_request, validate_selector,
};
use crate::domain::runtime::{RuntimeCapability, RuntimeTarget};
use crate::domain::secret::SecretScope;

#[derive(Clone)]
pub struct RuntimeOperationsService {
    application: Application,
    driver: Arc<dyn RuntimeOperationDriver>,
    secrets: Arc<dyn SecretMaterialResolver>,
    authorizer: Arc<dyn RuntimeOperationAuthorizer>,
    audit: Arc<dyn ExecAuditJournal>,
    clock: Arc<dyn Clock>,
    /// On a cluster node: restarts that drop no request.
    drained: Option<DrainedRestart>,
}

pub struct RuntimeOperationsDependencies {
    pub application: Application,
    pub driver: Arc<dyn RuntimeOperationDriver>,
    pub secrets: Arc<dyn SecretMaterialResolver>,
    pub authorizer: Arc<dyn RuntimeOperationAuthorizer>,
    pub audit: Arc<dyn ExecAuditJournal>,
    pub clock: Arc<dyn Clock>,
}

impl RuntimeOperationsService {
    #[must_use]
    pub fn new(dependencies: RuntimeOperationsDependencies) -> Self {
        Self {
            application: dependencies.application,
            driver: dependencies.driver,
            secrets: dependencies.secrets,
            authorizer: dependencies.authorizer,
            audit: dependencies.audit,
            clock: dependencies.clock,
            drained: None,
        }
    }

    pub async fn status(
        &self,
        selection: &WorkspaceSelection,
        approval: &crate::domain::observability::ApprovalContext,
    ) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
        self.status_reading(selection, approval, crate::app::StatusRead::Rendered)
            .await
    }

    /// The status, asking as much of the engine as `read` says.
    pub async fn status_reading(
        &self,
        selection: &WorkspaceSelection,
        approval: &crate::domain::observability::ApprovalContext,
        read: crate::app::StatusRead,
    ) -> Result<crate::domain::workspace::WorkspaceSnapshot, MmError> {
        let (document, context) = self.application.operation_context(selection)?;
        self.authorizer.authorize(&AuthorizationRequest {
            principal: approval.principal.clone(),
            source: approval.source.clone(),
            operation: RuntimeOperation::Status,
            project_id: document.config.project.id,
            environment: context.selection.environment,
            selector: None,
            approved_high_risk: false,
        })?;
        self.application.status_reading(selection, read).await
    }

    /// Restarts one service's containers, or one container of it, without
    /// changing what is deployed. On a cluster node one container at a
    /// time, without dropping a request (`restart.rs`).
    pub async fn restart(
        &self,
        selection: &WorkspaceSelection,
        mut request: RestartRequest,
    ) -> Result<RestartOutcome, MmError> {
        validate_selector(&request.selector).map_err(|message| MmError::Environment { message })?;
        let (document, _, targets) = self.application.runtime_operation_context(selection)?;
        let target = select_target(&document, &targets, &request.selector)?;
        request.selector.module = Some(select_module(
            &document,
            &request.selector,
            &target.stack_name,
        )?);
        self.authorizer.authorize(&AuthorizationRequest {
            principal: request.approval.principal.clone(),
            source: request.approval.source.clone(),
            operation: RuntimeOperation::Restart,
            project_id: document.config.project.id,
            environment: target.environment.clone(),
            selector: Some(request.selector.clone()),
            approved_high_risk: request.approval.approved_high_risk,
        })?;
        let outcome = match &self.drained {
            Some(drained) => {
                self.restart_drained(drained, &document, target, &request.selector)
                    .await?
            }
            None => {
                let outcome = self.driver.restart(target, &request.selector).await?;
                self.await_restarted(&document, target, &request.selector.service)
                    .await?;
                outcome
            }
        };
        tracing::info!(
            principal = %request.approval.principal,
            source = %request.approval.source,
            environment = %target.environment,
            stack = %target.stack_name,
            service = %request.selector.service,
            containers = outcome.containers.len(),
            rolling = outcome.rolling,
            "service restarted"
        );
        Ok(outcome)
    }

    /// Waits until a restarted service is healthy again, as long as a
    /// deployment of the environment would wait for it.
    async fn await_restarted(
        &self,
        document: &crate::domain::config::ConfigDocument,
        target: &RuntimeTarget,
        service: &str,
    ) -> Result<(), MmError> {
        let environment = &target.environment;
        let deploy = document
            .config
            .environments
            .get(environment)
            .and_then(|config| config.deploy.as_ref());
        let timeout = crate::domain::config::DeployConfig::convergence_timeout(deploy).map_err(
            |problem| MmError::Environment {
                message: format!("environments.{environment}.deploy.convergence-timeout {problem}"),
            },
        )?;
        let outcome = self
            .application
            .wait_for_services(target, &[service.to_owned()], timeout)
            .await?;
        if outcome.state == crate::domain::runtime::ConvergenceState::Succeeded {
            return Ok(());
        }
        Err(MmError::RuntimeResponse {
            message: format!(
                "service '{service}' was restarted and is not healthy: {}. `mm service logs {service}` shows why",
                outcome
                    .message
                    .unwrap_or_else(|| format!("{:?}", outcome.state).to_lowercase())
            ),
        })
    }

    /// Where a forwarded connection to `service`'s `port` goes: the address
    /// of its one running container. Never a host port.
    pub async fn forward_address(
        &self,
        selection: &WorkspaceSelection,
        service: &str,
        port: u16,
    ) -> Result<std::net::SocketAddr, MmError> {
        let (document, _, targets) = self.application.runtime_operation_context(selection)?;
        let selector = crate::domain::observability::ServiceSelector {
            stack: None,
            module: None,
            service: service.to_owned(),
            container: None,
        };
        let target = select_target(&document, &targets, &selector)?;
        let address = self.driver.container_address(target, service).await?;
        Ok(std::net::SocketAddr::new(address, port))
    }

    pub async fn exec(
        &self,
        selection: &WorkspaceSelection,
        mut request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        validate_exec_request(&request).map_err(|message| MmError::Environment { message })?;
        let (document, _, targets) = self.application.runtime_operation_context(selection)?;
        let target = select_target(&document, &targets, &request.selector)?;
        request.selector.module = Some(select_module(
            &document,
            &request.selector,
            &target.stack_name,
        )?);
        let operation = if request.shell_wrapper {
            RuntimeOperation::Shell
        } else {
            RuntimeOperation::Exec
        };
        self.authorizer.authorize(&AuthorizationRequest {
            principal: request.approval.principal.clone(),
            source: request.approval.source.clone(),
            operation,
            project_id: document.config.project.id,
            environment: target.environment.clone(),
            selector: Some(request.selector.clone()),
            approved_high_risk: request.approval.approved_high_risk,
        })?;
        self.application
            .require_runtime_capabilities("exec", &[RuntimeCapability::Exec])
            .await?;
        let environment = self
            .resolve_exec_environment(&target.environment, &request)
            .await?;
        let audit_id = uuid::Uuid::new_v4();
        let started_at_unix_ms = self.clock.now_unix_ms();
        let executable = request.argv[0].clone();
        let mut audit = ExecAuditRecord {
            id: audit_id,
            principal: request.approval.principal.clone(),
            runtime: self.application.runtime_kind(),
            project_id: document.config.project.id,
            environment: target.environment.clone(),
            stack: target.stack_name.clone(),
            module: request.selector.module.clone(),
            service: request.selector.service.clone(),
            container_id: None,
            executable,
            argv_hash: argv_hash(&request.argv),
            started_at_unix_ms,
            finished_at_unix_ms: None,
            exit_code: None,
            error_code: None,
        };
        self.audit.append_exec_audit(&audit)?;
        let upstream = self
            .driver
            .exec(
                target,
                ResolvedExecRequest {
                    request,
                    environment,
                },
                cancellation.clone(),
            )
            .await;
        let upstream = match upstream {
            Ok(stream) => stream,
            Err(error) => {
                audit.finished_at_unix_ms = Some(self.clock.now_unix_ms());
                audit.error_code = Some(error.code().to_owned());
                self.audit.append_exec_audit(&audit)?;
                return Err(error);
            }
        };
        Ok(self.audited_exec_stream(upstream, audit, cancellation))
    }

    fn audited_exec_stream(
        &self,
        mut upstream: ExecStream,
        mut audit: ExecAuditRecord,
        cancellation: OperationCancellation,
    ) -> ExecStream {
        let (sender, receiver) = tokio::sync::mpsc::channel(128);
        let journal = Arc::clone(&self.audit);
        let clock = Arc::clone(&self.clock);
        let task_cancellation = cancellation.clone();
        tokio::spawn(async move {
            while let Some(event) = upstream.recv().await {
                let journal_result = match &event {
                    Ok(ExecEvent::Selected { container_id, .. }) => {
                        audit.container_id = Some(container_id.clone());
                        journal.append_exec_audit(&audit)
                    }
                    Ok(ExecEvent::Finished(outcome)) => {
                        audit.finished_at_unix_ms = Some(outcome.finished_at_unix_ms);
                        audit.exit_code = Some(outcome.exit_code);
                        journal.append_exec_audit(&audit)
                    }
                    Err(error) => {
                        audit.finished_at_unix_ms = Some(clock.now_unix_ms());
                        audit.error_code = Some(error.code().to_owned());
                        journal.append_exec_audit(&audit)
                    }
                    Ok(ExecEvent::Output { .. }) => Ok(()),
                };
                if let Err(error) = journal_result {
                    if sender.send(Err(error)).await.is_err() {
                        tracing::debug!("exec receiver closed before audit failure diagnostics");
                    }
                    upstream.cancel();
                    break;
                }
                if sender.send(event).await.is_err() {
                    upstream.cancel();
                    break;
                }
            }
            if audit.finished_at_unix_ms.is_none() {
                audit.finished_at_unix_ms = Some(clock.now_unix_ms());
                audit.error_code = Some("MM_CANCELLED".to_owned());
                if let Err(error) = journal.append_exec_audit(&audit)
                    && sender.send(Err(error)).await.is_err()
                {
                    tracing::debug!("exec receiver closed before cancellation audit failure");
                }
            }
            task_cancellation.cancel();
        });
        ExecStream::new(receiver, cancellation)
    }

    async fn resolve_exec_environment(
        &self,
        environment: &str,
        request: &ExecRequest,
    ) -> Result<crate::domain::runtime::RuntimeEnvironment, MmError> {
        let scope = SecretScope::new(environment);
        let mut values = BTreeMap::new();
        for item in &request.environment {
            let resolution = self.secrets.resolve(&scope, &item.reference).await?;
            let material = resolution.material.ok_or_else(|| MmError::Secret {
                message: format!("exec environment reference '{}' is unavailable", item.name),
            })?;
            values.insert(item.name.clone(), material.into_zeroizing().to_string());
        }
        Ok(environment_values(values))
    }
}

fn select_module(
    document: &crate::domain::config::ConfigDocument,
    selector: &crate::domain::observability::ServiceSelector,
    stack: &str,
) -> Result<String, MmError> {
    let matches = document
        .config
        .modules
        .iter()
        .filter(|(name, _)| {
            selector
                .module
                .as_ref()
                .is_none_or(|module| module == *name)
        })
        .filter(|(_, module)| {
            module
                .services
                .get(&selector.service)
                .is_some_and(|service| service.stack == stack)
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [module] => Ok(module.clone()),
        [] => Err(MmError::Environment {
            message: format!(
                "service '{}' has no matching configured module",
                selector.service
            ),
        }),
        _ => Err(MmError::Environment {
            message: format!(
                "service '{}' is ambiguous across modules; pass an explicit module selector",
                selector.service
            ),
        }),
    }
}

fn select_target<'a>(
    document: &crate::domain::config::ConfigDocument,
    targets: &'a [RuntimeTarget],
    selector: &crate::domain::observability::ServiceSelector,
) -> Result<&'a RuntimeTarget, MmError> {
    let matches = targets
        .iter()
        .filter(|target| {
            selector
                .stack
                .as_ref()
                .is_none_or(|stack| stack == &target.stack_name)
        })
        .filter(|target| {
            document.config.modules.values().any(|module| {
                module
                    .services
                    .get(&selector.service)
                    .is_some_and(|service| service.stack == target.stack_name)
            })
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [target] => Ok(*target),
        [] => Err(MmError::Environment {
            message: format!(
                "service '{}' is not in the selected stack",
                selector.service
            ),
        }),
        _ => Err(MmError::Environment {
            message: format!(
                "service '{}' is ambiguous across stacks; pass an explicit stack selector",
                selector.service
            ),
        }),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalRuntimeOperationAuthorizer;

impl RuntimeOperationAuthorizer for LocalRuntimeOperationAuthorizer {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), MmError> {
        if matches!(
            request.operation,
            RuntimeOperation::RawLogs | RuntimeOperation::Shell
        ) && !request.approved_high_risk
        {
            return Err(MmError::Security {
                message: "high-risk runtime operation requires explicit local approval".to_owned(),
            });
        }
        if request.principal.trim().is_empty()
            || request.source.trim().is_empty()
            || request.source.len() > 255
            || request.source.chars().any(char::is_control)
        {
            return Err(MmError::Security {
                message: "runtime operation has no valid authenticated local source".to_owned(),
            });
        }
        Ok(())
    }
}

#[path = "observability/logs.rs"]
mod logs;
#[path = "observability/restart.rs"]
mod restart;
pub use restart::DrainedRestart;
#[path = "observability/merged.rs"]
pub(crate) mod merged;

#[cfg(test)]
mod tests;
