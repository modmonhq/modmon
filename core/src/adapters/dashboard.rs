use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::sync::broadcast;

use crate::app::ports::{Clock, DashboardSubscription};
use crate::domain::dashboard::DashboardEvent;

pub struct DashboardEventHub {
    sender: broadcast::Sender<DashboardEvent>,
    sequence: Arc<AtomicU64>,
}

impl DashboardEventHub {
    #[must_use]
    pub fn start(clock: Arc<dyn Clock>) -> Self {
        let (sender, _) = broadcast::channel(32);
        let sequence = Arc::new(AtomicU64::new(1));
        let background_sender = sender.clone();
        let background_sequence = Arc::clone(&sequence);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let event = DashboardEvent::Refresh {
                    sequence: background_sequence.fetch_add(1, Ordering::Relaxed),
                    occurred_at_unix_ms: clock.now_unix_ms(),
                };
                if background_sender.send(event).is_err() {
                    tracing::trace!("dashboard refresh had no active receivers");
                }
            }
        });
        Self { sender, sequence }
    }
}

impl DashboardSubscription for DashboardEventHub {
    fn connected_event(&self) -> DashboardEvent {
        DashboardEvent::Connected {
            api_version: crate::domain::dashboard::DASHBOARD_API_VERSION,
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<DashboardEvent> {
        self.sender.subscribe()
    }
}
