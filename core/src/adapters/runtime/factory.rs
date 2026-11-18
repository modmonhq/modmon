use std::sync::Arc;

use crate::adapters::runtime::{
    DockerComposeRuntime, PodmanComposeRuntime, RuntimeOperationsAdapter,
};
use crate::app::ports::{
    LocalDeploymentRuntime, ProcessRunner, RuntimeDriver, RuntimeJobDriver, RuntimeOperationDriver,
    StreamingProcessRunner,
};
use crate::domain::error::MmError;
use crate::domain::runtime::RuntimeKind;

#[derive(Clone)]
pub struct RuntimeFactory {
    podman: Arc<PodmanComposeRuntime>,
    docker_compose: Arc<DockerComposeRuntime>,
    operations: Option<[Arc<RuntimeOperationsAdapter>; 2]>,
}

impl std::fmt::Debug for RuntimeFactory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeFactory")
            .finish_non_exhaustive()
    }
}

impl RuntimeFactory {
    #[must_use]
    pub fn new(process: Arc<dyn ProcessRunner>) -> Self {
        Self {
            podman: Arc::new(PodmanComposeRuntime::new(Arc::clone(&process))),
            docker_compose: Arc::new(DockerComposeRuntime::new(process)),
            operations: None,
        }
    }

    #[must_use]
    pub fn with_streaming(
        process: Arc<dyn ProcessRunner>,
        streaming: Arc<dyn StreamingProcessRunner>,
    ) -> Self {
        Self {
            podman: Arc::new(PodmanComposeRuntime::new(Arc::clone(&process))),
            docker_compose: Arc::new(DockerComposeRuntime::new(Arc::clone(&process))),
            operations: Some([
                Arc::new(RuntimeOperationsAdapter::new(
                    RuntimeKind::PodmanCompose,
                    Arc::clone(&process),
                    Arc::clone(&streaming),
                )),
                Arc::new(RuntimeOperationsAdapter::new(
                    RuntimeKind::DockerCompose,
                    process,
                    streaming,
                )),
            ]),
        }
    }

    /// Has Docker's reads (a status's containers, logs) go to the engine's
    /// API on `engine` instead of starting the CLI. `None` keeps the CLI.
    #[must_use]
    pub fn reading_engine_api(
        mut self,
        engine: Option<crate::adapters::engine_api::EngineApi>,
    ) -> Self {
        let docker = self
            .docker_compose
            .as_ref()
            .clone()
            .reading_engine_api(engine.clone());
        self.docker_compose = Arc::new(docker);
        if let Some(operations) = &mut self.operations {
            operations[1] = Arc::new(operations[1].as_ref().clone().reading_engine_api(engine));
        }
        self
    }

    /// Has Podman's reads (a status's containers, the containers whose
    /// logs are read) go to its Docker-compatible API on `engine` instead
    /// of starting the CLI. `None` keeps the CLI. Each container's lines
    /// still come from `podman logs`: Podman 5.4's API stamps them to the
    /// second only.
    #[must_use]
    pub fn reading_podman_api(
        mut self,
        engine: Option<crate::adapters::engine_api::EngineApi>,
    ) -> Self {
        let podman = self
            .podman
            .as_ref()
            .clone()
            .reading_engine_api(engine.clone());
        self.podman = Arc::new(podman);
        if let Some(operations) = &mut self.operations {
            operations[0] = Arc::new(operations[0].as_ref().clone().reading_engine_api(engine));
        }
        self
    }

    #[must_use]
    pub fn driver(&self, kind: RuntimeKind) -> Arc<dyn RuntimeDriver> {
        match kind {
            RuntimeKind::PodmanCompose => self.podman.clone(),
            RuntimeKind::DockerCompose => self.docker_compose.clone(),
        }
    }

    pub fn deployment(
        &self,
        kind: RuntimeKind,
    ) -> Result<Arc<dyn LocalDeploymentRuntime>, MmError> {
        Ok(match kind {
            RuntimeKind::PodmanCompose => self.podman.clone(),
            RuntimeKind::DockerCompose => self.docker_compose.clone(),
        })
    }

    pub fn operations(
        &self,
        kind: RuntimeKind,
    ) -> Result<Arc<dyn RuntimeOperationDriver>, MmError> {
        let operations = self.operations.as_ref().ok_or_else(|| MmError::State {
            message: "runtime factory has no streaming process adapter".to_owned(),
        })?;
        Ok(match kind {
            RuntimeKind::PodmanCompose => operations[0].clone(),
            RuntimeKind::DockerCompose => operations[1].clone(),
        })
    }

    pub fn jobs(&self, kind: RuntimeKind) -> Result<Arc<dyn RuntimeJobDriver>, MmError> {
        let operations = self.operations.as_ref().ok_or_else(|| MmError::State {
            message: "runtime factory has no process adapter for jobs".to_owned(),
        })?;
        Ok(match kind {
            RuntimeKind::PodmanCompose => operations[0].clone(),
            RuntimeKind::DockerCompose => operations[1].clone(),
        })
    }

    #[must_use]
    pub fn podman(&self) -> Arc<PodmanComposeRuntime> {
        Arc::clone(&self.podman)
    }
}

#[cfg(test)]
#[path = "factory/tests.rs"]
mod tests;
