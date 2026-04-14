//! Backtesting Module
//! 
//! CTA backtesting engine for strategy testing and optimization
//! Supports both spot and futures trading

pub mod engine;
pub mod base;
pub mod statistics;
pub mod database;
pub mod optimization;
pub mod portfolio;

pub use engine::BacktestingEngine;
pub use base::{BacktestingMode, DailyResult, BacktestingResult, BacktestingStatistics};
pub use statistics::calculate_statistics;
pub use database::DatabaseLoader;
pub use optimization::{OptimizationEngine, OptimizationSettings, OptimizationTarget, Parameter, OptimizationResult};
pub use portfolio::{PortfolioBacktestingEngine, SymbolConfig, PortfolioStatistics, SymbolStatistics};
