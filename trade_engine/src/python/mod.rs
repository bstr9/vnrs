//! Python integration for the trading engine
//! Provides interfaces for Python-based trading strategies

#[cfg(feature = "python")]
pub mod strategy;
#[cfg(feature = "python")]
pub mod engine;
#[cfg(feature = "python")]
pub mod data_converter;
#[cfg(feature = "python")]
pub mod bindings;
#[cfg(feature = "python")]
pub mod strategy_bindings;
#[cfg(feature = "python")]
pub mod backtesting_bindings;
#[cfg(feature = "python")]
pub mod strategy_adapter;

#[cfg(feature = "python")]
pub use strategy::PythonStrategy;
#[cfg(feature = "python")]
pub use engine::PythonEngine;
#[cfg(feature = "python")]
pub use strategy_bindings::{PyStrategy, PyStrategyEngine};
#[cfg(feature = "python")]
pub use backtesting_bindings::{PyBacktestingEngine, PyBarData, PyBacktestingStatistics};
#[cfg(feature = "python")]
pub use strategy_adapter::{PythonStrategyAdapter, load_strategies_from_directory};