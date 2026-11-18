//! What `mmd` says about its host's container engine when a client asks
//! about the node (`mm target nodes`, `mm doctor`).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::domain::remote::EngineFacts;
use crate::domain::runtime::RuntimeKind;

/// How long a probe of the engine is kept: a node is asked about itself
/// often (the terminal UI, `mm target nodes`), and the engine's version
/// and Compose provider change only with packages.
const KEEP: Duration = Duration::from_secs(60);
/// How long a node waits for its engine to answer before it says so.
const PATIENCE: Duration = Duration::from_secs(5);

/// The last probe of the host's engine.
#[derive(Default)]
pub(crate) struct HostEngine {
    last: Mutex<Option<(Instant, EngineFacts)>>,
}

impl HostEngine {
    /// The engine `runtime` as it answers now, or as it answered less than
    /// a minute ago.
    pub(crate) async fn facts(&self, runtime: RuntimeKind) -> EngineFacts {
        if let Some((at, facts)) = self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            && facts.runtime == runtime
            && at.elapsed() < KEEP
        {
            return facts.clone();
        }
        let driver = super::RuntimeComposition::new().driver(runtime);
        let facts = match tokio::time::timeout(PATIENCE, driver.probe()).await {
            Ok(probe) => EngineFacts::of(&probe),
            Err(_) => EngineFacts {
                runtime,
                version: None,
                compose: None,
                compose_version: None,
                problem: Some(format!(
                    "the engine did not answer within {} seconds",
                    PATIENCE.as_secs()
                )),
            },
        };
        *self
            .last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((Instant::now(), facts.clone()));
        facts
    }
}
