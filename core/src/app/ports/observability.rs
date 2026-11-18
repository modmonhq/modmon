use async_trait::async_trait;
use zeroize::Zeroizing;

use crate::domain::error::MmError;
use crate::domain::observability::{
    AuthorizationRequest, ExecAuditRecord, ExecEvent, LogRecord, LogRequest, OperationCancellation,
    ResolvedExecRequest, RestartOutcome, ServiceSelector,
};
use crate::domain::runtime::RuntimeTarget;

pub struct LogStream {
    receiver: tokio::sync::mpsc::Receiver<Result<LogRecord, MmError>>,
    cancellation: OperationCancellation,
}

impl LogStream {
    #[must_use]
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<Result<LogRecord, MmError>>,
        cancellation: OperationCancellation,
    ) -> Self {
        Self {
            receiver,
            cancellation,
        }
    }

    pub async fn recv(&mut self) -> Option<Result<LogRecord, MmError>> {
        self.receiver.recv().await
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl Drop for LogStream {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

pub struct ExecStream {
    receiver: tokio::sync::mpsc::Receiver<Result<ExecEvent, MmError>>,
    cancellation: OperationCancellation,
}

impl ExecStream {
    #[must_use]
    pub fn new(
        receiver: tokio::sync::mpsc::Receiver<Result<ExecEvent, MmError>>,
        cancellation: OperationCancellation,
    ) -> Self {
        Self {
            receiver,
            cancellation,
        }
    }

    pub async fn recv(&mut self) -> Option<Result<ExecEvent, MmError>> {
        self.receiver.recv().await
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
}

impl Drop for ExecStream {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

#[async_trait]
pub trait RuntimeOperationDriver: Send + Sync {
    /// Whether one read returns every service of a Compose project (a
    /// request without a selector). A driver that cannot is asked for one
    /// service at a time.
    fn reads_whole_projects(&self) -> bool {
        false
    }

    /// The log lines of one Compose project's service, or of all its
    /// services, every container of them in the order the lines were
    /// written. Each line names its service and, when the driver knows
    /// it, its container.
    async fn logs(
        &self,
        target: &RuntimeTarget,
        request: LogRequest,
        redactions: Vec<Zeroizing<String>>,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError>;

    async fn exec(
        &self,
        target: &RuntimeTarget,
        request: ResolvedExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError>;

    /// Restarts the selected service's containers, or only the selected one.
    async fn restart(
        &self,
        _target: &RuntimeTarget,
        _selector: &ServiceSelector,
    ) -> Result<RestartOutcome, MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime does not support restarting one service".to_owned(),
        })
    }

    /// The containers `restart` restarts for `selector`, each with whether
    /// it runs: what a restart one container at a time goes through.
    async fn restartable(
        &self,
        _target: &RuntimeTarget,
        _selector: &ServiceSelector,
    ) -> Result<Vec<(String, bool)>, MmError> {
        Err(MmError::RuntimeResponse {
            message: "this runtime does not list a service's containers to restart".to_owned(),
        })
    }

    /// The address of the one running container of `service`, for port
    /// forwarding from the daemon.
    async fn container_address(
        &self,
        _target: &RuntimeTarget,
        _service: &str,
    ) -> Result<std::net::IpAddr, MmError> {
        Err(MmError::Environment {
            message: "this runtime does not support port forwarding".to_owned(),
        })
    }
}

pub trait RuntimeOperationAuthorizer: Send + Sync {
    fn authorize(&self, request: &AuthorizationRequest) -> Result<(), MmError>;
}

pub trait ExecAuditJournal: Send + Sync {
    fn append_exec_audit(&self, record: &ExecAuditRecord) -> Result<(), MmError>;
}
