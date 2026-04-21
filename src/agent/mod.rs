//! Agent Layer Module — autonomous agents for trading intelligence.
//!
//! This module provides a framework for building autonomous agents that
//! participate in the trading system's decision-making process. Each agent
//! follows an **observe → decide → feedback** lifecycle:
//!
//! 1. **Observe**: Gather data from the trading engine, feature store,
//!    and external sources.
//! 2. **Decide**: Analyze observations and produce a decision (trade signal,
//!    risk adjustment, sentiment signal, or no action).
//! 3. **Feedback**: Receive feedback on whether the decision was executed
//!    and the outcome, enabling learning and adaptation.
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────────────────┐
//! │                      Agent Layer                              │
//! ├───────────────────────────────────────────────────────────────┤
//! │                                                               │
//! │  ┌─────────────┐  ┌──────────────────┐  ┌─────────────────┐ │
//! │  │  Agent Trait │  │   McpBridge       │  │   Agent Types   │ │
//! │  │ (lifecycle)  │  │ (LLM interface)   │  │ (shared types)  │ │
//! │  └──────┬───────┘  └────────┬─────────┘  └────────┬────────┘ │
//! │         │                   │                      │          │
//! │  ┌──────┴───────┐  ┌───────┴──────────┐          │          │
//! │  │  RiskAgent    │  │  SentimentAgent   │          │          │
//! │  │ (risk mgmt)  │  │ (market sentiment)│          │          │
//! │  └──────────────┘  └──────────────────┘          │          │
//! │                                                   │          │
//! └───────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Feature Flag
//!
//! This module requires the `agent` feature flag (which implies `feature-store`):
//!
//! ```toml
//! [dependencies]
//! trade_engine = { features = ["agent"] }
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use trade_engine::agent::{Agent, RiskAgent, SentimentAgent, AgentConfig, AgentType};
//! use trade_engine::trader::MainEngine;
//!
//! let engine = MainEngine::new();
//!
//! // Create a risk agent
//! let config = AgentConfig::new("risk_1", AgentType::RiskAssessor);
//! let mut risk_agent = RiskAgent::new(config);
//!
//! // Run the agent lifecycle
//! let context = AgentContext { engine, feature_store: None };
//! let obs = risk_agent.observe(&context).await?;
//! let decision = risk_agent.decide(&obs).await?;
//! ```

mod mcp_bridge;
mod risk_agent;
mod sentiment_agent;
mod traits;
mod types;

// Re-export main types for convenience
pub use mcp_bridge::{
    LlmClient, McpBridge, McpBridgeConfig, NoOpLlmClient, SamplingParams,
};
pub use risk_agent::{RiskAgent, RiskThresholds};
pub use sentiment_agent::{SentimentAgent, SentimentThresholds};
pub use traits::Agent;
pub use types::{
    AgentConfig, AgentContext, AgentResult, AgentType, Decision, DecisionResult, Observation,
};
