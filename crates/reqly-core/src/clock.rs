//! Time source abstraction so services stay deterministic under test.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Milliseconds since the Unix epoch (UTC).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize, TS,
)]
#[ts(export, type = "number")]
pub struct Timestamp(pub i64);

impl Timestamp {
    pub fn millis(self) -> i64 {
        self.0
    }
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        Timestamp(ms)
    }
}

/// Manually driven clock for tests.
#[derive(Debug, Default)]
pub struct FixedClock(AtomicI64);

impl FixedClock {
    pub fn new(ms: i64) -> Self {
        Self(AtomicI64::new(ms))
    }

    pub fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.0.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_advances() {
        let clock = FixedClock::new(1_000);
        assert_eq!(clock.now(), Timestamp(1_000));
        clock.advance(500);
        assert_eq!(clock.now(), Timestamp(1_500));
    }

    #[test]
    fn system_clock_is_after_2026() {
        assert!(SystemClock.now().millis() > 1_767_225_600_000);
    }
}
