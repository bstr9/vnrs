//! MessageBus — inter-strategy communication via pub/sub pattern.
//!
//! Allows Python strategies to communicate with each other by publishing
//! messages to named topics. Strategies subscribe to topics by name;
//! the bus routes messages to all subscribers of a given topic.
//!
//! This Python MessageBus is a thin wrapper around the Rust MessageBus
//! (from `crate::trader::message_bus`), delegating all operations to the
//! native implementation for better performance and unified state.
//!
//! ```python
//! # Signal strategy
//! self.message_bus.publish("SIGNAL.BTCUSDT", {"action": "BUY", "strength": "0.8"}, "signal_strategy")
//!
//! # Execution strategy
//! self.message_bus.subscribe("SIGNAL.BTCUSDT", "execution_strategy")
//! messages = self.message_bus.get_messages_for("execution_strategy")
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::trader::message_bus::{self, BusMessage};

// ---------------------------------------------------------------------------
// PyMessage — Python-facing message object
// ---------------------------------------------------------------------------

/// A message object exposed to Python.
///
/// ```python
/// msg = self.message_bus.get_messages_for("my_strategy")[0]
/// print(msg.topic, msg.sender, msg.timestamp)
/// print(msg.data["action"])
/// print(msg.to_dict())
/// ```
#[pyclass(name = "Message")]
#[derive(Clone)]
pub struct PyMessage {
    inner: BusMessage,
}

#[pymethods]
impl PyMessage {
    /// The topic this message was published to.
    #[getter]
    fn topic(&self) -> &str {
        &self.inner.topic
    }

    /// Message payload as a Python dict.
    #[getter]
    fn data(&self, py: Python) -> Py<PyDict> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.data {
            dict.set_item(k, v)
                .expect("setting string key-value in PyDict should not fail");
        }
        dict.into()
    }

    /// Name of the strategy that sent this message.
    #[getter]
    fn sender(&self) -> &str {
        &self.inner.sender
    }

    /// Timestamp in RFC 3339 format (e.g. "2026-04-16T12:34:56.789+00:00").
    #[getter]
    fn timestamp(&self) -> String {
        self.inner.timestamp.to_rfc3339()
    }

    /// Convert to a Python dict for convenient access.
    fn to_dict(&self, py: Python) -> Py<PyDict> {
        let dict = PyDict::new(py);
        dict.set_item("topic", &self.inner.topic)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("sender", &self.inner.sender)
            .expect("setting str key in PyDict should not fail");
        dict.set_item("timestamp", self.timestamp())
            .expect("setting str key in PyDict should not fail");

        let data_dict = PyDict::new(py);
        for (k, v) in &self.inner.data {
            data_dict
                .set_item(k, v)
                .expect("setting string key-value in PyDict should not fail");
        }
        dict.set_item("data", data_dict)
            .expect("setting dict key in PyDict should not fail");
        dict.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "Message(topic='{}', sender='{}', timestamp='{}', data={:?})",
            self.inner.topic,
            self.inner.sender,
            self.timestamp(),
            self.inner.data,
        )
    }
}

impl PyMessage {
    pub fn from_bus_message(msg: BusMessage) -> Self {
        Self { inner: msg }
    }
}

// ---------------------------------------------------------------------------
// MessageBus — thin wrapper around the Rust MessageBus
// ---------------------------------------------------------------------------

/// Pub/sub message bus for inter-strategy communication.
///
/// This is a thin Python-facing wrapper around the Rust `MessageBus`
/// (registered in `MainEngine`). All operations delegate to the native
/// implementation, so both Rust and Python strategies share the same
/// pub/sub state.
///
/// ```python
/// bus = MessageBus()
/// bus.subscribe("SIGNAL.BTCUSDT", "execution_strategy")
/// bus.publish("SIGNAL.BTCUSDT", {"action": "BUY"}, "signal_strategy")
/// messages = bus.get_messages_for("execution_strategy")
/// ```
#[pyclass(name = "MessageBus")]
pub struct MessageBus {
    inner: Arc<message_bus::MessageBus>,
}

impl MessageBus {
    /// Create a new empty message bus (standalone, not connected to MainEngine).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(message_bus::MessageBus::new()),
        }
    }

    /// Create a Python MessageBus wrapping the Rust MessageBus from MainEngine.
    pub fn from_rust_message_bus(inner: Arc<message_bus::MessageBus>) -> Self {
        Self { inner }
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[pymethods]
impl MessageBus {
    #[new]
    fn py_new() -> Self {
        Self::new()
    }

    /// Register a strategy for a topic.
    ///
    /// Args:
    ///     topic: The topic string (e.g. "SIGNAL.BTCUSDT")
    ///     strategy_name: Name of the subscribing strategy
    fn subscribe(&self, topic: &str, strategy_name: &str) {
        self.inner.subscribe(topic, strategy_name);
    }

    /// Unregister a strategy from a topic.
    ///
    /// Args:
    ///     topic: The topic string
    ///     strategy_name: Name of the strategy to unsubscribe
    fn unsubscribe(&self, topic: &str, strategy_name: &str) {
        self.inner.unsubscribe(topic, strategy_name);
    }

    /// Post a message to a topic. All subscribers will receive it.
    ///
    /// Args:
    ///     topic: The topic string
    ///     data: Message payload as a dict of string key-value pairs
    ///     sender: Name of the sending strategy
    fn publish(&self, topic: &str, data: HashMap<String, String>, sender: &str) {
        self.inner.publish(topic, data, sender);
    }

    /// Whether there are any pending (unread) messages in the queue.
    fn has_pending(&self) -> bool {
        self.inner.has_pending()
    }

    /// Number of pending messages in the queue.
    fn pending_count(&self) -> usize {
        self.inner.pending_count()
    }

    /// Get all pending messages for a strategy, removing them from the queue.
    ///
    /// Args:
    ///     strategy_name: Name of the strategy to retrieve messages for
    ///
    /// Returns:
    ///     List of Message objects destined for this strategy.
    fn get_messages_for(&self, strategy_name: &str) -> Vec<PyMessage> {
        self.inner
            .get_messages_for(strategy_name)
            .into_iter()
            .map(PyMessage::from_bus_message)
            .collect()
    }

    /// Clear all pending messages from the queue.
    fn clear(&self) {
        self.inner.clear();
    }

    /// List all active topics (those with at least one subscriber).
    fn topics(&self) -> Vec<String> {
        self.inner.topics()
    }

    /// List subscriber strategy names for a given topic.
    ///
    /// Args:
    ///     topic: The topic string
    ///
    /// Returns:
    ///     List of strategy names subscribed to this topic.
    fn subscribers_of(&self, topic: &str) -> Vec<String> {
        self.inner.subscribers_of(topic)
    }

    fn __repr__(&self) -> String {
        format!(
            "MessageBus(topics={}, pending={})",
            self.inner.topics().len(),
            self.inner.pending_count(),
        )
    }
}

// ---------------------------------------------------------------------------
// Registration helper (called from bindings.rs)
// ---------------------------------------------------------------------------

/// Register MessageBus and PyMessage with the PyO3 module.
pub fn register_message_bus_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<MessageBus>()?;
    m.add_class::<PyMessage>()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

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
        assert_eq!(messages[0].inner.topic, "SIGNAL.BTCUSDT");
        assert_eq!(messages[0].inner.sender, "signal_strategy");
        assert_eq!(messages[0].inner.data.get("action").unwrap(), "BUY");
        assert_eq!(messages[0].inner.data.get("strength").unwrap(), "0.8");

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
        assert_eq!(msgs_a[0].inner.data["action"], "SELL");

        let msgs_b = bus.get_messages_for("exec_b");
        assert_eq!(msgs_b.len(), 1);
        assert_eq!(msgs_b[0].inner.data["action"], "SELL");

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
        assert_eq!(msgs_a[0].inner.data["action"], "SELL");

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
        assert_eq!(msgs[0].inner.data["action"], "BUY");
        assert_eq!(msgs[1].inner.data["action"], "SELL");
        assert_eq!(msgs[2].inner.data["action"], "HOLD");
        assert_eq!(msgs[0].inner.sender, "signal_1");
        assert_eq!(msgs[1].inner.sender, "signal_2");
        assert_eq!(msgs[2].inner.sender, "signal_3");

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

        let ts_str = msgs[0].inner.timestamp.to_rfc3339();
        // Verify it parses back as a valid RFC 3339 datetime
        let parsed = DateTime::parse_from_rfc3339(&ts_str);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_message_bus_wrapper_pattern() {
        let bus = MessageBus::new();
        bus.subscribe("TOPIC", "strat");
        bus.publish(
            "TOPIC",
            HashMap::from([("k".to_string(), "v".to_string())]),
            "sender",
        );

        assert!(bus.has_pending());
        assert_eq!(bus.pending_count(), 1);

        let msgs = bus.get_messages_for("strat");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].inner.data["k"], "v");
        assert!(!bus.has_pending());
    }

    #[test]
    fn test_from_rust_message_bus() {
        // Verify that wrapping a Rust MessageBus works correctly
        let rust_bus = Arc::new(message_bus::MessageBus::new());
        let py_bus = MessageBus::from_rust_message_bus(rust_bus.clone());

        py_bus.subscribe("TOPIC", "strat");
        py_bus.publish(
            "TOPIC",
            HashMap::from([("action".to_string(), "BUY".to_string())]),
            "sender",
        );

        assert!(py_bus.has_pending());
        assert_eq!(py_bus.pending_count(), 1);

        let msgs = py_bus.get_messages_for("strat");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].inner.data["action"], "BUY");

        // Also verify that the Rust bus sees the same state
        assert!(!rust_bus.has_pending()); // messages were consumed
        assert_eq!(rust_bus.subscribers_of("TOPIC"), vec!["strat"]);
    }
}
