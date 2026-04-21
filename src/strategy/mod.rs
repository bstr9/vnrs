//! Trading Strategy Framework
//! 
//! Unified strategy framework supporting both spot and futures trading.
//! Integrates with Python for strategy logic while using Rust for execution.
//!
//! # Sync vs Async strategies
//!
//! - [`StrategyTemplate`] — synchronous trait for traditional strategies
//! - [`AsyncStrategy`] — async trait for ML-driven strategies (ONNX, gRPC, LLM)
//!
//! Both can coexist: `StrategyEngine` manages sync strategies while
//! `AsyncStrategyEngine` manages async strategies.

pub mod template;
pub mod engine;
pub mod base;
pub mod volatility;
pub mod futures_template;
pub mod grid_template;
pub mod async_template;
pub mod async_engine;

pub use template::{StrategyTemplate, StrategyContext};
#[cfg(feature = "gui")]
pub use template::IndicatorRef;
pub use engine::StrategyEngine;
pub use engine::TimerEntry;
pub use base::{StrategyType, StrategyState, StrategySetting, StopOrder, StopOrderStatus, StopOrderRequest, CancelRequestType, StrategyRiskConfig};
pub use volatility::VolatilityStrategy;
pub use futures_template::{FuturesStrategy, OffsetMode};
pub use grid_template::{GridStrategy, GridLevel, GridStatus};

// Async strategy re-exports
pub use async_template::{AsyncStrategy, DecisionRecord, SignalType, StrategyError};
pub use async_engine::AsyncStrategyEngine;
