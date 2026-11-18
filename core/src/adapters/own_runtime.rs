//! A runtime on a thread of its own, for work that has to go on while the
//! runtime that asked for it is busy or stuck: answering names for a
//! node's containers, and watching over child processes.

use std::future::Future;
use std::time::Duration;

use crate::domain::error::MmError;

#[derive(Debug, Clone)]
pub struct OwnRuntime {
    name: &'static str,
    handle: tokio::runtime::Handle,
}

impl OwnRuntime {
    /// Starts the thread `name`, which runs what is spawned here until the
    /// process ends.
    pub fn start(name: &'static str) -> Result<Self, MmError> {
        let failed = |source| MmError::Io {
            context: format!("starting the {name} thread"),
            source,
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .thread_name(name)
            .build()
            .map_err(failed)?;
        let handle = runtime.handle().clone();
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || runtime.block_on(std::future::pending::<()>()))
            .map_err(failed)?;
        Ok(Self { name, handle })
    }

    /// Runs `work` on this runtime's thread.
    pub fn spawn<T: Send + 'static>(
        &self,
        work: impl Future<Output = T> + Send + 'static,
    ) -> tokio::task::JoinHandle<T> {
        self.handle.spawn(work)
    }

    /// Whether the thread runs a new task within `patience`: it is not
    /// stuck in something that does not yield.
    pub fn answers(&self, patience: Duration) -> Result<(), String> {
        let (sender, receiver) = std::sync::mpsc::channel();
        self.handle.spawn(async move {
            if sender.send(()).is_err() {
                tracing::debug!("nobody waited for a runtime's answer");
            }
        });
        receiver.recv_timeout(patience).map_err(|_| {
            format!(
                "the {} thread ran nothing for {} ms",
                self.name,
                patience.as_millis()
            )
        })
    }
}
