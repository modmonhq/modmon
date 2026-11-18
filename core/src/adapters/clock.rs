use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::ports::Clock;

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> i64 {
        let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return 0;
        };
        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub unix_ms: i64,
}

impl Clock for FixedClock {
    fn now_unix_ms(&self) -> i64 {
        self.unix_ms
    }
}
