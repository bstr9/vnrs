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
pub mod portfolio;
#[cfg(feature = "python")]
pub mod portfolio_stats;
#[cfg(feature = "python")]
pub mod order_factory;
#[cfg(feature = "python")]
pub mod message_bus;
#[cfg(feature = "python")]
pub mod risk_manager;

#[cfg(feature = "python")]
pub use strategy::Strategy;
#[cfg(feature = "python")]
pub use engine::PythonEngine;
#[cfg(feature = "python")]
#[allow(deprecated)]
pub use strategy_bindings::{PyStrategy, PyStrategyEngine};
#[cfg(feature = "python")]
pub use backtesting_bindings::{PyBacktestingEngine, PyBarData, PyBacktestingStatistics};
#[cfg(feature = "python")]
pub use strategy_adapter::{PythonStrategyAdapter, load_strategies_from_directory};
#[cfg(feature = "python")]
pub use portfolio::{PortfolioFacade, PyPosition, PositionSnapshot, PortfolioState};
#[cfg(feature = "python")]
pub use portfolio_stats::PyPortfolioStatistics;
#[cfg(feature = "python")]
pub use order_factory::{PyOrder, OrderFactory};
#[cfg(feature = "python")]
pub use message_bus::{MessageBus, PyMessage, Message, MessageBusInner};
#[cfg(feature = "python")]
pub use risk_manager::{PyRiskManager, PyRiskConfig, PyRiskCheckResult};
