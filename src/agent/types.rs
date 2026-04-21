//! Type definitions for the Agent Layer module.
//!
//! Contains core data types for agent lifecycle management:
//! agent types, configuration, observations, decisions, and results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use crate::trader::Direction;

// ---------------------------------------------------------------------------
// AgentType — classification of agent roles
// ---------------------------------------------------------------------------

/// Classification of agent roles in the trading system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// Analyzes market sentiment from news, social media, etc.
    SentimentAnalyst,
    /// Performs technical analysis on price/volume data.
    TechnicalAnalyst,
    /// Evaluates portfolio risk and suggests adjustments.
    RiskAssessor,
    /// Reinforcement-learning based trading agent.
    RLTrader,
    /// Optimizes order execution (slippage, timing).
    ExecutionOptimizer,
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentType::SentimentAnalyst => write!(f, "sentiment_analyst"),
            AgentType::TechnicalAnalyst => write!(f, "technical_analyst"),
            AgentType::RiskAssessor => write!(f, "risk_assessor"),
            AgentType::RLTrader => write!(f, "rl_trader"),
            AgentType::ExecutionOptimizer => write!(f, "execution_optimizer"),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentConfig — agent configuration
// ---------------------------------------------------------------------------

/// Configuration for an agent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Unique name for this agent instance.
    pub name: String,
    /// The type of agent.
    pub agent_type: AgentType,
    /// Whether the agent is enabled.
    pub enabled: bool,
    /// Maximum observation history to retain.
    pub max_history: usize,
    /// Additional parameters (e.g., thresholds, model hints).
    pub params: HashMap<String, serde_json::Value>,
}

impl AgentConfig {
    /// Create a new `AgentConfig` with the given name and type.
    pub fn new(name: impl Into<String>, agent_type: AgentType) -> Self {
        Self {
            name: name.into(),
            agent_type,
            enabled: true,
            max_history: 100,
            params: HashMap::new(),
        }
    }

    /// Set a parameter value.
    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    /// Get a parameter value by key.
    pub fn get_param(&self, key: &str) -> Option<&serde_json::Value> {
        self.params.get(key)
    }
}

// ---------------------------------------------------------------------------
// Observation — data collected by an agent
// ---------------------------------------------------------------------------

/// Data collected by an agent during the observe phase.
///
/// Contains a timestamp and a flexible map of observed data points
/// (prices, volumes, indicators, news items, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// When the observation was made (UTC).
    pub timestamp: DateTime<Utc>,
    /// Observed data as key-value pairs.
    pub data: HashMap<String, serde_json::Value>,
}

impl Observation {
    /// Create a new observation with the current timestamp.
    pub fn new() -> Self {
        Self {
            timestamp: Utc::now(),
            data: HashMap::new(),
        }
    }

    /// Insert a data point.
    pub fn insert(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.data.insert(key.into(), value);
    }

    /// Get a data point by key.
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    /// Number of data points in the observation.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the observation is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Default for Observation {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Decision — agent output
// ---------------------------------------------------------------------------

/// A decision produced by an agent during the decide phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Decision {
    /// No action required.
    NoAction,
    /// Risk adjustment suggestion.
    RiskAdjustment {
        /// Description of the risk analysis.
        analysis: String,
        /// Suggested action to mitigate risk.
        suggested_action: String,
    },
    /// Sentiment signal from market analysis.
    SentimentSignal {
        /// Sentiment label (e.g., "bullish", "bearish", "neutral").
        sentiment: String,
        /// Confidence in [0.0, 1.0].
        confidence: f64,
    },
    /// Trade signal with direction and strength.
    TradeSignal {
        /// Trading symbol (e.g., "BTCUSDT").
        symbol: String,
        /// Trade direction.
        direction: Direction,
        /// Signal strength in [0.0, 1.0].
        strength: f64,
    },
}

impl Decision {
    /// Returns true if this decision is `NoAction`.
    pub fn is_no_action(&self) -> bool {
        matches!(self, Decision::NoAction)
    }
}

// ---------------------------------------------------------------------------
// DecisionResult — feedback on a decision
// ---------------------------------------------------------------------------

/// Result of executing (or not executing) a decision.
///
/// Used in the feedback phase to inform the agent whether its
/// decision was acted upon and what the outcome was.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResult {
    /// The decision that was evaluated.
    pub decision: Decision,
    /// Whether the decision was executed.
    pub executed: bool,
    /// Optional outcome description (e.g., "profitable", "stopped out").
    pub outcome: Option<String>,
}

impl DecisionResult {
    /// Create a new `DecisionResult`.
    pub fn new(decision: Decision, executed: bool, outcome: Option<String>) -> Self {
        Self {
            decision,
            executed,
            outcome,
        }
    }

    /// Create a result for an unexecuted decision.
    pub fn not_executed(decision: Decision) -> Self {
        Self {
            decision,
            executed: false,
            outcome: None,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentContext — execution context provided to agents
// ---------------------------------------------------------------------------

/// Execution context provided to agents during the observe phase.
///
/// Gives agents access to the main trading engine and optional
/// feature store for gathering data.
pub struct AgentContext {
    /// Reference to the main trading engine.
    pub engine: std::sync::Arc<crate::trader::MainEngine>,
    /// Optional feature store for ML feature access.
    pub feature_store: Option<std::sync::Arc<crate::feature::FeatureStore>>,
}

// ---------------------------------------------------------------------------
// AgentResult — convenience alias
// ---------------------------------------------------------------------------

/// Result type alias used across the agent module.
pub type AgentResult<T> = Result<T, String>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_display() {
        assert_eq!(format!("{}", AgentType::SentimentAnalyst), "sentiment_analyst");
        assert_eq!(format!("{}", AgentType::RiskAssessor), "risk_assessor");
        assert_eq!(format!("{}", AgentType::RLTrader), "rl_trader");
        assert_eq!(format!("{}", AgentType::ExecutionOptimizer), "execution_optimizer");
        assert_eq!(format!("{}", AgentType::TechnicalAnalyst), "technical_analyst");
    }

    #[test]
    fn test_agent_config_new() {
        let config = AgentConfig::new("risk_1", AgentType::RiskAssessor);
        assert_eq!(config.name, "risk_1");
        assert_eq!(config.agent_type, AgentType::RiskAssessor);
        assert!(config.enabled);
        assert_eq!(config.max_history, 100);
        assert!(config.params.is_empty());
    }

    #[test]
    fn test_agent_config_with_param() {
        let config = AgentConfig::new("sent_1", AgentType::SentimentAnalyst)
            .with_param("threshold", serde_json::json!(0.7));
        assert_eq!(
            config.get_param("threshold"),
            Some(&serde_json::json!(0.7))
        );
        assert!(config.get_param("nonexistent").is_none());
    }

    #[test]
    fn test_observation_new() {
        let obs = Observation::new();
        assert!(obs.is_empty());
        assert_eq!(obs.len(), 0);
    }

    #[test]
    fn test_observation_insert_get() {
        let mut obs = Observation::new();
        obs.insert("close", serde_json::json!(42000.5));
        obs.insert("volume", serde_json::json!(1234.0));

        assert_eq!(obs.len(), 2);
        assert!(!obs.is_empty());
        assert_eq!(obs.get("close"), Some(&serde_json::json!(42000.5)));
        assert_eq!(obs.get("missing"), None);
    }

    #[test]
    fn test_decision_no_action() {
        let d = Decision::NoAction;
        assert!(d.is_no_action());
    }

    #[test]
    fn test_decision_risk_adjustment() {
        let d = Decision::RiskAdjustment {
            analysis: "High drawdown".to_string(),
            suggested_action: "Reduce position".to_string(),
        };
        assert!(!d.is_no_action());
    }

    #[test]
    fn test_decision_sentiment_signal() {
        let d = Decision::SentimentSignal {
            sentiment: "bullish".to_string(),
            confidence: 0.85,
        };
        assert!(!d.is_no_action());
    }

    #[test]
    fn test_decision_trade_signal() {
        let d = Decision::TradeSignal {
            symbol: "BTCUSDT".to_string(),
            direction: Direction::Long,
            strength: 0.9,
        };
        assert!(!d.is_no_action());
    }

    #[test]
    fn test_decision_result_new() {
        let d = Decision::NoAction;
        let result = DecisionResult::new(d, true, Some("ok".to_string()));
        assert!(result.executed);
        assert_eq!(result.outcome.as_deref(), Some("ok"));
    }

    #[test]
    fn test_decision_result_not_executed() {
        let d = Decision::SentimentSignal {
            sentiment: "neutral".to_string(),
            confidence: 0.3,
        };
        let result = DecisionResult::not_executed(d);
        assert!(!result.executed);
        assert!(result.outcome.is_none());
    }

    #[test]
    fn test_agent_type_serde_roundtrip() {
        for at in [
            AgentType::SentimentAnalyst,
            AgentType::TechnicalAnalyst,
            AgentType::RiskAssessor,
            AgentType::RLTrader,
            AgentType::ExecutionOptimizer,
        ] {
            let json = serde_json::to_string(&at).unwrap();
            let back: AgentType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, at);
        }
    }

    #[test]
    fn test_observation_serialization() {
        let mut obs = Observation::new();
        obs.insert("price", serde_json::json!(100.0));
        let json = serde_json::to_string(&obs).unwrap();
        let back: Observation = serde_json::from_str(&json).unwrap();
        assert_eq!(back.len(), 1);
    }

    #[test]
    fn test_decision_serialization() {
        let d = Decision::TradeSignal {
            symbol: "ETHUSDT".to_string(),
            direction: Direction::Short,
            strength: 0.6,
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: Decision = serde_json::from_str(&json).unwrap();
        assert!(!back.is_no_action());
    }
}
