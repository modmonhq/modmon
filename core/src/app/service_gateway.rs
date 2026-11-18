use crate::app::ports::{ExecStream, LogStream};
use crate::app::{RemoteEnvironment, RuntimeOperationsService, WorkspaceSelection};
use crate::domain::error::MmError;
use crate::domain::observability::{
    ApprovalContext, ExecRequest, LogRequest, OperationCancellation, RestartOutcome, RestartRequest,
};
use crate::domain::workspace::WorkspaceSnapshot;

/// Status, logs, commands, and restarts for one selected environment, whether
/// it runs on this machine, behind one remote daemon, or on the nodes of a
/// cluster. The command line, the terminal UI, and the MCP server all go
/// through it, so an operation behaves the same wherever it is started.
pub enum ServiceGateway {
    Local {
        operations: Box<RuntimeOperationsService>,
        selection: Box<WorkspaceSelection>,
    },
    Remote {
        environment: Box<RemoteEnvironment>,
    },
}

impl ServiceGateway {
    /// The remote target's name, or `None` when the environment is local.
    #[must_use]
    pub fn remote_target(&self) -> Option<&str> {
        match self {
            Self::Local { .. } => None,
            Self::Remote { environment } => Some(environment.target()),
        }
    }

    /// The names of the nodes of a cluster target, in declared order;
    /// empty for one host, local or remote.
    #[must_use]
    pub fn node_names(&self) -> Vec<String> {
        match self {
            Self::Local { .. } => Vec::new(),
            Self::Remote { environment } => environment
                .nodes()
                .iter()
                .filter_map(|node| node.name.clone())
                .collect(),
        }
    }

    pub async fn status(&self, approval: &ApprovalContext) -> Result<WorkspaceSnapshot, MmError> {
        self.status_reporting(approval, None).await
    }

    /// `status`, telling `so_far` what is known each time a node of a
    /// cluster answers. One host, local or remote, answers once.
    pub async fn status_reporting(
        &self,
        approval: &ApprovalContext,
        so_far: Option<crate::app::StatusSoFar<'_>>,
    ) -> Result<WorkspaceSnapshot, MmError> {
        match self {
            Self::Local {
                operations,
                selection,
            } => operations.status(selection, approval).await,
            Self::Remote { environment } => environment.status_reporting(so_far).await,
        }
    }

    /// The log lines of a service, or of every service, from all their
    /// containers on every node, as one stream in the order they were
    /// written.
    pub async fn logs(
        &self,
        request: LogRequest,
        cancellation: OperationCancellation,
    ) -> Result<LogStream, MmError> {
        match self {
            Self::Local {
                operations,
                selection,
            } => {
                if !request.nodes.is_empty() {
                    return Err(MmError::Target {
                        message: "this environment runs on this machine; --node applies to a target that lists nodes:".to_owned(),
                    });
                }
                operations.logs(selection, request, cancellation).await
            }
            Self::Remote { environment } => environment.logs(request, cancellation).await,
        }
    }

    pub async fn exec(
        &self,
        request: ExecRequest,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        self.exec_on(request, None, cancellation).await
    }

    /// `exec`, on one node of a cluster target when `node` names it.
    pub async fn exec_on(
        &self,
        request: ExecRequest,
        node: Option<&str>,
        cancellation: OperationCancellation,
    ) -> Result<ExecStream, MmError> {
        match self {
            Self::Local { .. } if node.is_some() => Err(MmError::Target {
                message: "this environment runs on this machine; a node applies to a target that lists nodes:".to_owned(),
            }),
            Self::Local {
                operations,
                selection,
            } => operations.exec(selection, request, cancellation).await,
            Self::Remote { environment } => {
                environment.exec_on(request, node, cancellation).await
            }
        }
    }

    /// Restarts a service's containers, or one of them, and waits until
    /// they are healthy again. On a cluster the nodes that run the service
    /// restart one at a time.
    pub async fn restart(&self, request: RestartRequest) -> Result<RestartOutcome, MmError> {
        match self {
            Self::Local {
                operations,
                selection,
            } => operations.restart(selection, request).await,
            Self::Remote { environment } => environment.restart(request).await,
        }
    }
}
