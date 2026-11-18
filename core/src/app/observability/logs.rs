//! Log lines of one service, or of every service of an environment, on
//! this host: all their containers as one stream in the order the lines
//! were written.

use std::time::Duration;

use super::merged::{LogSourceRead, MergedRead, merged};
use super::{RuntimeOperationsService, select_module, select_target};
use crate::app::WorkspaceSelection;
use crate::app::ports::LogStream;
use crate::domain::config::ConfigDocument;
use crate::domain::error::MmError;
use crate::domain::observability::{
    AuthorizationRequest, LogMode, LogRequest, OperationCancellation, RuntimeOperation,
    ServiceSelector, validate_log_request,
};
use crate::domain::runtime::{RuntimeCapability, RuntimeTarget, ServiceIdentity};

/// How long a followed line waits for an older line of another stack.
const STACK_HOLD: Duration = Duration::from_millis(100);

impl RuntimeOperationsService {
    pub async fn logs(
        &self,
        selection: &WorkspaceSelection,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        validate_log_request(&request).map_err(|message| MmError::Environment { message })?;
        let (document, environment, targets) =
            self.application.runtime_operation_context(selection)?;
        let reads = self.log_reads(&document, &environment, &targets, &request)?;
        let operation = if request.mode == LogMode::Raw {
            RuntimeOperation::RawLogs
        } else {
            RuntimeOperation::Logs
        };
        self.authorizer.authorize(&AuthorizationRequest {
            principal: request.approval.principal.clone(),
            source: request.approval.source.clone(),
            operation,
            project_id: document.config.project.id,
            environment: environment.clone(),
            selector: reads
                .first()
                .filter(|_| request.selector.is_some())
                .and_then(|(_, request)| request.selector.clone())
                .or_else(|| request.selector.clone()),
            approved_high_risk: request.approval.approved_high_risk,
        })?;
        self.application
            .require_read_capabilities("logs", &[RuntimeCapability::StreamLogs])
            .await?;
        let redactions = if request.mode == LogMode::Raw {
            Vec::new()
        } else {
            self.log_redactions(&document, &environment).await?
        };
        let modules = service_modules(&document);
        // One read needs no merge: the driver's stream is the answer.
        if let [(target, read)] = reads.as_slice() {
            let stream = self
                .driver
                .logs(target, read.clone(), redactions, cancellation.clone())
                .await?;
            return Ok(with_modules(stream, modules, cancellation));
        }
        let identity = ServiceIdentity {
            project_id: document.config.project.id,
            environment,
            stack: String::new(),
            module: None,
            service: String::new(),
        };
        let sources = reads
            .into_iter()
            .map(|(target, read)| {
                let driver = std::sync::Arc::clone(&self.driver);
                let redactions = redactions.clone();
                let modules = modules.clone();
                let cancellation = OperationCancellation::default();
                LogSourceRead {
                    node: None,
                    opening: Box::pin(async move {
                        let stream = driver
                            .logs(&target, read, redactions, cancellation.clone())
                            .await?;
                        Ok(with_modules(stream, modules, cancellation))
                    }),
                }
            })
            .collect();
        Ok(merged(
            MergedRead {
                sources,
                follow: request.follow,
                tail: request.tail,
                hold: STACK_HOLD,
                patience: None,
                identity,
                buffered_records: request.limits.buffered_records,
            },
            cancellation,
        ))
    }

    /// What is asked of the driver: one read for a named service, and for
    /// every service one read per stack, or per service where the driver
    /// reads one service at a time. A service that the configuration
    /// places on another node has no containers here: no read.
    fn log_reads(
        &self,
        document: &ConfigDocument,
        environment: &str,
        targets: &[RuntimeTarget],
        request: &LogRequest,
    ) -> Result<Vec<(RuntimeTarget, LogRequest)>, MmError> {
        if let Some(selector) = &request.selector {
            let target = select_target(document, targets, selector)?;
            let module = select_module(document, selector, &target.stack_name)?;
            let here = document
                .config
                .modules
                .get(&module)
                .and_then(|module| module.services.get(&selector.service))
                .is_some_and(|service| service.is_active(environment));
            if !here {
                return Ok(Vec::new());
            }
            let mut read = request.clone();
            read.selector = Some(ServiceSelector {
                module: Some(module),
                ..selector.clone()
            });
            return Ok(vec![(target.clone(), read)]);
        }
        let mut reads = Vec::new();
        for target in targets {
            let services = document
                .config
                .modules
                .iter()
                .flat_map(|(module, config)| {
                    config
                        .services
                        .iter()
                        .filter(|(_, service)| {
                            service.stack == target.stack_name && service.is_active(environment)
                        })
                        .map(move |(service, _)| (module.clone(), service.clone()))
                })
                .collect::<Vec<_>>();
            if services.is_empty() {
                continue;
            }
            if self.driver.reads_whole_projects() {
                reads.push((target.clone(), request.clone()));
                continue;
            }
            for (module, service) in services {
                let mut read = request.clone();
                read.selector = Some(ServiceSelector {
                    stack: Some(target.stack_name.clone()),
                    module: Some(module),
                    service,
                    container: None,
                });
                reads.push((target.clone(), read));
            }
        }
        Ok(reads)
    }
}

/// The module of every service, by stack and service name.
fn service_modules(
    document: &ConfigDocument,
) -> std::sync::Arc<std::collections::BTreeMap<(String, String), String>> {
    std::sync::Arc::new(
        document
            .config
            .modules
            .iter()
            .flat_map(|(module, config)| {
                config.services.iter().map(move |(service, settings)| {
                    ((settings.stack.clone(), service.clone()), module.clone())
                })
            })
            .collect(),
    )
}

/// The stream with each line's module filled in from the configuration,
/// for a read of a whole project, whose driver knows services only.
/// `cancellation` is the read's own: it ends with the returned stream.
fn with_modules(
    mut stream: LogStream,
    modules: std::sync::Arc<std::collections::BTreeMap<(String, String), String>>,
    cancellation: OperationCancellation,
) -> LogStream {
    let (sender, receiver) = tokio::sync::mpsc::channel(128);
    tokio::spawn(async move {
        loop {
            let item = tokio::select! {
                item = stream.recv() => item,
                () = sender.closed() => break,
            };
            let Some(item) = item else {
                break;
            };
            let item = item.map(|mut record| {
                if record.identity.module.is_none() {
                    record.identity.module = modules
                        .get(&(
                            record.identity.stack.clone(),
                            record.identity.service.clone(),
                        ))
                        .cloned();
                }
                record
            });
            if sender.send(item).await.is_err() {
                break;
            }
        }
    });
    LogStream::new(receiver, cancellation)
}
