//! Agent trait definition for the Agent Layer module.
//!
//! Defines the core `Agent` trait with the observe/decide/feedback lifecycle
//! pattern. This is inspired by reinforcement learning and autonomous agent
//! architectures.

use async_trait::async_trait;

use super::types::{AgentContext, AgentType, Decision, DecisionResult, Observation};

/// Core Agent trait for autonomous trading agents.
///
/// The agent lifecycle follows a three-phase pattern:
///
/// 1. **Observe**: Gather data from the trading engine, feature store,
///    and external sources.
/// 2. **Decide**: Analyze observations and produce a decision (trade signal,
///    risk adjustment, sentiment signal, or no action).
/// 3. **Feedback**: Receive feedback on whether the decision was executed
///    and the outcome, for learning/adaptation.
///
/// All methods are async to allow for non-blocking operations such as
/// network requests (LLM inference) or heavy computation.
///
/// # Example
///
/// ```rust,ignore
/// use trade_engine::agent::{Agent, AgentContext, Observation, Decision, DecisionResult};
///
/// struct MyAgent {
///     name: String,
/// }
///
/// #[async_trait]
/// impl Agent for MyAgent {
///     fn agent_name(&self) -> &str { &self.name }
///     fn agent_type(&self) -> AgentType { AgentType::RiskAssessor }
///
///     async fn observe(&mut self, ctx: &AgentContext) -> Result<Observation, String> {
///         // Gather data...
///         Ok(Observation::new())
///     }
///
///     async fn decide(&mut self, obs: &Observation) -> Result<Decision, String> {
///         // Analyze and decide...
///         Ok(Decision::NoAction)
///     }
///
///     async fn feedback(&mut self, result: &DecisionResult) -> Result<(), String> {
///         // Learn from feedback...
///         Ok(())
///     }
/// }
/// ```
#[async_trait]
pub trait Agent: Send + Sync {
    /// Returns the unique name of this agent instance.
    fn agent_name(&self) -> &str;

    /// Returns the type/classification of this agent.
    fn agent_type(&self) -> AgentType;

    /// Observe the current state of the trading system.
    ///
    /// This is the data-gathering phase. The agent should collect relevant
    /// information from the `AgentContext` (trading engine, feature store)
    /// and return an `Observation`.
    ///
    /// # Arguments
    /// * `context` - Execution context providing access to the trading engine
    ///   and optional feature store.
    ///
    /// # Returns
    /// An `Observation` containing the data collected by the agent.
    async fn observe(&mut self, context: &AgentContext) -> Result<Observation, String>;

    /// Decide on an action based on the observation.
    ///
    /// This is the analysis and decision phase. The agent processes the
    /// observation and produces a `Decision` (trade signal, risk adjustment,
    /// sentiment signal, or no action).
    ///
    /// # Arguments
    /// * `observation` - The data collected during the observe phase.
    ///
    /// # Returns
    /// A `Decision` representing the agent's output.
    async fn decide(&mut self, observation: &Observation) -> Result<Decision, String>;

    /// Receive feedback on a decision.
    ///
    /// This is the learning phase. The agent receives information about
    /// whether its decision was executed and the outcome. This can be used
    /// for online learning, model updates, or performance tracking.
    ///
    /// # Arguments
    /// * `result` - The result of executing (or not executing) the decision.
    async fn feedback(&mut self, result: &DecisionResult) -> Result<(), String>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::MainEngine;

    /// Minimal agent implementation for testing the trait.
    struct TestAgent {
        name: String,
        observation_count: usize,
        decision_count: usize,
        feedback_count: usize,
    }

    impl TestAgent {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                observation_count: 0,
                decision_count: 0,
                feedback_count: 0,
            }
        }
    }

    #[async_trait]
    impl Agent for TestAgent {
        fn agent_name(&self) -> &str {
            &self.name
        }

        fn agent_type(&self) -> AgentType {
            AgentType::RiskAssessor
        }

        async fn observe(&mut self, _context: &AgentContext) -> Result<Observation, String> {
            self.observation_count += 1;
            let mut obs = Observation::new();
            obs.insert("test_value", serde_json::json!(42));
            Ok(obs)
        }

        async fn decide(&mut self, observation: &Observation) -> Result<Decision, String> {
            self.decision_count += 1;
            if observation.get("test_value").is_some() {
                Ok(Decision::NoAction)
            } else {
                Ok(Decision::RiskAdjustment {
                    analysis: "No data".to_string(),
                    suggested_action: "Wait".to_string(),
                })
            }
        }

        async fn feedback(&mut self, _result: &DecisionResult) -> Result<(), String> {
            self.feedback_count += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_agent_lifecycle() {
        let mut agent = TestAgent::new("test_agent");
        let engine = MainEngine::new();
        let context = AgentContext {
            engine,
            feature_store: None,
        };

        // Observe
        let obs = agent.observe(&context).await.expect("observe failed");
        assert_eq!(agent.observation_count, 1);
        assert_eq!(obs.len(), 1);

        // Decide
        let decision = agent.decide(&obs).await.expect("decide failed");
        assert_eq!(agent.decision_count, 1);
        assert!(decision.is_no_action());

        // Feedback
        let result = DecisionResult::not_executed(decision);
        agent.feedback(&result).await.expect("feedback failed");
        assert_eq!(agent.feedback_count, 1);
    }

    #[test]
    fn test_agent_name_and_type() {
        let agent = TestAgent::new("my_risk_agent");
        assert_eq!(agent.agent_name(), "my_risk_agent");
        assert_eq!(agent.agent_type(), AgentType::RiskAssessor);
    }
}
