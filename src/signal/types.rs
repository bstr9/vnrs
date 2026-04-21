//! Signal types for the AI signal bus.
//!
//! Defines the core data structures for typed trading signals that bridge
//! AI signal sources (sentiment analysis, RL strategy output) with
//! traditional strategy consumers.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Direction of a trading signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalDirection {
    /// Bullish — expect price to rise.
    Long,
    /// Bearish — expect price to fall.
    Short,
    /// No directional bias.
    Neutral,
}

impl std::fmt::Display for SignalDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalDirection::Long => write!(f, "Long"),
            SignalDirection::Short => write!(f, "Short"),
            SignalDirection::Neutral => write!(f, "Neutral"),
        }
    }
}

/// Signal strength as a value in [0.0, 1.0].
///
/// Semantics:
/// - 0.0 → no signal / noise
/// - 1.0 → maximum conviction
pub type SignalStrength = f64;

/// A typed trading signal emitted by an AI source.
///
/// Strategies subscribe to topics on the `SignalBus` and receive `Signal`
/// instances they can combine with traditional indicators without directly
/// depending on model inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Unique identifier for this signal instance.
    pub signal_id: String,
    /// Source that produced this signal (e.g., "sentiment_v2", "rl_btc").
    pub source: String,
    /// Trading symbol this signal pertains to (e.g., "BTCUSDT.BINANCE").
    pub symbol: String,
    /// Directional bias of the signal.
    pub direction: SignalDirection,
    /// Signal strength in [0.0, 1.0].
    pub strength: SignalStrength,
    /// Confidence of the model in [0.0, 1.0].
    pub confidence: f64,
    /// Additional feature map (model-specific key-value pairs).
    pub features: HashMap<String, f64>,
    /// Version of the model that produced this signal.
    pub model_version: String,
    /// Timestamp when the signal was generated.
    pub timestamp: DateTime<Utc>,
}

impl Signal {
    /// Create a new signal with the given fields and a UTC timestamp.
    pub fn new(
        signal_id: impl Into<String>,
        source: impl Into<String>,
        symbol: impl Into<String>,
        direction: SignalDirection,
        strength: SignalStrength,
        confidence: f64,
    ) -> Self {
        Self {
            signal_id: signal_id.into(),
            source: source.into(),
            symbol: symbol.into(),
            direction,
            strength: strength.clamp(0.0, 1.0),
            confidence: confidence.clamp(0.0, 1.0),
            features: HashMap::new(),
            model_version: String::new(),
            timestamp: Utc::now(),
        }
    }

    /// Set the model version.
    pub fn with_model_version(mut self, version: impl Into<String>) -> Self {
        self.model_version = version.into();
        self
    }

    /// Add a feature key-value pair.
    pub fn with_feature(mut self, key: impl Into<String>, value: f64) -> Self {
        self.features.insert(key.into(), value);
        self
    }

    /// Set the features map, replacing any existing features.
    pub fn with_features(mut self, features: HashMap<String, f64>) -> Self {
        self.features = features;
        self
    }

    /// Set the timestamp explicitly.
    pub fn with_timestamp(mut self, timestamp: DateTime<Utc>) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Whether this signal carries a directional bias (Long or Short).
    pub fn is_directional(&self) -> bool {
        self.direction != SignalDirection::Neutral
    }

    /// Whether the signal strength exceeds a given threshold.
    pub fn is_stronger_than(&self, threshold: SignalStrength) -> bool {
        self.strength > threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_direction_display() {
        assert_eq!(SignalDirection::Long.to_string(), "Long");
        assert_eq!(SignalDirection::Short.to_string(), "Short");
        assert_eq!(SignalDirection::Neutral.to_string(), "Neutral");
    }

    #[test]
    fn test_signal_new_clamps_strength_and_confidence() {
        let signal = Signal::new("id1", "src", "BTCUSDT.BINANCE", SignalDirection::Long, 1.5, -0.2);
        assert!((signal.strength - 1.0).abs() < f64::EPSILON);
        assert!((signal.confidence - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_signal_builder_pattern() {
        let signal = Signal::new("id2", "sentiment", "ETHUSDT.BINANCE", SignalDirection::Short, 0.7, 0.85)
            .with_model_version("v3.1")
            .with_feature("sentiment_score", -0.65)
            .with_feature("volume_ratio", 1.3);

        assert_eq!(signal.signal_id, "id2");
        assert_eq!(signal.source, "sentiment");
        assert_eq!(signal.symbol, "ETHUSDT.BINANCE");
        assert_eq!(signal.direction, SignalDirection::Short);
        assert!((signal.strength - 0.7).abs() < f64::EPSILON);
        assert!((signal.confidence - 0.85).abs() < f64::EPSILON);
        assert_eq!(signal.model_version, "v3.1");
        assert_eq!(signal.features.len(), 2);
        assert!((signal.features["sentiment_score"] - (-0.65)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_is_directional() {
        let long_signal = Signal::new("id3", "src", "SYM", SignalDirection::Long, 0.5, 0.5);
        let neutral_signal = Signal::new("id4", "src", "SYM", SignalDirection::Neutral, 0.5, 0.5);
        assert!(long_signal.is_directional());
        assert!(!neutral_signal.is_directional());
    }

    #[test]
    fn test_is_stronger_than() {
        let signal = Signal::new("id5", "src", "SYM", SignalDirection::Long, 0.6, 0.5);
        assert!(signal.is_stronger_than(0.5));
        assert!(!signal.is_stronger_than(0.6));
        assert!(!signal.is_stronger_than(0.7));
    }

    #[test]
    fn test_signal_serialize_deserialize() {
        let signal = Signal::new("id6", "rl", "BTCUSDT.BINANCE", SignalDirection::Long, 0.8, 0.9)
            .with_model_version("v1")
            .with_feature("q_value", 0.42);

        let json = serde_json::to_string(&signal).expect("serialize");
        let deserialized: Signal = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.signal_id, "id6");
        assert_eq!(deserialized.source, "rl");
        assert_eq!(deserialized.direction, SignalDirection::Long);
        assert!((deserialized.strength - 0.8).abs() < f64::EPSILON);
        assert_eq!(deserialized.model_version, "v1");
        assert_eq!(deserialized.features["q_value"], 0.42);
    }
}
