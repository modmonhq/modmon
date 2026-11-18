use super::*;
use crate::domain::observability::{
    ExecRequest, LogRequest, OperationCancellation, RestartRequest, ServiceSelector,
};

/// Logs, commands, and restarts for the TUI: the same requests through the
/// same gateway as `mm service logs`, `exec`, and `restart`.
pub(super) struct ServiceOperations {
    pub(super) reads: Arc<super::Reads>,
}

impl ServiceOperations {
    fn gateway(&self) -> Result<Arc<crate::app::ServiceGateway>, MmError> {
        self.reads.gateway()
    }
}

fn selector(target: &tui::ServiceTarget) -> ServiceSelector {
    ServiceSelector {
        stack: Some(target.stack.clone()),
        module: None,
        service: target.service.clone(),
        container: target.container.clone(),
    }
}

#[async_trait::async_trait]
impl tui::ServiceOperations for ServiceOperations {
    async fn logs(
        &self,
        target: &tui::ServiceTarget,
        tail: u32,
    ) -> Result<crate::app::ports::LogStream, MmError> {
        // A log view follows, and keeps timestamps so it can show or hide
        // them. A node's row reads that node, as `--node` does.
        let request = LogRequest {
            tail,
            follow: true,
            timestamps: true,
            nodes: target.node.iter().cloned().collect(),
            ..LogRequest::new(selector(target), approval("tui", false))
        };
        self.gateway()?
            .logs(request, OperationCancellation::default())
            .await
    }

    async fn exec(
        &self,
        target: &tui::ServiceTarget,
        argv: Vec<String>,
    ) -> Result<crate::app::ports::ExecStream, MmError> {
        let request = ExecRequest::new(selector(target), argv, approval("tui", false));
        self.gateway()?
            .exec_on(
                request,
                target.node.as_deref(),
                OperationCancellation::default(),
            )
            .await
    }

    async fn host_values(&self) -> Result<Option<crate::domain::staged::RemoteValues>, MmError> {
        match self.reads.remote_environment()? {
            Some(environment) => environment.host_values().await.map(Some),
            None => Ok(None),
        }
    }

    async fn stage(&self, edit: crate::app::KeyEdit) -> Result<String, MmError> {
        let environment = self
            .reads
            .remote_environment()?
            .ok_or_else(|| MmError::Environment {
                message: "this environment runs on this machine; nothing is staged here".to_owned(),
            })?;
        let report = environment
            .stage_values(vec![edit], &composition::AgeValueSealer)
            .await?;
        Ok(output::stage_summary(&report))
    }

    async fn restart(&self, target: &tui::ServiceTarget) -> Result<String, MmError> {
        let outcome = self
            .gateway()?
            .restart(RestartRequest {
                selector: selector(target),
                approval: approval("tui", false),
            })
            .await?;
        Ok(output::restart_summary(&outcome))
    }
}
