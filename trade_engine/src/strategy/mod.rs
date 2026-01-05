//! Trading Strategy Framework
//! 
//! Unified strategy framework supporting both spot and futures trading.
//! Integrates with Python for strategy logic while using Rust for execution.

pub mod template;
pub mod engine;
pub mod base;

pub use template::{StrategyTemplate, StrategyContext};
pub use engine::StrategyEngine;
pub use base::{StrategyType, StrategyState, StopOrder, StopOrderStatus};
