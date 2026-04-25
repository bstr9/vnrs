//! SignalBus — topic-based pub/sub for typed trading signals.
//!
//! The `SignalBus` bridges AI signal sources with traditional strategies.
//! Signal sources publish typed `Signal` instances to named topics; strategies
//! subscribe to topics and receive signals through `mpsc` channels.
//!
//! # Architecture
//!
//! ```text
//! AI Source ──publish("sentiment.btc", signal)──▶ SignalBus ──▶ Strategy (receiver)
//!                                                         │
//!                                                         └─▶ latest signal cache
//! ```
//!
//! # Example
//!
//! ```rust
//! use trade_engine::signal::{SignalBus, Signal, SignalDirection};
//!
//! let bus = SignalBus::new();
//!
//! // Strategy subscribes
//! let mut rx = bus.subscribe("sentiment.btc");
//!
//! // AI source publishes
//! let signal = Signal::new("s1", "sentiment_v2", "BTCUSDT.BINANCE", SignalDirection::Long, 0.8, 0.9);
//! bus.publish("sentiment.btc", signal);
//!
//! // Strategy reads signal
//! let received = rx.try_recv().unwrap();
//! assert_eq!(received.direction, SignalDirection::Long);
//!
//! // Strategy can also read cached latest signal
//! let latest = bus.get_latest("sentiment.btc").unwrap();
//! assert_eq!(latest.signal_id, "s1");
//! ```

use std::collections::HashMap;
use std::sync::RwLock;

use tokio::sync::mpsc;
use tracing::warn;

use super::types::Signal;

// ---------------------------------------------------------------------------
// SignalBus — concurrent topic-based pub/sub
// ---------------------------------------------------------------------------

/// Topic-based pub/sub bus for typed trading signals.
///
/// Thread safety is provided by `std::sync::RwLock` with lock-poisoning
/// recovery (falls back to `into_inner` on poison), following the same
/// pattern as `MessageBus`.
///
    /// **Note**: The spec mentions DashMap, but `RwLock<HashMap>` is used here
    /// for simplicity and equivalent correctness for this use case.
pub struct SignalBus {
    /// Topic → list of subscriber senders.
    subscribers: RwLock<HashMap<String, Vec<mpsc::UnboundedSender<Signal>>>>,
    /// Topic → latest cached signal.
    latest: RwLock<HashMap<String, Signal>>,
}

impl SignalBus {
    /// Create a new empty signal bus.
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            latest: RwLock::new(HashMap::new()),
        }
    }

    /// Subscribe to a topic.
    ///
    /// Creates an `mpsc::unbounded_channel`, stores the sender, and returns
    /// the receiver. The receiver can be used to asynchronously consume
    /// signals published to the given topic.
    ///
    /// If the topic does not exist yet, it is created automatically.
    pub fn subscribe(&self, topic: &str) -> mpsc::UnboundedReceiver<Signal> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subs = self.subscribers.write().unwrap_or_else(|e| {
            warn!("SignalBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        subs.entry(topic.to_string()).or_default().push(tx);
        rx
    }

    /// Publish a signal to a topic.
    ///
    /// - Sends the signal to all current subscribers (channels with a closed
    ///   receiver are lazily removed).
    /// - Caches the signal as the latest for the topic (overwriting any
    ///   previous cached signal).
    pub fn publish(&self, topic: &str, signal: Signal) {
        // Cache latest signal
        {
            let mut latest = self.latest.write().unwrap_or_else(|e| {
                warn!("SignalBus latest lock poisoned, recovering");
                e.into_inner()
            });
            latest.insert(topic.to_string(), signal.clone());
        }

        // Send to subscribers, removing dead senders
        let mut subs = self.subscribers.write().unwrap_or_else(|e| {
            warn!("SignalBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        if let Some(senders) = subs.get_mut(topic) {
            senders.retain(|tx| tx.send(signal.clone()).is_ok());
        }
    }

    /// Get the latest cached signal for a topic.
    ///
    /// Returns `None` if no signal has been published to the topic yet.
    pub fn get_latest(&self, topic: &str) -> Option<Signal> {
        let latest = self.latest.read().unwrap_or_else(|e| {
            warn!("SignalBus latest lock poisoned, recovering");
            e.into_inner()
        });
        latest.get(topic).cloned()
    }

    /// Get the latest cached signals for a given symbol across all topics.
    ///
    /// Scans all cached signals and returns those whose `symbol` field
    /// matches the provided symbol.
    pub fn get_latest_by_symbol(&self, symbol: &str) -> Vec<Signal> {
        let latest = self.latest.read().unwrap_or_else(|e| {
            warn!("SignalBus latest lock poisoned, recovering");
            e.into_inner()
        });
        latest
            .values()
            .filter(|signal| signal.symbol == symbol)
            .cloned()
            .collect()
    }

    /// Remove all subscribers for a topic.
    ///
    /// Drops all sender handles, which will cause the corresponding receiver
    /// ends to return `None` on the next `recv()` call.
    /// Also removes the cached latest signal for the topic.
    pub fn unsubscribe(&self, topic: &str) {
        {
            let mut subs = self.subscribers.write().unwrap_or_else(|e| {
                warn!("SignalBus subscribers lock poisoned, recovering");
                e.into_inner()
            });
            subs.remove(topic);
        }
        {
            let mut latest = self.latest.write().unwrap_or_else(|e| {
                warn!("SignalBus latest lock poisoned, recovering");
                e.into_inner()
            });
            latest.remove(topic);
        }
    }

    /// List all topics that currently have at least one subscriber.
    pub fn topics(&self) -> Vec<String> {
        let subs = self.subscribers.read().unwrap_or_else(|e| {
            warn!("SignalBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        subs.keys().cloned().collect()
    }

    /// List all topics that have a cached latest signal.
    pub fn cached_topics(&self) -> Vec<String> {
        let latest = self.latest.read().unwrap_or_else(|e| {
            warn!("SignalBus latest lock poisoned, recovering");
            e.into_inner()
        });
        latest.keys().cloned().collect()
    }

    /// Number of active subscribers for a topic.
    pub fn subscriber_count(&self, topic: &str) -> usize {
        let subs = self.subscribers.read().unwrap_or_else(|e| {
            warn!("SignalBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        subs.get(topic).map(|s| s.len()).unwrap_or(0)
    }
}

impl Default for SignalBus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::types::SignalDirection;

    fn make_signal(id: &str, symbol: &str, direction: SignalDirection, strength: f64) -> Signal {
        Signal::new(id, "test_source", symbol, direction, strength, 0.9)
    }

    #[test]
    fn test_subscribe_and_publish() {
        let bus = SignalBus::new();
        let mut rx = bus.subscribe("sentiment.btc");

        let signal = make_signal("s1", "BTCUSDT.BINANCE", SignalDirection::Long, 0.8);
        bus.publish("sentiment.btc", signal.clone());

        let received = rx.try_recv().expect("should receive signal");
        assert_eq!(received.signal_id, "s1");
        assert_eq!(received.direction, SignalDirection::Long);
    }

    #[tokio::test]
    async fn test_subscribe_and_publish_async() {
        let bus = SignalBus::new();
        let mut rx = bus.subscribe("rl.eth");

        let signal = make_signal("s2", "ETHUSDT.BINANCE", SignalDirection::Short, 0.6);
        bus.publish("rl.eth", signal.clone());

        let received = rx.recv().await.expect("should receive signal");
        assert_eq!(received.signal_id, "s2");
    }

    #[test]
    fn test_multiple_subscribers_same_topic() {
        let bus = SignalBus::new();
        let mut rx1 = bus.subscribe("topic_a");
        let mut rx2 = bus.subscribe("topic_a");

        let signal = make_signal("s3", "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);
        bus.publish("topic_a", signal);

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_get_latest() {
        let bus = SignalBus::new();
        assert!(bus.get_latest("topic_b").is_none());

        let signal1 = make_signal("s4", "BTCUSDT.BINANCE", SignalDirection::Long, 0.7);
        bus.publish("topic_b", signal1);
        let latest = bus.get_latest("topic_b").expect("should have cached signal");
        assert_eq!(latest.signal_id, "s4");

        let signal2 = make_signal("s5", "BTCUSDT.BINANCE", SignalDirection::Short, 0.9);
        bus.publish("topic_b", signal2);
        let latest = bus.get_latest("topic_b").expect("should have cached signal");
        assert_eq!(latest.signal_id, "s5");
    }

    #[test]
    fn test_get_latest_by_symbol() {
        let bus = SignalBus::new();

        let s1 = make_signal("s6", "BTCUSDT.BINANCE", SignalDirection::Long, 0.8);
        let s2 = make_signal("s7", "ETHUSDT.BINANCE", SignalDirection::Short, 0.6);
        let s3 = make_signal("s8", "BTCUSDT.BINANCE", SignalDirection::Neutral, 0.3);

        bus.publish("topic_btc_1", s1);
        bus.publish("topic_eth", s2);
        bus.publish("topic_btc_2", s3);

        let btc_signals = bus.get_latest_by_symbol("BTCUSDT.BINANCE");
        assert_eq!(btc_signals.len(), 2);

        let eth_signals = bus.get_latest_by_symbol("ETHUSDT.BINANCE");
        assert_eq!(eth_signals.len(), 1);

        let no_signals = bus.get_latest_by_symbol("SOLUSDT.BINANCE");
        assert!(no_signals.is_empty());
    }

    #[test]
    fn test_unsubscribe_removes_subscribers_and_cache() {
        let bus = SignalBus::new();
        let mut rx = bus.subscribe("topic_c");

        let signal = make_signal("s9", "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);
        bus.publish("topic_c", signal);
        assert!(rx.try_recv().is_ok());
        assert!(bus.get_latest("topic_c").is_some());

        bus.unsubscribe("topic_c");
        assert!(bus.get_latest("topic_c").is_none());
        assert_eq!(bus.subscriber_count("topic_c"), 0);
    }

    #[test]
    fn test_unsubscribe_closes_receiver() {
        let bus = SignalBus::new();
        let mut rx = bus.subscribe("topic_d");

        bus.unsubscribe("topic_d");

        // After unsubscribe, the receiver should be closed
        // Publishing to a topic with no subscribers is a no-op
        let signal = make_signal("s10", "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);
        bus.publish("topic_d", signal);

        // Receiver should return None since sender was dropped
        let result = rx.try_recv();
        assert!(result.is_err());
    }

    #[test]
    fn test_publish_to_nonexistent_topic() {
        let bus = SignalBus::new();
        let signal = make_signal("s11", "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);
        // Should not panic
        bus.publish("nonexistent", signal);
        // Latest should still be cached
        assert!(bus.get_latest("nonexistent").is_some());
    }

    #[test]
    fn test_dead_subscribers_are_pruned() {
        let bus = SignalBus::new();
        let rx1 = bus.subscribe("topic_e");
        let mut rx2 = bus.subscribe("topic_e");

        assert_eq!(bus.subscriber_count("topic_e"), 2);

        // Drop rx1 — the sender for rx1 will fail on next publish
        drop(rx1);

        let signal = make_signal("s12", "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);
        bus.publish("topic_e", signal);

        // rx1's dead sender should have been pruned
        assert_eq!(bus.subscriber_count("topic_e"), 1);
        assert!(rx2.try_recv().is_ok());
    }

    #[test]
    fn test_topics() {
        let bus = SignalBus::new();
        bus.subscribe("alpha");
        bus.subscribe("beta");
        bus.subscribe("gamma");

        let mut topics = bus.topics();
        topics.sort();
        assert_eq!(topics, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn test_cached_topics() {
        let bus = SignalBus::new();
        let signal = make_signal("s13", "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);

        bus.publish("topic_x", signal.clone());
        bus.publish("topic_y", signal);

        let mut cached = bus.cached_topics();
        cached.sort();
        assert_eq!(cached, vec!["topic_x", "topic_y"]);
    }

    #[test]
    fn test_subscriber_count() {
        let bus = SignalBus::new();
        assert_eq!(bus.subscriber_count("topic_f"), 0);

        bus.subscribe("topic_f");
        bus.subscribe("topic_f");
        assert_eq!(bus.subscriber_count("topic_f"), 2);
    }

    #[test]
    fn test_default_trait() {
        let bus = SignalBus::default();
        assert!(bus.topics().is_empty());
        assert!(bus.cached_topics().is_empty());
    }

    #[test]
    fn test_multiple_signals_fifo_order() {
        let bus = SignalBus::new();
        let mut rx = bus.subscribe("fifo_topic");

        for i in 0..5 {
            let signal = make_signal(&format!("fifo_{}", i), "BTCUSDT.BINANCE", SignalDirection::Long, 0.5);
            bus.publish("fifo_topic", signal);
        }

        for i in 0..5 {
            let received = rx.try_recv().expect("should receive signal");
            assert_eq!(received.signal_id, format!("fifo_{}", i));
        }
    }
}
