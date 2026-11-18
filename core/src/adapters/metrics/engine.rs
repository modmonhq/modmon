//! The engine's own usage numbers, for a container whose control group
//! the host does not show.
//!
//! The sampler runs on a thread of its own and the engine is asked with
//! commands that run on the daemon's runtime, so this waits for them from
//! the sampler's thread.

use std::sync::Arc;
use std::time::Duration;

use crate::app::ports::{EngineUsage, RuntimeDriver};
use crate::domain::error::MmError;
use crate::domain::metrics::{ContainerSample, STEP_SECONDS};

/// The engine measures over a second before it answers; it must be done
/// before the next step begins.
const PATIENCE: Duration = Duration::from_secs(STEP_SECONDS - 1);

pub struct EngineStats {
    runtime: Arc<dyn RuntimeDriver>,
    commands: tokio::runtime::Handle,
}

impl std::fmt::Debug for EngineStats {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EngineStats")
            .finish_non_exhaustive()
    }
}

impl EngineStats {
    /// Asks `runtime`, running its commands on the runtime `commands`
    /// belongs to.
    #[must_use]
    pub fn new(runtime: Arc<dyn RuntimeDriver>, commands: tokio::runtime::Handle) -> Self {
        Self { runtime, commands }
    }
}

impl EngineUsage for EngineStats {
    /// Must not be called on a thread of the runtime itself: it waits.
    fn usage(&self, containers: &[String]) -> Result<Vec<ContainerSample>, MmError> {
        self.commands.block_on(async {
            tokio::time::timeout(PATIENCE, self.runtime.container_usage(containers))
                .await
                .map_err(|_| MmError::ProcessTimeout {
                    program: "the engine's stats".to_owned(),
                    seconds: PATIENCE.as_secs(),
                })?
        })
    }
}
