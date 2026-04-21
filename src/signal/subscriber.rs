//! Subscriber management for the SignalBus.
//!
//! Provides types for tracking signal subscribers and their subscriptions.

use std::sync::atomic::{AtomicU64, Ordering};

/// Unique identifier for a signal subscriber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberId(u64);

impl SubscriberId {
    /// Generate a new unique subscriber ID.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for SubscriberId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubscriberId({})", self.0)
    }
}

impl Default for SubscriberId {
    fn default() -> Self {
        Self::new()
    }
}

/// A subscription handle returned when subscribing to a topic.
///
/// Can be used to unsubscribe from the topic.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// The topic this subscription is for.
    pub topic: String,
    /// The subscriber ID.
    pub subscriber_id: SubscriberId,
}

impl Subscription {
    /// Create a new subscription handle.
    pub fn new(topic: impl Into<String>, subscriber_id: SubscriberId) -> Self {
        Self {
            topic: topic.into(),
            subscriber_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscriber_id_uniqueness() {
        let id1 = SubscriberId::new();
        let id2 = SubscriberId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_subscriber_id_display() {
        let id = SubscriberId::new();
        let display = format!("{}", id);
        assert!(display.starts_with("SubscriberId("));
    }

    #[test]
    fn test_subscription_new() {
        let sub = Subscription::new("SIGNAL.BTC", SubscriberId::new());
        assert_eq!(sub.topic, "SIGNAL.BTC");
    }
}
