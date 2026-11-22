//! What a host-changing operation does right now, as `mm deploy status`
//! and the heartbeat of a waiting client show it.

use std::future::Future;
use std::sync::Arc;

/// Takes what the operation does now, or `None` once that is over.
pub type DoingSink = Arc<dyn Fn(Option<String>) + Send + Sync>;

tokio::task_local! {
    static SINK: DoingSink;
}

/// Runs `work`, telling `sink` what every `Doing` inside it says.
pub async fn reporting_doing<F: Future>(sink: DoingSink, work: F) -> F::Output {
    SINK.scope(sink, work).await
}

/// What the running operation does until this is dropped. Outside
/// `reporting_doing` it tells no one.
pub struct Doing(());

impl Doing {
    pub fn now(what: impl Into<String>) -> Self {
        tell(Some(what.into()));
        Self(())
    }

    pub fn update(&self, what: impl Into<String>) {
        tell(Some(what.into()));
    }
}

impl Drop for Doing {
    fn drop(&mut self) {
        tell(None);
    }
}

fn tell(what: Option<String>) {
    match SINK.try_with(|sink| sink(what)) {
        Ok(()) | Err(_) => {}
    }
}
