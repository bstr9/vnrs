//! Clock abstraction for deterministic time in backtesting.
//!
//! Provides a [`Clock`] trait with two implementations:
//! - [`LiveClock`]: returns real system time (for live trading)
//! - [`TestClock`]: manually advanceable time (for backtesting)

use chrono::{DateTime, Duration, Utc};
use std::sync::RwLock;

/// Clock trait for time abstraction.
///
/// Implementations provide the current time, allowing backtesting
/// to use deterministic, manually-controlled time instead of
/// real system time.
pub trait Clock: Send + Sync {
    /// Get the current time.
    fn now(&self) -> DateTime<Utc>;

    /// Get the current timestamp in milliseconds.
    fn timestamp_ms(&self) -> i64 {
        self.now().timestamp_millis()
    }

    /// Set the current time (advances the clock).
    ///
    /// For `TestClock` this updates internal state; for `LiveClock`
    /// this is a no-op since wall-clock time cannot be changed.
    fn set_time(&self, _time: DateTime<Utc>) {
        // Default: no-op. TestClock overrides this.
    }
}

/// Live clock that returns real system time.
///
/// Use this in production/live trading scenarios where
/// timestamps should reflect actual wall-clock time.
pub struct LiveClock;

impl LiveClock {
    /// Create a new `LiveClock`.
    pub fn new() -> Self {
        Self
    }
}

impl Clock for LiveClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

impl Default for LiveClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Test clock with manually advanceable time for backtesting.
///
/// Time is stored in an `RwLock` so the clock can be shared
/// across threads while remaining deterministic.
pub struct TestClock {
    time: RwLock<DateTime<Utc>>,
}

impl TestClock {
    /// Create a new `TestClock` starting at the given time.
    pub fn new(time: DateTime<Utc>) -> Self {
        Self {
            time: RwLock::new(time),
        }
    }

    /// Advance time by a duration.
    pub fn advance(&self, duration: Duration) {
        let mut time = self.time.write().unwrap_or_else(|e| e.into_inner());
        *time = *time + duration;
    }

    /// Set the time directly.
    pub fn set_time(&self, time: DateTime<Utc>) {
        let mut t = self.time.write().unwrap_or_else(|e| e.into_inner());
        *t = time;
    }

    /// Get the current test time without advancing.
    pub fn peek(&self) -> DateTime<Utc> {
        *self.time.read().unwrap_or_else(|e| e.into_inner())
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.time.read().unwrap_or_else(|e| e.into_inner())
    }

    fn set_time(&self, time: DateTime<Utc>) {
        let mut t = self.time.write().unwrap_or_else(|e| e.into_inner());
        *t = time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn test_live_clock_returns_current_time() {
        let clock = LiveClock::new();
        let before = Utc::now();
        let now = clock.now();
        let after = Utc::now();
        assert!(now >= before);
        assert!(now <= after);
    }

    #[test]
    fn test_live_clock_default() {
        let clock = LiveClock::default();
        let _ = clock.now(); // should not panic
    }

    #[test]
    fn test_live_clock_timestamp_ms() {
        let clock = LiveClock::new();
        let ms = clock.timestamp_ms();
        assert!(ms > 0);
    }

    #[test]
    fn test_test_clock_initial_time() {
        let t = fixed_time();
        let clock = TestClock::new(t);
        assert_eq!(clock.now(), t);
        assert_eq!(clock.peek(), t);
    }

    #[test]
    fn test_test_clock_advance() {
        let t = fixed_time();
        let clock = TestClock::new(t);
        clock.advance(Duration::seconds(30));
        assert_eq!(clock.now(), t + Duration::seconds(30));
    }

    #[test]
    fn test_test_clock_set_time() {
        let t1 = fixed_time();
        let clock = TestClock::new(t1);
        let t2 = Utc.with_ymd_and_hms(2025, 6, 15, 8, 30, 0).unwrap();
        clock.set_time(t2);
        assert_eq!(clock.now(), t2);
    }

    #[test]
    fn test_test_clock_timestamp_ms() {
        let t = fixed_time();
        let clock = TestClock::new(t);
        assert_eq!(clock.timestamp_ms(), t.timestamp_millis());
    }

    #[test]
    fn test_test_clock_advance_multiple() {
        let t = fixed_time();
        let clock = TestClock::new(t);
        clock.advance(Duration::hours(1));
        clock.advance(Duration::minutes(30));
        assert_eq!(clock.now(), t + Duration::minutes(90));
    }

    #[test]
    fn test_clock_trait_object() {
        let t = fixed_time();
        let live: Box<dyn Clock> = Box::new(LiveClock::new());
        let test: Box<dyn Clock> = Box::new(TestClock::new(t));
        // Both should be callable via trait object
        let _ = live.now();
        assert_eq!(test.now(), t);
    }

    #[test]
    fn test_test_clock_peek_does_not_advance() {
        let t = fixed_time();
        let clock = TestClock::new(t);
        let _ = clock.peek();
        assert_eq!(clock.now(), t);
    }
}
