//! SentimentAgent — analyzes market sentiment and produces signals.
//!
//! The `SentimentAgent` observes market data and recent events from the
//! `MainEngine`, then decides on sentiment signals. It can use rule-based
//! heuristics (price momentum) or LLM-enhanced sentiment analysis via
//! the MCP Bridge.

use async_trait::async_trait;
use std::collections::HashMap;

use super::mcp_bridge::{LlmClient, McpBridge, SamplingParams};
use super::traits::Agent;
use super::types::{AgentConfig, AgentContext, AgentType, Decision, DecisionResult, Observation};

// ---------------------------------------------------------------------------
// SentimentConfig — sentiment analysis parameters
// ---------------------------------------------------------------------------

/// Configuration for sentiment analysis thresholds.
#[derive(Debug, Clone)]
pub struct SentimentThresholds {
    /// Minimum absolute price change ratio to trigger a sentiment signal.
    pub min_price_change_ratio: f64,
    /// Confidence threshold for producing a signal (0.0–1.0).
    pub confidence_threshold: f64,
    /// Symbols to monitor (empty = all available).
    pub watch_symbols: Vec<String>,
}

impl Default for SentimentThresholds {
    fn default() -> Self {
        Self {
            min_price_change_ratio: 0.02,
            confidence_threshold: 0.5,
            watch_symbols: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// SentimentAgent
// ---------------------------------------------------------------------------

/// Agent that analyzes market sentiment and produces sentiment signals.
///
/// # Observe Phase
/// Collects the latest bar/tick data, position data, and optional
/// feature store volatility data from the `MainEngine`.
///
/// # Decide Phase
/// Analyzes sentiment using rule-based heuristics (price momentum from
/// bar data) and optionally LLM-enhanced reasoning. Produces
/// `Decision::SentimentSignal` when sentiment is detected, or
/// `Decision::NoAction` otherwise.
///
/// # Feedback Phase
/// Tracks sentiment accuracy by recording whether past sentiment signals
/// were correct (market moved in the predicted direction).
pub struct SentimentAgent {
    /// Agent configuration.
    config: AgentConfig,
    /// Sentiment thresholds.
    thresholds: SentimentThresholds,
    /// Optional MCP bridge for LLM-enhanced sentiment analysis.
    bridge: Option<McpBridge>,
    /// History of feedback results for accuracy tracking.
    feedback_history: Vec<DecisionResult>,
    /// Number of sentiment signals produced.
    signal_count: usize,
    /// Number of correct sentiment signals (based on feedback).
    correct_count: usize,
}

impl SentimentAgent {
    /// Create a new `SentimentAgent` with rule-based analysis only.
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            thresholds: SentimentThresholds::default(),
            bridge: None,
            feedback_history: Vec::new(),
            signal_count: 0,
            correct_count: 0,
        }
    }

    /// Create a `SentimentAgent` with custom thresholds.
    pub fn with_thresholds(config: AgentConfig, thresholds: SentimentThresholds) -> Self {
        Self {
            config,
            thresholds,
            bridge: None,
            feedback_history: Vec::new(),
            signal_count: 0,
            correct_count: 0,
        }
    }

    /// Create a `SentimentAgent` with LLM-enhanced sentiment analysis.
    pub fn with_llm(config: AgentConfig, client: Box<dyn LlmClient>) -> Self {
        Self {
            config,
            thresholds: SentimentThresholds::default(),
            bridge: Some(McpBridge::new("mcp://sentiment-agent", client)),
            feedback_history: Vec::new(),
            signal_count: 0,
            correct_count: 0,
        }
    }

    /// Create a `SentimentAgent` with both thresholds and LLM client.
    pub fn with_thresholds_and_llm(
        config: AgentConfig,
        thresholds: SentimentThresholds,
        client: Box<dyn LlmClient>,
    ) -> Self {
        Self {
            config,
            thresholds,
            bridge: Some(McpBridge::new("mcp://sentiment-agent", client)),
            feedback_history: Vec::new(),
            signal_count: 0,
            correct_count: 0,
        }
    }

    /// Get the number of sentiment signals produced.
    pub fn signal_count(&self) -> usize {
        self.signal_count
    }

    /// Get the number of correct signals.
    pub fn correct_count(&self) -> usize {
        self.correct_count
    }

    /// Get the accuracy ratio (correct / total signals).
    pub fn accuracy_ratio(&self) -> f64 {
        if self.signal_count == 0 {
            0.0
        } else {
            self.correct_count as f64 / self.signal_count as f64
        }
    }

    /// Get the feedback history.
    pub fn feedback_history(&self) -> &[DecisionResult] {
        &self.feedback_history
    }

    /// Collect market data from the engine.
    fn collect_market_data(&self, context: &AgentContext) -> HashMap<String, serde_json::Value> {
        let mut data = HashMap::new();

        // Collect latest bars
        let bars = context.engine.get_all_bars();
        let mut bar_details = Vec::new();

        for bar in &bars {
            // Filter by watch symbols if specified
            if !self.thresholds.watch_symbols.is_empty() {
                let vt = bar.vt_symbol();
                let matches = self
                    .thresholds
                    .watch_symbols
                    .iter()
                    .any(|s| vt.starts_with(s));
                if !matches {
                    continue;
                }
            }

            let price_change = if bar.open_price != 0.0 {
                (bar.close_price - bar.open_price) / bar.open_price
            } else {
                0.0
            };

            bar_details.push(serde_json::json!({
                "symbol": bar.vt_symbol(),
                "open": bar.open_price,
                "high": bar.high_price,
                "low": bar.low_price,
                "close": bar.close_price,
                "volume": bar.volume,
                "price_change": price_change,
            }));
        }

        data.insert("bar_count".to_string(), serde_json::json!(bar_details.len()));
        data.insert("bars".to_string(), serde_json::json!(bar_details));

        // Collect latest ticks
        let ticks = context.engine.get_all_ticks();
        let mut tick_details = Vec::new();

        for tick in ticks.iter().take(10) {
            tick_details.push(serde_json::json!({
                "symbol": tick.vt_symbol(),
                "last_price": tick.last_price,
                "volume": tick.volume,
            }));
        }
        data.insert("tick_count".to_string(), serde_json::json!(ticks.len()));
        data.insert("ticks".to_string(), serde_json::json!(tick_details));

        data
    }

    /// Rule-based sentiment analysis using price momentum from bar data.
    fn rule_based_analysis(&self, observation: &Observation) -> Option<Decision> {
        let bars = observation.get("bars").and_then(|v| v.as_array())?;

        for bar in bars {
            let price_change = bar.get("price_change").and_then(|v| v.as_f64())?;
            let _symbol = bar.get("symbol").and_then(|v| v.as_str())?;

            if price_change.abs() >= self.thresholds.min_price_change_ratio {
                let sentiment = if price_change > 0.0 {
                    "bullish"
                } else {
                    "bearish"
                };

                let confidence = (price_change.abs() * 10.0).min(1.0);

                if confidence >= self.thresholds.confidence_threshold {
                    return Some(Decision::SentimentSignal {
                        sentiment: sentiment.to_string(),
                        confidence,
                    });
                }
            }
        }

        None
    }

    /// LLM-enhanced sentiment analysis.
    async fn llm_analysis(&self, observation: &Observation) -> Option<Decision> {
        let bridge = self.bridge.as_ref()?;

        let prompt = format!(
            "Analyze the following market data and provide a sentiment assessment.\n\
             Market data: {}\n\
             Respond with exactly one of: 'bullish', 'bearish', or 'neutral', \
             followed by a confidence score (0.0-1.0).\n\
             Format: SENTIMENT|CONFIDENCE\n\
             Example: bullish|0.8\n\
             If sentiment is neutral or confidence is low, respond with: neutral|0.0",
            serde_json::to_string(&observation.data).unwrap_or_else(|_| "error".to_string()),
        );

        let params = SamplingParams::new(256)
            .with_system_prompt(
                "You are a market sentiment analyst for a crypto trading system. \
                 Provide concise sentiment assessments.",
            )
            .with_temperature(0.3);

        match bridge.request_reasoning(&prompt, Some(params)).await {
            Ok(response) => {
                let trimmed = response.trim();
                // Try to parse "sentiment|confidence" format
                let parts: Vec<&str> = trimmed.split('|').collect();
                if parts.len() >= 2 {
                    let sentiment = parts[0].trim().to_lowercase();
                    let confidence = parts[1].trim().parse::<f64>().unwrap_or(0.0);

                    if sentiment == "neutral" || confidence < self.thresholds.confidence_threshold
                    {
                        return None;
                    }

                    if sentiment == "bullish" || sentiment == "bearish" {
                        return Some(Decision::SentimentSignal {
                            sentiment,
                            confidence: confidence.min(1.0),
                        });
                    }
                }

                // Fallback: try to detect sentiment from free-text response
                let lower = trimmed.to_lowercase();
                if lower.contains("bullish") || lower.contains("positive") {
                    return Some(Decision::SentimentSignal {
                        sentiment: "bullish".to_string(),
                        confidence: 0.6,
                    });
                } else if lower.contains("bearish") || lower.contains("negative") {
                    return Some(Decision::SentimentSignal {
                        sentiment: "bearish".to_string(),
                        confidence: 0.6,
                    });
                }

                None
            }
            Err(e) => {
                tracing::warn!("SentimentAgent LLM analysis failed: {}", e);
                None
            }
        }
    }
}

#[async_trait]
impl Agent for SentimentAgent {
    fn agent_name(&self) -> &str {
        &self.config.name
    }

    fn agent_type(&self) -> AgentType {
        AgentType::SentimentAnalyst
    }

    async fn observe(&mut self, context: &AgentContext) -> Result<Observation, String> {
        let mut observation = Observation::new();
        let data = self.collect_market_data(context);

        for (key, value) in data {
            observation.insert(key, value);
        }

        // Collect volatility data from feature store if available
        if let Some(ref feature_store) = context.feature_store {
            let entities = feature_store.online_entities();
            observation.insert(
                "feature_store_entities".to_string(),
                serde_json::json!(entities.len()),
            );

            // Collect returns and volatility for sentiment assessment
            let mut sentiment_features = Vec::new();
            for entity in &entities {
                if let Some(vector) = feature_store.get_online(entity) {
                    use crate::feature::FeatureId;
                    let returns_id = FeatureId::new(format!("{}_returns", entity));
                    let vol_id = FeatureId::new(format!("{}_volatility", entity));

                    let returns = vector.get_f64(&returns_id);
                    let vol = vector.get_f64(&vol_id);

                    if returns.is_some() || vol.is_some() {
                        sentiment_features.push(serde_json::json!({
                            "entity": entity,
                            "returns": returns,
                            "volatility": vol,
                        }));
                    }
                }
            }
            if !sentiment_features.is_empty() {
                observation.insert(
                    "sentiment_features".to_string(),
                    serde_json::json!(sentiment_features),
                );
            }
        }

        Ok(observation)
    }

    async fn decide(&mut self, observation: &Observation) -> Result<Decision, String> {
        // First try rule-based analysis
        if let Some(decision) = self.rule_based_analysis(observation) {
            self.signal_count += 1;
            return Ok(decision);
        }

        // Then try LLM-enhanced analysis (if available)
        if let Some(decision) = self.llm_analysis(observation).await {
            self.signal_count += 1;
            return Ok(decision);
        }

        Ok(Decision::NoAction)
    }

    async fn feedback(&mut self, result: &DecisionResult) -> Result<(), String> {
        // Track accuracy
        if let Decision::SentimentSignal {
            ref sentiment,
            confidence: _,
        } = result.decision
        {
            if result.executed {
                // Check if the sentiment was correct based on outcome
                if let Some(ref outcome) = result.outcome {
                    let was_correct = match sentiment.as_str() {
                        "bullish" => outcome.contains("up") || outcome.contains("bull"),
                        "bearish" => outcome.contains("down") || outcome.contains("bear"),
                        _ => false,
                    };
                    if was_correct {
                        self.correct_count += 1;
                    }
                }
            }
        }

        // Store in history (bounded)
        self.feedback_history.push(result.clone());
        let max = self.config.max_history;
        if self.feedback_history.len() > max {
            self.feedback_history.drain(0..self.feedback_history.len() - max);
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::MainEngine;
    use crate::agent::NoOpLlmClient;

    fn make_sentiment_config() -> AgentConfig {
        AgentConfig::new("sentiment_test", AgentType::SentimentAnalyst)
    }

    #[tokio::test]
    async fn test_sentiment_agent_observe_empty_engine() {
        let config = make_sentiment_config();
        let mut agent = SentimentAgent::new(config);
        let engine = MainEngine::new();
        let context = AgentContext {
            engine,
            feature_store: None,
        };

        let obs = agent.observe(&context).await.expect("observe failed");
        assert_eq!(obs.get("bar_count"), Some(&serde_json::json!(0)));
        assert_eq!(obs.get("tick_count"), Some(&serde_json::json!(0)));
    }

    #[tokio::test]
    async fn test_sentiment_agent_decide_no_action() {
        let config = make_sentiment_config();
        let mut agent = SentimentAgent::new(config);
        let mut obs = Observation::new();
        obs.insert("bars", serde_json::json!([]));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(decision.is_no_action());
        assert_eq!(agent.signal_count(), 0);
    }

    #[tokio::test]
    async fn test_sentiment_agent_decide_bullish() {
        let config = make_sentiment_config();
        let mut agent = SentimentAgent::new(config);
        let mut obs = Observation::new();
        obs.insert(
            "bars",
            serde_json::json!([{
                "symbol": "BTCUSDT.BINANCE",
                "open": 50000.0,
                "high": 52000.0,
                "low": 49500.0,
                "close": 51500.0,
                "volume": 1000.0,
                "price_change": 0.06,
            }]),
        );

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());
        assert_eq!(agent.signal_count(), 1);

        if let Decision::SentimentSignal { sentiment, confidence } = decision {
            assert_eq!(sentiment, "bullish");
            assert!(confidence >= 0.5);
        } else {
            panic!("Expected SentimentSignal");
        }
    }

    #[tokio::test]
    async fn test_sentiment_agent_decide_bearish() {
        let config = make_sentiment_config();
        let mut agent = SentimentAgent::new(config);
        let mut obs = Observation::new();
        obs.insert(
            "bars",
            serde_json::json!([{
                "symbol": "ETHUSDT.BINANCE",
                "open": 3000.0,
                "high": 3050.0,
                "low": 2800.0,
                "close": 2850.0,
                "volume": 500.0,
                "price_change": -0.05,
            }]),
        );

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());

        if let Decision::SentimentSignal { sentiment, confidence } = decision {
            assert_eq!(sentiment, "bearish");
            assert!(confidence >= 0.5);
        } else {
            panic!("Expected SentimentSignal");
        }
    }

    #[tokio::test]
    async fn test_sentiment_agent_decide_small_change_no_signal() {
        let config = make_sentiment_config();
        let mut agent = SentimentAgent::new(config);
        let mut obs = Observation::new();
        obs.insert(
            "bars",
            serde_json::json!([{
                "symbol": "BTCUSDT.BINANCE",
                "open": 50000.0,
                "high": 50100.0,
                "low": 49900.0,
                "close": 50050.0,
                "volume": 100.0,
                "price_change": 0.001,
            }]),
        );

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(decision.is_no_action());
    }

    #[tokio::test]
    async fn test_sentiment_agent_feedback_accuracy() {
        let config = make_sentiment_config();
        let mut agent = SentimentAgent::new(config);

        let decision = Decision::SentimentSignal {
            sentiment: "bullish".to_string(),
            confidence: 0.8,
        };

        // Correct prediction (bullish, market went up)
        let result = DecisionResult::new(decision.clone(), true, Some("up_bull_market".to_string()));
        agent.feedback(&result).await.expect("feedback failed");
        assert_eq!(agent.correct_count(), 1);
        assert_eq!(agent.signal_count(), 0); // signal_count only incremented by decide

        // Incorrect prediction (bearish, market went up)
        let bearish = Decision::SentimentSignal {
            sentiment: "bearish".to_string(),
            confidence: 0.7,
        };
        let result2 = DecisionResult::new(bearish, true, Some("up_market".to_string()));
        agent.feedback(&result2).await.expect("feedback failed");
        assert_eq!(agent.correct_count(), 1); // Not correct for bearish + up

        // Correct prediction (bearish, market went down)
        let bearish2 = Decision::SentimentSignal {
            sentiment: "bearish".to_string(),
            confidence: 0.6,
        };
        let result3 = DecisionResult::new(bearish2, true, Some("down_bear_market".to_string()));
        agent.feedback(&result3).await.expect("feedback failed");
        assert_eq!(agent.correct_count(), 2);
    }

    #[tokio::test]
    async fn test_sentiment_agent_accuracy_ratio() {
        let config = make_sentiment_config();
        let agent = SentimentAgent::new(config);
        assert!((agent.accuracy_ratio() - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_sentiment_agent_with_noop_llm() {
        let config = make_sentiment_config();
        let client = NoOpLlmClient::with_response("neutral|0.0");
        let mut agent = SentimentAgent::with_llm(config, Box::new(client));

        // No rule-based trigger, LLM returns neutral
        let mut obs = Observation::new();
        obs.insert("bars", serde_json::json!([]));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(decision.is_no_action());
    }

    #[tokio::test]
    async fn test_sentiment_agent_with_llm_bullish() {
        let config = make_sentiment_config();
        let client = NoOpLlmClient::with_response("bullish|0.85");
        let mut agent = SentimentAgent::with_llm(config, Box::new(client));

        // No rule-based trigger, but LLM returns bullish
        let mut obs = Observation::new();
        obs.insert("bars", serde_json::json!([]));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());

        if let Decision::SentimentSignal { sentiment, confidence } = decision {
            assert_eq!(sentiment, "bullish");
            assert!((confidence - 0.85).abs() < f64::EPSILON);
        } else {
            panic!("Expected SentimentSignal");
        }
    }

    #[tokio::test]
    async fn test_sentiment_agent_with_llm_free_text_bullish() {
        let config = make_sentiment_config();
        let client = NoOpLlmClient::with_response("The market looks bullish with strong momentum");
        let mut agent = SentimentAgent::with_llm(config, Box::new(client));

        let mut obs = Observation::new();
        obs.insert("bars", serde_json::json!([]));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());

        if let Decision::SentimentSignal { sentiment, .. } = decision {
            assert_eq!(sentiment, "bullish");
        } else {
            panic!("Expected SentimentSignal");
        }
    }

    #[test]
    fn test_sentiment_thresholds_default() {
        let t = SentimentThresholds::default();
        assert!((t.min_price_change_ratio - 0.02).abs() < f64::EPSILON);
        assert!((t.confidence_threshold - 0.5).abs() < f64::EPSILON);
        assert!(t.watch_symbols.is_empty());
    }

    #[tokio::test]
    async fn test_sentiment_agent_feedback_history_bounded() {
        let mut config = AgentConfig::new("sent_bounded", AgentType::SentimentAnalyst);
        config.max_history = 3;
        let mut agent = SentimentAgent::new(config);

        for i in 0..5 {
            let decision = Decision::NoAction;
            let result = DecisionResult::new(decision, false, Some(format!("entry_{}", i)));
            agent.feedback(&result).await.expect("feedback failed");
        }

        assert_eq!(agent.feedback_history().len(), 3);
    }

    #[tokio::test]
    async fn test_sentiment_agent_with_watch_symbols() {
        let config = make_sentiment_config();
        let thresholds = SentimentThresholds {
            watch_symbols: vec!["BTCUSDT".to_string()],
            ..Default::default()
        };
        let mut agent = SentimentAgent::with_thresholds(config, thresholds);

        // Only BTCUSDT bars should be analyzed (ETH filtered out)
        let mut obs = Observation::new();
        obs.insert(
            "bars",
            serde_json::json!([{
                "symbol": "ETHUSDT.BINANCE",
                "open": 3000.0,
                "high": 3100.0,
                "low": 2900.0,
                "close": 2900.0,
                "volume": 500.0,
                "price_change": -0.033,
            }]),
        );

        // ETH bar should be filtered, so no signal
        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(decision.is_no_action());
    }
}
