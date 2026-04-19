//! MessageBus — inter-component communication via pub/sub pattern.
//!
//! Allows strategies and engine components to communicate by publishing
//! messages to named topics. Components subscribe to topics by name;
//! the bus routes messages to all subscribers of a given topic.
//!
//! ```
//! use trade_engine::trader::message_bus::MessageBus;
//! use std::collections::HashMap;
//!
//! let bus = MessageBus::new();
//! bus.subscribe("SIGNAL.BTCUSDT", "execution_strategy");
//! bus.publish("SIGNAL.BTCUSDT", HashMap::from([("action".to_string(), "BUY".to_string())]), "signal_strategy");
//!
//! let messages = bus.get_messages_for("execution_strategy");
//! assert_eq!(messages.len(), 1);
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use tracing::warn;

use super::engine::BaseEngine;
use super::gateway::GatewayEvent;

// ---------------------------------------------------------------------------
// BusMessage — internal data holder
// ---------------------------------------------------------------------------

/// A message in the bus, destined for a specific recipient.
#[derive(Debug, Clone)]
pub struct BusMessage {
    /// The topic this message was published to.
    pub topic: String,
    /// Message payload as string key-value pairs.
    pub data: HashMap<String, String>,
    /// Name of the component that sent this message.
    pub sender: String,
    /// Timestamp when the message was published.
    pub timestamp: DateTime<Utc>,
    /// The component this message is destined for (set at enqueue time).
    pub recipient: String,
}

// ---------------------------------------------------------------------------
// MessageBus — native Rust pub/sub message bus
// ---------------------------------------------------------------------------

/// Native Rust message bus for inter-component communication via pub/sub.
///
/// This is the core implementation — the Python `MessageBus` wraps this.
/// Thread safety is provided by `std::sync::RwLock` with lock-poisoning
/// recovery (falls back to `into_inner` on poison).
pub struct MessageBus {
    /// Topic → list of subscriber names.
    subscribers: RwLock<HashMap<String, Vec<String>>>,
    /// Pending messages awaiting retrieval.
    queue: RwLock<VecDeque<BusMessage>>,
}

impl MessageBus {
    /// Create a new empty message bus.
    pub fn new() -> Self {
        Self {
            subscribers: RwLock::new(HashMap::new()),
            queue: RwLock::new(VecDeque::new()),
        }
    }

    /// Subscribe a component to a topic. No-op if already subscribed.
    pub fn subscribe(&self, topic: &str, subscriber_name: &str) {
        let mut subs = self.subscribers.write().unwrap_or_else(|e| {
            warn!("MessageBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        let entry = subs.entry(topic.to_string()).or_default();
        if !entry.contains(&subscriber_name.to_string()) {
            entry.push(subscriber_name.to_string());
        }
    }

    /// Unsubscribe a component from a topic. Removes the topic entry if empty.
    pub fn unsubscribe(&self, topic: &str, subscriber_name: &str) {
        let mut subs = self.subscribers.write().unwrap_or_else(|e| {
            warn!("MessageBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        if let Some(list) = subs.get_mut(topic) {
            list.retain(|s| s != subscriber_name);
            if list.is_empty() {
                subs.remove(topic);
            }
        }
    }

    /// Publish a message to a topic. All current subscribers receive it
    /// (the message is enqueued once per subscriber with recipient set).
    pub fn publish(&self, topic: &str, data: HashMap<String, String>, sender: &str) {
        let subs = self.subscribers.read().unwrap_or_else(|e| {
            warn!("MessageBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        if let Some(subscriber_list) = subs.get(topic) {
            let mut queue = self.queue.write().unwrap_or_else(|e| {
                warn!("MessageBus queue lock poisoned, recovering");
                e.into_inner()
            });
            for subscriber_name in subscriber_list {
                let msg = BusMessage {
                    topic: topic.to_string(),
                    data: data.clone(),
                    sender: sender.to_string(),
                    timestamp: Utc::now(),
                    recipient: subscriber_name.clone(),
                };
                queue.push_back(msg);
            }
        }
    }

    /// Retrieve and remove all pending messages for a given subscriber.
    /// Only returns messages whose `recipient` matches the subscriber name.
    pub fn get_messages_for(&self, subscriber_name: &str) -> Vec<BusMessage> {
        let mut queue = self.queue.write().unwrap_or_else(|e| {
            warn!("MessageBus queue lock poisoned, recovering");
            e.into_inner()
        });
        let mut result = Vec::new();
        let mut remaining = VecDeque::new();

        while let Some(msg) = queue.pop_front() {
            if msg.recipient == subscriber_name {
                result.push(msg);
            } else {
                remaining.push_back(msg);
            }
        }

        *queue = remaining;
        result
    }

    /// Whether there are any pending messages in the queue.
    pub fn has_pending(&self) -> bool {
        let queue = self.queue.read().unwrap_or_else(|e| {
            warn!("MessageBus queue lock poisoned, recovering");
            e.into_inner()
        });
        !queue.is_empty()
    }

    /// Number of pending messages.
    pub fn pending_count(&self) -> usize {
        let queue = self.queue.read().unwrap_or_else(|e| {
            warn!("MessageBus queue lock poisoned, recovering");
            e.into_inner()
        });
        queue.len()
    }

    /// Clear all pending messages.
    pub fn clear(&self) {
        let mut queue = self.queue.write().unwrap_or_else(|e| {
            warn!("MessageBus queue lock poisoned, recovering");
            e.into_inner()
        });
        queue.clear();
    }

    /// List all active topics (topics that have at least one subscriber).
    pub fn topics(&self) -> Vec<String> {
        let subs = self.subscribers.read().unwrap_or_else(|e| {
            warn!("MessageBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        subs.keys().cloned().collect()
    }

    /// List subscribers for a given topic.
    pub fn subscribers_of(&self, topic: &str) -> Vec<String> {
        let subs = self.subscribers.read().unwrap_or_else(|e| {
            warn!("MessageBus subscribers lock poisoned, recovering");
            e.into_inner()
        });
        subs.get(topic).cloned().unwrap_or_default()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for MessageBus {
    fn engine_name(&self) -> &str {
        "MessageBus"
    }

    fn process_event(&self, _event_type: &str, _event: &GatewayEvent) {
        // MessageBus does not process gateway events directly.
    }

    fn close(&self) {
        self.clear();
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_publish_receive() {
        let bus = MessageBus::new();
        bus.subscribe("SIGNAL.BTCUSDT", "execution_strategy");
        bus.publish(
            "SIGNAL.BTCUSDT",
            HashMap::from([
                ("action".to_string(), "BUY".to_string()),
                ("strength".to_string(), "0.8".to_string()),
            ]),
            "signal_strategy",
        );

        assert!(bus.has_pending());
        assert_eq!(bus.pending_count(), 1);

        let messages = bus.get_messages_for("execution_strategy");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].topic, "SIGNAL.BTCUSDT");
        assert_eq!(messages[0].sender, "signal_strategy");
        assert_eq!(messages[0].data.get("action").unwrap(), "BUY");
        assert_eq!(messages[0].data.get("strength").unwrap(), "0.8");

        // Messages should be removed after retrieval
        assert!(!bus.has_pending());
        assert_eq!(bus.pending_count(), 0);
    }

    #[test]
    fn test_multiple_subscribers_same_topic() {
        let bus = MessageBus::new();
        bus.subscribe("SIGNAL.BTCUSDT", "exec_a");
        bus.subscribe("SIGNAL.BTCUSDT", "exec_b");

        bus.publish(
            "SIGNAL.BTCUSDT",
            HashMap::from([("action".to_string(), "SELL".to_string())]),
            "signal_strategy",
        );

        // Two messages in queue (one per subscriber)
        assert_eq!(bus.pending_count(), 2);

        let msgs_a = bus.get_messages_for("exec_a");
        assert_eq!(msgs_a.len(), 1);
        assert_eq!(msgs_a[0].data["action"], "SELL");

        let msgs_b = bus.get_messages_for("exec_b");
        assert_eq!(msgs_b.len(), 1);
        assert_eq!(msgs_b[0].data["action"], "SELL");

        assert!(!bus.has_pending());
    }

    #[test]
    fn test_unsubscribe_stops_delivery() {
        let bus = MessageBus::new();
        bus.subscribe("SIGNAL.ETH", "exec_a");
        bus.subscribe("SIGNAL.ETH", "exec_b");

        bus.publish(
            "SIGNAL.ETH",
            HashMap::from([("action".to_string(), "BUY".to_string())]),
            "signal",
        );
        assert_eq!(bus.pending_count(), 2);

        // Retrieve messages to clear queue
        let _ = bus.get_messages_for("exec_a");
        let _ = bus.get_messages_for("exec_b");

        // Unsubscribe exec_b
        bus.unsubscribe("SIGNAL.ETH", "exec_b");

        bus.publish(
            "SIGNAL.ETH",
            HashMap::from([("action".to_string(), "SELL".to_string())]),
            "signal",
        );

        // Only one message now (for exec_a only)
        assert_eq!(bus.pending_count(), 1);

        let msgs_a = bus.get_messages_for("exec_a");
        assert_eq!(msgs_a.len(), 1);
        assert_eq!(msgs_a[0].data["action"], "SELL");

        let msgs_b = bus.get_messages_for("exec_b");
        assert!(msgs_b.is_empty());
    }

    #[test]
    fn test_empty_topic_returns_no_messages() {
        let bus = MessageBus::new();
        // No one subscribed to "NONEXISTENT"
        bus.publish("NONEXISTENT", HashMap::new(), "nobody");

        assert!(!bus.has_pending());
        assert_eq!(bus.pending_count(), 0);

        let msgs = bus.get_messages_for("any_strategy");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_multiple_messages_queue_correctly() {
        let bus = MessageBus::new();
        bus.subscribe("SIGNAL.BTC", "exec");

        bus.publish(
            "SIGNAL.BTC",
            HashMap::from([("action".to_string(), "BUY".to_string())]),
            "signal_1",
        );
        bus.publish(
            "SIGNAL.BTC",
            HashMap::from([("action".to_string(), "SELL".to_string())]),
            "signal_2",
        );
        bus.publish(
            "SIGNAL.BTC",
            HashMap::from([("action".to_string(), "HOLD".to_string())]),
            "signal_3",
        );

        assert_eq!(bus.pending_count(), 3);

        let msgs = bus.get_messages_for("exec");
        assert_eq!(msgs.len(), 3);
        // Order is preserved (FIFO)
        assert_eq!(msgs[0].data["action"], "BUY");
        assert_eq!(msgs[1].data["action"], "SELL");
        assert_eq!(msgs[2].data["action"], "HOLD");
        assert_eq!(msgs[0].sender, "signal_1");
        assert_eq!(msgs[1].sender, "signal_2");
        assert_eq!(msgs[2].sender, "signal_3");

        assert!(!bus.has_pending());
    }

    #[test]
    fn test_topics_and_subscribers() {
        let bus = MessageBus::new();
        bus.subscribe("SIGNAL.BTC", "exec_a");
        bus.subscribe("SIGNAL.BTC", "exec_b");
        bus.subscribe("SIGNAL.ETH", "exec_c");

        let mut topics = bus.topics();
        topics.sort();
        assert_eq!(topics, vec!["SIGNAL.BTC", "SIGNAL.ETH"]);

        let mut subs_btc = bus.subscribers_of("SIGNAL.BTC");
        subs_btc.sort();
        assert_eq!(subs_btc, vec!["exec_a", "exec_b"]);

        assert_eq!(bus.subscribers_of("SIGNAL.ETH"), vec!["exec_c"]);
        assert!(bus.subscribers_of("NONEXISTENT").is_empty());
    }

    #[test]
    fn test_clear() {
        let bus = MessageBus::new();
        bus.subscribe("TOPIC", "strat");
        bus.publish("TOPIC", HashMap::new(), "sender");

        assert_eq!(bus.pending_count(), 1);
        bus.clear();
        assert_eq!(bus.pending_count(), 0);
        assert!(!bus.has_pending());
    }

    #[test]
    fn test_subscribe_idempotent() {
        let bus = MessageBus::new();
        bus.subscribe("TOPIC", "strat");
        bus.subscribe("TOPIC", "strat"); // duplicate

        assert_eq!(bus.subscribers_of("TOPIC"), vec!["strat"]);

        bus.publish("TOPIC", HashMap::new(), "sender");
        // Only one message despite double-subscribe
        assert_eq!(bus.pending_count(), 1);
    }

    #[test]
    fn test_unsubscribe_removes_empty_topic() {
        let bus = MessageBus::new();
        bus.subscribe("TOPIC", "strat");
        assert_eq!(bus.topics(), vec!["TOPIC"]);

        bus.unsubscribe("TOPIC", "strat");
        assert!(bus.topics().is_empty());
    }

    #[test]
    fn test_timestamp_is_rfc3339() {
        let bus = MessageBus::new();
        bus.subscribe("TOPIC", "strat");
        bus.publish("TOPIC", HashMap::new(), "sender");

        let msgs = bus.get_messages_for("strat");
        assert_eq!(msgs.len(), 1);

        let ts_str = msgs[0].timestamp.to_rfc3339();
        // Verify it parses back as a valid RFC 3339 datetime
        let parsed = DateTime::parse_from_rfc3339(&ts_str);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_base_engine_trait() {
        let bus = MessageBus::new();
        assert_eq!(bus.engine_name(), "MessageBus");

        bus.subscribe("TOPIC", "strat");
        bus.publish("TOPIC", HashMap::new(), "sender");
        assert_eq!(bus.pending_count(), 1);

        // close() should clear pending messages
        bus.close();
        assert_eq!(bus.pending_count(), 0);
        assert!(!bus.has_pending());
    }

    #[test]
    fn test_default_trait() {
        let bus = MessageBus::default();
        assert!(!bus.has_pending());
        assert_eq!(bus.pending_count(), 0);
        assert!(bus.topics().is_empty());
    }
}
