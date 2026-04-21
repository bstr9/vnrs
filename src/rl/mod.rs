//! Reinforcement Learning Environment Module
//!
//! Provides a Gym-compatible trading environment for training RL agents
//! on the vnrs backtesting engine. The module is feature-gated behind
//! the `rl` feature flag and adds no external dependencies.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  TradingEnv                      │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │
//! │  │   reset   │  │   step   │  │   is_done    │  │
//! │  └─────┬─────┘  └────┬─────┘  └──────┬───────┘  │
//! │        │             │               │           │
//! │  ┌─────▼─────────────▼───────────────▼───────┐   │
//! │  │              ActionMapper                  │   │
//! │  │   DiscreteActionMapper / ContinuousAction  │   │
//! │  └─────────────────┬─────────────────────────┘   │
//! │                    │ OrderRequests                │
//! │  ┌─────────────────▼─────────────────────────┐   │
//! │  │            BacktestingEngine               │   │
//! │  │    (bar-by-bar simulation with fills)      │   │
//! │  └─────────────────┬─────────────────────────┘   │
//! │                    │                              │
//! │  ┌─────────────────▼─────────────────────────┐   │
//! │  │           RewardFunction                   │   │
//! │  │   PnlReward / SharpeReward / RiskAdj...    │   │
//! │  └─────────────────┬─────────────────────────┘   │
//! │                    │                              │
//! │  ┌─────────────────▼─────────────────────────┐   │
//! │  │            Observation                     │   │
//! │  │   features + PortfolioSnapshot + Market    │   │
//! │  └───────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! # Quick Start
//!
//! ```rust,ignore
//! use trade_engine::rl::{
//!     TradingEnv, TradingEnvConfig,
//!     DiscreteActionMapper, ContinuousActionMapper, ActionValue,
//!     PnlReward, SharpeReward, RiskAdjustedReward,
//! };
//!
//! // Create environment with builder pattern
//! let mut env = TradingEnv::builder()
//!     .initial_capital(100_000.0)
//!     .action_mapper(Box::new(DiscreteActionMapper::new(
//!         "BTCUSDT".to_string(),
//!         Exchange::Binance,
//!         1.0,
//!     )))
//!     .reward_fn(Box::new(PnlReward::new()))
//!     .bars(historical_bars)
//!     .build()
//!     .expect("should build");
//!
//! // RL loop
//! let obs = env.reset();
//! loop {
//!     let action = agent.select_action(&obs);
//!     let result = env.step(&action).expect("step");
//!     if result.terminated || result.truncated {
//!         break;
//!     }
//! }
//! ```

pub mod action;
pub mod env;
pub mod info;
pub mod observation;
pub mod reward;

// Re-export main types for convenience
pub use action::{
    ActionMapper, ActionSpace, ActionValue,
    DiscreteActionMapper, DiscreteAction,
    ContinuousActionMapper, map_continuous_action_with_position,
};
pub use env::{TradingEnv, TradingEnvBuilder, TradingEnvConfig, StepResult};
pub use info::{StepInfo, EpisodeInfo, TerminationReason};
pub use observation::{
    Observation, ObservationBuilder, ObservationSpace,
    PortfolioSnapshot, MarketSnapshot,
};
pub use reward::{
    RewardFunction, PnlReward, SharpeReward, RiskAdjustedReward,
    CompositeReward, RewardFunctionType,
};
