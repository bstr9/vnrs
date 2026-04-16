//! MessageBus — inter-strategy communication via pub/sub pattern.
//!
//! Allows Python strategies to communicate with each other by publishing
//! messages to named topics. Strategies subscribe to topics by name;
//! the bus routes messages to all subscribers of a given topic.
//!
//! ```python
//! # Signal strategy
//! self.message_bus.publish("SIGNAL.BTCUSDT", {"action": "BUY", "strength": "0.8"}, "signal_strategy")
//!
//! # Execution strategy
//! self.message_bus.subscribe("SIGNAL.BTCUSDT", "execution_strategy")
//! messages = self.message_bus.get_messages_for("execution_strategy")
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use pyo3::prelude::*;
use pyo3::types::PyDict;

// ---------------------------------------------------------------------------
// Message — internal data holder
// ---------------------------------------------------------------------------

/// Internal message representation.
#[derive(Debug, Clone)]
pub struct Message {
    pub topic: String,
    pub data: HashMap<String, String>,
    pub sender: String,
    pub timestamp: DateTime<Utc>,
    /// The strategy this message is destined for (set at enqueue time).
    pub recipient: String,
}

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
    inner: Message,
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
    pub fn from_message(msg: Message) -> Self {
        Self { inner: msg }
    }
}

// ---------------------------------------------------------------------------
// MessageBusInner — mutable inner state protected by Mutex
// ---------------------------------------------------------------------------

/// Inner state of the message bus.
#[derive(Debug, Clone, Default)]
pub struct MessageBusInner {
    /// Topic → list of strategy names subscribed to that topic.
    pub subscribers: HashMap<String, Vec<String>>,
    /// Pending messages awaiting retrieval.
    pub queue: VecDeque<Message>,
}

impl MessageBusInner {
    /// Subscribe a strategy to a topic. No-op if already subscribed.
    fn subscribe(&mut self, topic: &str, strategy_name: &str) {
        let subs = self.subscribers.entry(topic.to_string()).or_default();
        if !subs.contains(&strategy_name.to_string()) {
            subs.push(strategy_name.to_string());
        }
    }

    /// Unsubscribe a strategy from a topic. Removes the topic entry if empty.
    fn unsubscribe(&mut self, topic: &str, strategy_name: &str) {
        if let Some(subs) = self.subscribers.get_mut(topic) {
            subs.retain(|s| s != strategy_name);
            if subs.is_empty() {
                self.subscribers.remove(topic);
            }
        }
    }

    /// Publish a message to a topic. All current subscribers receive it
    /// (the message is enqueued once per subscriber with recipient set).
    fn publish(&mut self, topic: &str, data: HashMap<String, String>, sender: &str) {
        if let Some(subs) = self.subscribers.get(topic) {
            for strategy_name in subs {
                let msg = Message {
                    topic: topic.to_string(),
                    data: data.clone(),
                    sender: sender.to_string(),
                    timestamp: Utc::now(),
                    recipient: strategy_name.clone(),
                };
                self.queue.push_back(msg);
            }
        }
    }

    /// Whether there are any pending messages in the queue.
    fn has_pending(&self) -> bool {
        !self.queue.is_empty()
    }

    /// Number of pending messages.
    fn pending_count(&self) -> usize {
        self.queue.len()
    }

    /// Retrieve and remove all pending messages for a given strategy.
    /// Only returns messages whose `recipient` matches the strategy name.
    fn get_messages_for(&mut self, strategy_name: &str) -> Vec<Message> {
        let mut result = Vec::new();
        let mut remaining = VecDeque::new();

        while let Some(msg) = self.queue.pop_front() {
            if msg.recipient == strategy_name {
                result.push(msg);
            } else {
                remaining.push_back(msg);
            }
        }

        self.queue = remaining;
        result
    }

    /// Clear all pending messages.
    fn clear(&mut self) {
        self.queue.clear();
    }

    /// List all active topics (topics that have at least one subscriber).
    fn topics(&self) -> Vec<String> {
        self.subscribers.keys().cloned().collect()
    }

    /// List subscribers for a given topic.
    fn subscribers_of(&self, topic: &str) -> Vec<String> {
        self.subscribers.get(topic).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// MessageBus — the main PyO3 class exposed to Python strategies
// ---------------------------------------------------------------------------

/// Pub/sub message bus for inter-strategy communication.
///
/// ```python
/// bus = MessageBus()
/// bus.subscribe("SIGNAL.BTCUSDT", "execution_strategy")
/// bus.publish("SIGNAL.BTCUSDT", {"action": "BUY"}, "signal_strategy")
/// messages = bus.get_messages_for("execution_strategy")
/// ```
#[pyclass(name = "MessageBus")]
pub struct MessageBus {
    inner: Arc<Mutex<MessageBusInner>>,
}

impl MessageBus {
    /// Create a new empty message bus.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MessageBusInner::default())),
        }
    }

    /// Create a bus backed by a shared `MessageBusInner`.
    pub fn from_inner(inner: Arc<Mutex<MessageBusInner>>) -> Self {
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
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .subscribe(topic, strategy_name);
    }

    /// Unregister a strategy from a topic.
    ///
    /// Args:
    ///     topic: The topic string
    ///     strategy_name: Name of the strategy to unsubscribe
    fn unsubscribe(&self, topic: &str, strategy_name: &str) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unsubscribe(topic, strategy_name);
    }

    /// Post a message to a topic. All subscribers will receive it.
    ///
    /// Args:
    ///     topic: The topic string
    ///     data: Message payload as a dict of string key-value pairs
    ///     sender: Name of the sending strategy
    fn publish(&self, topic: &str, data: HashMap<String, String>, sender: &str) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .publish(topic, data, sender);
    }

    /// Whether there are any pending (unread) messages in the queue.
    fn has_pending(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .has_pending()
    }

    /// Number of pending messages in the queue.
    fn pending_count(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_count()
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
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_messages_for(strategy_name)
            .into_iter()
            .map(PyMessage::from_message)
            .collect()
    }

    /// Clear all pending messages from the queue.
    fn clear(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// List all active topics (those with at least one subscriber).
    fn topics(&self) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .topics()
    }

    /// List subscriber strategy names for a given topic.
    ///
    /// Args:
    ///     topic: The topic string
    ///
    /// Returns:
    ///     List of strategy names subscribed to this topic.
    fn subscribers_of(&self, topic: &str) -> Vec<String> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .subscribers_of(topic)
    }

    fn __repr__(&self) -> String {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "MessageBus(topics={}, pending={})",
            inner.subscribers.len(),
            inner.queue.len(),
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

    #[test]
    fn test_subscribe_publish_receive() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("SIGNAL.BTCUSDT", "execution_strategy");
        inner.publish(
            "SIGNAL.BTCUSDT",
            HashMap::from([
                ("action".to_string(), "BUY".to_string()),
                ("strength".to_string(), "0.8".to_string()),
            ]),
            "signal_strategy",
        );

        assert!(inner.has_pending());
        assert_eq!(inner.pending_count(), 1);

        let messages = inner.get_messages_for("execution_strategy");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].topic, "SIGNAL.BTCUSDT");
        assert_eq!(messages[0].sender, "signal_strategy");
        assert_eq!(messages[0].data.get("action").unwrap(), "BUY");
        assert_eq!(messages[0].data.get("strength").unwrap(), "0.8");

        // Messages should be removed after retrieval
        assert!(!inner.has_pending());
        assert_eq!(inner.pending_count(), 0);
    }

    #[test]
    fn test_multiple_subscribers_same_topic() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("SIGNAL.BTCUSDT", "exec_a");
        inner.subscribe("SIGNAL.BTCUSDT", "exec_b");

        inner.publish(
            "SIGNAL.BTCUSDT",
            HashMap::from([("action".to_string(), "SELL".to_string())]),
            "signal_strategy",
        );

        // Two messages in queue (one per subscriber)
        assert_eq!(inner.pending_count(), 2);

        let msgs_a = inner.get_messages_for("exec_a");
        assert_eq!(msgs_a.len(), 1);
        assert_eq!(msgs_a[0].data["action"], "SELL");

        let msgs_b = inner.get_messages_for("exec_b");
        assert_eq!(msgs_b.len(), 1);
        assert_eq!(msgs_b[0].data["action"], "SELL");

        assert!(!inner.has_pending());
    }

    #[test]
    fn test_unsubscribe_stops_delivery() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("SIGNAL.ETH", "exec_a");
        inner.subscribe("SIGNAL.ETH", "exec_b");

        inner.publish(
            "SIGNAL.ETH",
            HashMap::from([("action".to_string(), "BUY".to_string())]),
            "signal",
        );
        assert_eq!(inner.pending_count(), 2);

        // Retrieve messages to clear queue
        let _ = inner.get_messages_for("exec_a");
        let _ = inner.get_messages_for("exec_b");

        // Unsubscribe exec_b
        inner.unsubscribe("SIGNAL.ETH", "exec_b");

        inner.publish(
            "SIGNAL.ETH",
            HashMap::from([("action".to_string(), "SELL".to_string())]),
            "signal",
        );

        // Only one message now (for exec_a only)
        assert_eq!(inner.pending_count(), 1);

        let msgs_a = inner.get_messages_for("exec_a");
        assert_eq!(msgs_a.len(), 1);
        assert_eq!(msgs_a[0].data["action"], "SELL");

        let msgs_b = inner.get_messages_for("exec_b");
        assert!(msgs_b.is_empty());
    }

    #[test]
    fn test_empty_topic_returns_no_messages() {
        let mut inner = MessageBusInner::default();
        // No one subscribed to "NONEXISTENT"
        inner.publish("NONEXISTENT", HashMap::new(), "nobody");

        assert!(!inner.has_pending());
        assert_eq!(inner.pending_count(), 0);

        let msgs = inner.get_messages_for("any_strategy");
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_multiple_messages_queue_correctly() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("SIGNAL.BTC", "exec");

        inner.publish(
            "SIGNAL.BTC",
            HashMap::from([("action".to_string(), "BUY".to_string())]),
            "signal_1",
        );
        inner.publish(
            "SIGNAL.BTC",
            HashMap::from([("action".to_string(), "SELL".to_string())]),
            "signal_2",
        );
        inner.publish(
            "SIGNAL.BTC",
            HashMap::from([("action".to_string(), "HOLD".to_string())]),
            "signal_3",
        );

        assert_eq!(inner.pending_count(), 3);

        let msgs = inner.get_messages_for("exec");
        assert_eq!(msgs.len(), 3);
        // Order is preserved (FIFO)
        assert_eq!(msgs[0].data["action"], "BUY");
        assert_eq!(msgs[1].data["action"], "SELL");
        assert_eq!(msgs[2].data["action"], "HOLD");
        assert_eq!(msgs[0].sender, "signal_1");
        assert_eq!(msgs[1].sender, "signal_2");
        assert_eq!(msgs[2].sender, "signal_3");

        assert!(!inner.has_pending());
    }

    #[test]
    fn test_topics_and_subscribers() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("SIGNAL.BTC", "exec_a");
        inner.subscribe("SIGNAL.BTC", "exec_b");
        inner.subscribe("SIGNAL.ETH", "exec_c");

        let mut topics = inner.topics();
        topics.sort();
        assert_eq!(topics, vec!["SIGNAL.BTC", "SIGNAL.ETH"]);

        let mut subs_btc = inner.subscribers_of("SIGNAL.BTC");
        subs_btc.sort();
        assert_eq!(subs_btc, vec!["exec_a", "exec_b"]);

        assert_eq!(inner.subscribers_of("SIGNAL.ETH"), vec!["exec_c"]);
        assert!(inner.subscribers_of("NONEXISTENT").is_empty());
    }

    #[test]
    fn test_clear() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("TOPIC", "strat");
        inner.publish("TOPIC", HashMap::new(), "sender");

        assert_eq!(inner.pending_count(), 1);
        inner.clear();
        assert_eq!(inner.pending_count(), 0);
        assert!(!inner.has_pending());
    }

    #[test]
    fn test_subscribe_idempotent() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("TOPIC", "strat");
        inner.subscribe("TOPIC", "strat"); // duplicate

        assert_eq!(inner.subscribers_of("TOPIC"), vec!["strat"]);

        inner.publish("TOPIC", HashMap::new(), "sender");
        // Only one message despite double-subscribe
        assert_eq!(inner.pending_count(), 1);
    }

    #[test]
    fn test_unsubscribe_removes_empty_topic() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("TOPIC", "strat");
        assert_eq!(inner.topics(), vec!["TOPIC"]);

        inner.unsubscribe("TOPIC", "strat");
        assert!(inner.topics().is_empty());
    }

    #[test]
    fn test_timestamp_is_rfc3339() {
        let mut inner = MessageBusInner::default();
        inner.subscribe("TOPIC", "strat");
        inner.publish("TOPIC", HashMap::new(), "sender");

        let msgs = inner.get_messages_for("strat");
        assert_eq!(msgs.len(), 1);

        let ts_str = msgs[0].timestamp.to_rfc3339();
        // Verify it parses back as a valid RFC 3339 datetime
        let parsed = DateTime::parse_from_rfc3339(&ts_str);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_message_bus_arc_mutex_pattern() {
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
}
