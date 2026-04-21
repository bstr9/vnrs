//! RiskAgent — evaluates portfolio risk and suggests adjustments.
//!
//! The `RiskAgent` observes position data, margin info, and risk metrics
//! from the `MainEngine`, then decides whether risk adjustments are needed.
//! It can optionally use an LLM client for advanced risk analysis, or fall
//! back to rule-based heuristics.

use async_trait::async_trait;
use std::collections::HashMap;

use super::mcp_bridge::{LlmClient, McpBridge, SamplingParams};
use super::traits::Agent;
use super::types::{AgentConfig, AgentContext, AgentType, Decision, DecisionResult, Observation};

// ---------------------------------------------------------------------------
// Risk thresholds
// ---------------------------------------------------------------------------

/// Default risk thresholds for the rule-based risk assessment.
#[derive(Debug, Clone)]
pub struct RiskThresholds {
    /// Maximum drawdown ratio before triggering a risk alert (0.0–1.0).
    pub max_drawdown_ratio: f64,
    /// Maximum position concentration (0.0–1.0) — fraction of portfolio
    /// in a single position.
    pub max_concentration: f64,
    /// Maximum number of active orders before suggesting order reduction.
    pub max_active_orders: usize,
    /// Maximum portfolio leverage ratio.
    pub max_leverage: f64,
}

impl Default for RiskThresholds {
    fn default() -> Self {
        Self {
            max_drawdown_ratio: 0.15,
            max_concentration: 0.3,
            max_active_orders: 20,
            max_leverage: 3.0,
        }
    }
}

// ---------------------------------------------------------------------------
// RiskAgent
// ---------------------------------------------------------------------------

/// Agent that evaluates portfolio risk and suggests adjustments.
///
/// # Observe Phase
/// Collects position data, margin info, active orders, and account data
/// from the `MainEngine`.
///
/// # Decide Phase
/// Analyzes risk levels using rule-based heuristics and optionally LLM
/// reasoning. Produces `Decision::RiskAdjustment` when risk thresholds
/// are breached, or `Decision::NoAction` otherwise.
///
/// # Feedback Phase
/// Tracks whether risk adjustments were effective by recording feedback
/// in an internal history.
pub struct RiskAgent {
    /// Agent configuration.
    config: AgentConfig,
    /// Risk thresholds.
    thresholds: RiskThresholds,
    /// Optional MCP bridge for LLM-enhanced analysis.
    bridge: Option<McpBridge>,
    /// History of feedback results for tracking effectiveness.
    feedback_history: Vec<DecisionResult>,
    /// Number of risk adjustments suggested (for metrics).
    adjustment_count: usize,
    /// Number of effective adjustments (for metrics).
    effective_count: usize,
}

impl RiskAgent {
    /// Create a new `RiskAgent` with rule-based risk assessment only.
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            thresholds: RiskThresholds::default(),
            bridge: None,
            feedback_history: Vec::new(),
            adjustment_count: 0,
            effective_count: 0,
        }
    }

    /// Create a `RiskAgent` with custom risk thresholds.
    pub fn with_thresholds(config: AgentConfig, thresholds: RiskThresholds) -> Self {
        Self {
            config,
            thresholds,
            bridge: None,
            feedback_history: Vec::new(),
            adjustment_count: 0,
            effective_count: 0,
        }
    }

    /// Create a `RiskAgent` with LLM-enhanced risk analysis.
    pub fn with_llm(config: AgentConfig, client: Box<dyn LlmClient>) -> Self {
        Self {
            config,
            thresholds: RiskThresholds::default(),
            bridge: Some(McpBridge::new("mcp://risk-agent", client)),
            feedback_history: Vec::new(),
            adjustment_count: 0,
            effective_count: 0,
        }
    }

    /// Create a `RiskAgent` with both thresholds and LLM client.
    pub fn with_thresholds_and_llm(
        config: AgentConfig,
        thresholds: RiskThresholds,
        client: Box<dyn LlmClient>,
    ) -> Self {
        Self {
            config,
            thresholds,
            bridge: Some(McpBridge::new("mcp://risk-agent", client)),
            feedback_history: Vec::new(),
            adjustment_count: 0,
            effective_count: 0,
        }
    }

    /// Get the number of risk adjustments suggested.
    pub fn adjustment_count(&self) -> usize {
        self.adjustment_count
    }

    /// Get the number of effective adjustments.
    pub fn effective_count(&self) -> usize {
        self.effective_count
    }

    /// Get the effectiveness ratio (effective / total adjustments).
    pub fn effectiveness_ratio(&self) -> f64 {
        if self.adjustment_count == 0 {
            0.0
        } else {
            self.effective_count as f64 / self.adjustment_count as f64
        }
    }

    /// Get the feedback history.
    pub fn feedback_history(&self) -> &[DecisionResult] {
        &self.feedback_history
    }

    /// Collect position and account data from the engine.
    fn collect_engine_data(&self, context: &AgentContext) -> HashMap<String, serde_json::Value> {
        let mut data = HashMap::new();

        // Collect positions
        let positions = context.engine.get_all_positions();
        data.insert(
            "position_count".to_string(),
            serde_json::json!(positions.len()),
        );

        // Calculate total position value and check concentration
        let mut total_long_value = 0.0_f64;
        let mut total_short_value = 0.0_f64;
        let mut position_details = Vec::new();

        for pos in &positions {
            let pos_value = pos.frozen + pos.volume * pos.price;
            if pos.direction == crate::trader::Direction::Long {
                total_long_value += pos_value;
            } else {
                total_short_value += pos_value;
            }
            position_details.push(serde_json::json!({
                "symbol": pos.vt_symbol(),
                "direction": format!("{}", pos.direction),
                "volume": pos.volume,
                "frozen": pos.frozen,
                "price": pos.price,
            }));
        }
        data.insert("positions".to_string(), serde_json::json!(position_details));
        data.insert("total_long_value".to_string(), serde_json::json!(total_long_value));
        data.insert("total_short_value".to_string(), serde_json::json!(total_short_value));

        // Concentration check
        let total_value = total_long_value + total_short_value;
        if total_value > 0.0 {
            let max_single = positions
                .iter()
                .map(|p| (p.frozen + p.volume * p.price) / total_value)
                .fold(0.0_f64, f64::max);
            data.insert("max_concentration".to_string(), serde_json::json!(max_single));
        }

        // Collect accounts
        let accounts = context.engine.get_all_accounts();
        if let Some(account) = accounts.first() {
            data.insert("balance".to_string(), serde_json::json!(account.balance));
            data.insert("available".to_string(), serde_json::json!(account.available()));
            data.insert("frozen".to_string(), serde_json::json!(account.frozen));
        }
        data.insert(
            "account_count".to_string(),
            serde_json::json!(accounts.len()),
        );

        // Active orders
        let active_orders = context.engine.get_all_active_orders();
        data.insert(
            "active_order_count".to_string(),
            serde_json::json!(active_orders.len()),
        );

        // Leverage calculation
        if total_value > 0.0 && !accounts.is_empty() {
            let balance = accounts[0].balance;
            if balance > 0.0 {
                let leverage = total_value / balance;
                data.insert("leverage".to_string(), serde_json::json!(leverage));
            }
        }

        data
    }

    /// Rule-based risk analysis.
    fn rule_based_analysis(&self, observation: &Observation) -> Option<Decision> {
        // Check active orders count
        if let Some(active_count) = observation
            .get("active_order_count")
            .and_then(|v| v.as_u64())
        {
            if active_count as usize > self.thresholds.max_active_orders {
                return Some(Decision::RiskAdjustment {
                    analysis: format!(
                        "Too many active orders: {} (max: {})",
                        active_count, self.thresholds.max_active_orders
                    ),
                    suggested_action: "Cancel low-priority orders to reduce exposure".to_string(),
                });
            }
        }

        // Check concentration
        if let Some(concentration) = observation
            .get("max_concentration")
            .and_then(|v| v.as_f64())
        {
            if concentration > self.thresholds.max_concentration {
                return Some(Decision::RiskAdjustment {
                    analysis: format!(
                        "Position concentration too high: {:.1}% (max: {:.1}%)",
                        concentration * 100.0,
                        self.thresholds.max_concentration * 100.0
                    ),
                    suggested_action: "Reduce largest position to improve diversification"
                        .to_string(),
                });
            }
        }

        // Check leverage
        if let Some(leverage) = observation.get("leverage").and_then(|v| v.as_f64()) {
            if leverage > self.thresholds.max_leverage {
                return Some(Decision::RiskAdjustment {
                    analysis: format!(
                        "Portfolio leverage too high: {:.2}x (max: {:.2}x)",
                        leverage, self.thresholds.max_leverage
                    ),
                    suggested_action: "Reduce position sizes to lower leverage".to_string(),
                });
            }
        }

        None
    }

    /// LLM-enhanced risk analysis.
    async fn llm_analysis(&self, observation: &Observation) -> Option<Decision> {
        let bridge = self.bridge.as_ref()?;

        let prompt = format!(
            "Analyze the following trading portfolio risk metrics and provide a risk assessment.\n\
             Observation data: {}\n\
             Risk thresholds: max_drawdown={}, max_concentration={}, max_leverage={}\n\
             If risk is acceptable, respond with 'NO_ACTION'. Otherwise, describe the risk and suggest an action.",
            serde_json::to_string(&observation.data).unwrap_or_else(|_| "error".to_string()),
            self.thresholds.max_drawdown_ratio,
            self.thresholds.max_concentration,
            self.thresholds.max_leverage,
        );

        let params = SamplingParams::new(512)
            .with_system_prompt("You are a risk management AI for a crypto trading system.")
            .with_temperature(0.3);

        match bridge.request_reasoning(&prompt, Some(params)).await {
            Ok(response) => {
                let trimmed = response.trim();
                if trimmed.eq_ignore_ascii_case("NO_ACTION") || trimmed.is_empty() {
                    None
                } else {
                    Some(Decision::RiskAdjustment {
                        analysis: trimmed.to_string(),
                        suggested_action: "Follow LLM risk recommendation".to_string(),
                    })
                }
            }
            Err(e) => {
                tracing::warn!("RiskAgent LLM analysis failed: {}", e);
                None
            }
        }
    }
}

#[async_trait]
impl Agent for RiskAgent {
    fn agent_name(&self) -> &str {
        &self.config.name
    }

    fn agent_type(&self) -> AgentType {
        AgentType::RiskAssessor
    }

    async fn observe(&mut self, context: &AgentContext) -> Result<Observation, String> {
        let mut observation = Observation::new();
        let data = self.collect_engine_data(context);

        for (key, value) in data {
            observation.insert(key, value);
        }

        // Also collect from feature store if available
        if let Some(ref feature_store) = context.feature_store {
            let entities = feature_store.online_entities();
            observation.insert(
                "feature_store_entities".to_string(),
                serde_json::json!(entities.len()),
            );

            // Collect volatility features for risk assessment
            let mut volatilities = Vec::new();
            for entity in &entities {
                if let Some(vector) = feature_store.get_online(entity) {
                    use crate::feature::FeatureId;
                    let vol_id = FeatureId::new(format!("{}_volatility", entity));
                    if let Some(vol) = vector.get_f64(&vol_id) {
                        volatilities.push(serde_json::json!({
                            "entity": entity,
                            "volatility": vol,
                        }));
                    }
                }
            }
            if !volatilities.is_empty() {
                observation.insert("volatilities".to_string(), serde_json::json!(volatilities));
            }
        }

        Ok(observation)
    }

    async fn decide(&mut self, observation: &Observation) -> Result<Decision, String> {
        // First try rule-based analysis
        if let Some(decision) = self.rule_based_analysis(observation) {
            self.adjustment_count += 1;
            return Ok(decision);
        }

        // Then try LLM-enhanced analysis (if available)
        if let Some(decision) = self.llm_analysis(observation).await {
            self.adjustment_count += 1;
            return Ok(decision);
        }

        Ok(Decision::NoAction)
    }

    async fn feedback(&mut self, result: &DecisionResult) -> Result<(), String> {
        // Track effectiveness
        if matches!(result.decision, Decision::RiskAdjustment { .. }) {
            if result.executed {
                if result.outcome.as_deref() == Some("effective")
                    || result.outcome.as_deref() == Some("resolved")
                {
                    self.effective_count += 1;
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

    fn make_risk_config() -> AgentConfig {
        AgentConfig::new("risk_test", AgentType::RiskAssessor)
    }

    #[tokio::test]
    async fn test_risk_agent_observe_empty_engine() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);
        let engine = MainEngine::new();
        let context = AgentContext {
            engine,
            feature_store: None,
        };

        let obs = agent.observe(&context).await.expect("observe failed");
        assert_eq!(obs.get("position_count"), Some(&serde_json::json!(0)));
        assert_eq!(obs.get("active_order_count"), Some(&serde_json::json!(0)));
    }

    #[tokio::test]
    async fn test_risk_agent_decide_no_action() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);
        let mut obs = Observation::new();
        obs.insert("active_order_count", serde_json::json!(5));
        obs.insert("max_concentration", serde_json::json!(0.1));
        obs.insert("leverage", serde_json::json!(1.5));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(decision.is_no_action());
        assert_eq!(agent.adjustment_count(), 0);
    }

    #[tokio::test]
    async fn test_risk_agent_decide_too_many_orders() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);
        let mut obs = Observation::new();
        obs.insert("active_order_count", serde_json::json!(25));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());
        assert_eq!(agent.adjustment_count(), 1);

        if let Decision::RiskAdjustment { analysis, .. } = decision {
            assert!(analysis.contains("Too many active orders"));
        } else {
            panic!("Expected RiskAdjustment");
        }
    }

    #[tokio::test]
    async fn test_risk_agent_decide_high_concentration() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);
        let mut obs = Observation::new();
        obs.insert("max_concentration", serde_json::json!(0.5));
        obs.insert("active_order_count", serde_json::json!(5));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());

        if let Decision::RiskAdjustment { analysis, .. } = decision {
            assert!(analysis.contains("concentration too high"));
        } else {
            panic!("Expected RiskAdjustment");
        }
    }

    #[tokio::test]
    async fn test_risk_agent_decide_high_leverage() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);
        let mut obs = Observation::new();
        obs.insert("leverage", serde_json::json!(5.0));
        obs.insert("active_order_count", serde_json::json!(5));
        obs.insert("max_concentration", serde_json::json!(0.1));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());

        if let Decision::RiskAdjustment { analysis, .. } = decision {
            assert!(analysis.contains("leverage too high"));
        } else {
            panic!("Expected RiskAdjustment");
        }
    }

    #[tokio::test]
    async fn test_risk_agent_feedback_tracking() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);

        let decision = Decision::RiskAdjustment {
            analysis: "test".to_string(),
            suggested_action: "reduce".to_string(),
        };

        // Feedback with effective result
        let result = DecisionResult::new(decision.clone(), true, Some("effective".to_string()));
        agent.feedback(&result).await.expect("feedback failed");
        assert_eq!(agent.effective_count(), 1);
        assert_eq!(agent.feedback_history().len(), 1);

        // Feedback with not-executed result
        let result2 = DecisionResult::not_executed(decision);
        agent.feedback(&result2).await.expect("feedback failed");
        assert_eq!(agent.feedback_history().len(), 2);
    }

    #[tokio::test]
    async fn test_risk_agent_effectiveness_ratio() {
        let config = make_risk_config();
        let mut agent = RiskAgent::new(config);

        // No adjustments yet
        assert!((agent.effectiveness_ratio() - 0.0).abs() < f64::EPSILON);

        // Manually set counters for testing
        agent.adjustment_count = 4;
        agent.effective_count = 3;
        assert!((agent.effectiveness_ratio() - 0.75).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_risk_agent_with_custom_thresholds() {
        let config = make_risk_config();
        let thresholds = RiskThresholds {
            max_active_orders: 5,
            ..Default::default()
        };
        let mut agent = RiskAgent::with_thresholds(config, thresholds);

        // 10 orders exceeds custom threshold of 5
        let mut obs = Observation::new();
        obs.insert("active_order_count", serde_json::json!(10));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());
    }

    #[tokio::test]
    async fn test_risk_agent_with_noop_llm() {
        let config = make_risk_config();
        let client = NoOpLlmClient::with_response("NO_ACTION");
        let mut agent = RiskAgent::with_llm(config, Box::new(client));

        // Rule-based should not trigger
        let mut obs = Observation::new();
        obs.insert("active_order_count", serde_json::json!(5));
        obs.insert("max_concentration", serde_json::json!(0.1));
        obs.insert("leverage", serde_json::json!(1.5));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(decision.is_no_action());
    }

    #[tokio::test]
    async fn test_risk_agent_with_llm_risk_detected() {
        let config = make_risk_config();
        let client = NoOpLlmClient::with_response(
            "Portfolio shows elevated correlation risk. Consider reducing BTC exposure.",
        );
        let mut agent = RiskAgent::with_llm(config, Box::new(client));

        // Rule-based should not trigger, but LLM should detect risk
        let mut obs = Observation::new();
        obs.insert("active_order_count", serde_json::json!(5));
        obs.insert("max_concentration", serde_json::json!(0.1));
        obs.insert("leverage", serde_json::json!(1.5));

        let decision = agent.decide(&obs).await.expect("decide failed");
        assert!(!decision.is_no_action());

        if let Decision::RiskAdjustment { analysis, .. } = decision {
            assert!(analysis.contains("correlation risk"));
        } else {
            panic!("Expected RiskAdjustment");
        }
    }

    #[test]
    fn test_risk_thresholds_default() {
        let t = RiskThresholds::default();
        assert!((t.max_drawdown_ratio - 0.15).abs() < f64::EPSILON);
        assert!((t.max_concentration - 0.3).abs() < f64::EPSILON);
        assert_eq!(t.max_active_orders, 20);
        assert!((t.max_leverage - 3.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_risk_agent_feedback_history_bounded() {
        let mut config = AgentConfig::new("risk_bounded", AgentType::RiskAssessor);
        config.max_history = 3;
        let mut agent = RiskAgent::new(config);

        // Add 5 feedback entries
        for i in 0..5 {
            let decision = Decision::NoAction;
            let result = DecisionResult::new(decision, false, Some(format!("entry_{}", i)));
            agent.feedback(&result).await.expect("feedback failed");
        }

        // Only last 3 should be kept
        assert_eq!(agent.feedback_history().len(), 3);
    }
}
