//! Trading Strategy Framework
//! 
//! Unified strategy framework supporting both spot and futures trading.
//! Integrates with Python for strategy logic while using Rust for execution.

pub mod template;
pub mod engine;
pub mod base;
pub mod volatility;

pub use template::{StrategyTemplate, StrategyContext};
#[cfg(feature = "gui")]
pub use template::IndicatorRef;
pub use engine::StrategyEngine;
pub use base::{StrategyType, StrategyState, StopOrder, StopOrderStatus, StopOrderRequest, CancelRequestType};
pub use volatility::VolatilityStrategy;
