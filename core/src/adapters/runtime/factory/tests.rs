use async_trait::async_trait;

use super::*;
use crate::app::ports::{CommandOutput, CommandSpec, ProcessStream, StreamCommandSpec};
use crate::domain::observability::OperationCancellation;

struct UnusedProcess;

#[async_trait]
impl ProcessRunner for UnusedProcess {
    async fn run(&self, _specification: CommandSpec) -> Result<CommandOutput, MmError> {
        Err(MmError::State {
            message: "factory construction executed a process".to_owned(),
        })
    }
}

#[async_trait]
impl StreamingProcessRunner for UnusedProcess {
    async fn spawn(
        &self,
        _specification: StreamCommandSpec,
        _cancellation: OperationCancellation,
    ) -> Result<ProcessStream, MmError> {
        Err(MmError::State {
            message: "factory construction spawned a process".to_owned(),
        })
    }
}

const KINDS: [RuntimeKind; 2] = RuntimeKind::ALL;

#[test]
fn basic_factory_selects_all_runtime_drivers_and_rejects_streaming_operations() {
    let factory = RuntimeFactory::new(Arc::new(UnusedProcess));
    assert!(format!("{factory:?}").starts_with("RuntimeFactory"));
    for kind in KINDS {
        assert_eq!(factory.driver(kind).kind(), kind);
        assert!(factory.deployment(kind).is_ok());
        assert!(factory.operations(kind).is_err());
        assert!(factory.jobs(kind).is_err());
    }
    assert_eq!(factory.podman().kind(), RuntimeKind::PodmanCompose);
}

#[test]
fn streaming_factory_provides_operation_and_job_drivers_for_every_runtime() {
    let process = Arc::new(UnusedProcess);
    let factory = RuntimeFactory::with_streaming(process.clone(), process);
    for kind in KINDS {
        assert!(factory.operations(kind).is_ok());
        assert!(factory.jobs(kind).is_ok());
    }
}
