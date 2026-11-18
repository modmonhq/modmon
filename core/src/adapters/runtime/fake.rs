use std::collections::BTreeMap;

use async_trait::async_trait;

use crate::app::ports::RuntimeDriver;
use crate::domain::error::MmError;
use crate::domain::runtime::{
    ConvergenceCancellation, ConvergenceOutcome, ConvergencePolicy, ConvergenceState,
    ObservedProject, RenderedProject, RuntimeKind, RuntimeProbe, RuntimeTarget,
};

#[derive(Debug, Clone)]
pub struct FakeRuntime {
    pub probe: RuntimeProbe,
    pub user_rendered: Option<BTreeMap<String, RenderedProject>>,
    pub rendered: BTreeMap<String, RenderedProject>,
    pub observed: BTreeMap<String, ObservedProject>,
}

#[async_trait]
impl RuntimeDriver for FakeRuntime {
    fn kind(&self) -> RuntimeKind {
        self.probe.kind
    }

    async fn probe(&self) -> RuntimeProbe {
        self.probe.clone()
    }

    async fn render(&self, target: &RuntimeTarget) -> Result<RenderedProject, MmError> {
        let rendered = if target.compose_files.len() == target.user_compose_file_count {
            self.user_rendered.as_ref().unwrap_or(&self.rendered)
        } else {
            &self.rendered
        };
        rendered
            .get(&target.stack_name)
            .cloned()
            .ok_or_else(|| MmError::RuntimeResponse {
                message: format!("fake runtime has no rendered stack '{}'", target.stack_name),
            })
    }

    async fn observe(&self, target: &RuntimeTarget) -> Result<ObservedProject, MmError> {
        self.observed
            .get(&target.stack_name)
            .cloned()
            .ok_or_else(|| MmError::RuntimeResponse {
                message: format!("fake runtime has no observed stack '{}'", target.stack_name),
            })
    }

    async fn wait_for_convergence(
        &self,
        target: &RuntimeTarget,
        _policy: ConvergencePolicy,
        cancellation: ConvergenceCancellation,
    ) -> Result<ConvergenceOutcome, MmError> {
        if cancellation.is_cancelled() {
            return Ok(ConvergenceOutcome {
                state: ConvergenceState::Cancelled,
                polls: 0,
                observation: ObservedProject::default(),
                service: None,
                diagnostic_code: Some("MM_CONVERGENCE_CANCELLED".to_owned()),
                message: None,
            });
        }
        Ok(ConvergenceOutcome {
            state: ConvergenceState::Succeeded,
            polls: 1,
            observation: self.observe(target).await?,
            service: None,
            diagnostic_code: None,
            message: None,
        })
    }
}

#[cfg(test)]
#[path = "fake/tests.rs"]
mod tests;
