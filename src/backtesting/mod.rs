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
pub mod position;
pub mod fill_model;
pub mod risk_engine;
pub mod data_merge;

pub use engine::BacktestingEngine;
pub use base::{BacktestingMode, DailyResult, BacktestingResult, BacktestingStatistics};
pub use statistics::calculate_statistics;
pub use database::DatabaseLoader;
pub use optimization::{OptimizationEngine, OptimizationSettings, OptimizationTarget, Parameter, OptimizationResult};
pub use portfolio::{PortfolioBacktestingEngine, SymbolConfig, PortfolioStatistics, SymbolStatistics};
pub use position::Position;
pub use fill_model::{
    FillModel, FillResult, LiquiditySide,
    BestPriceFillModel, TwoTierFillModel, SizeAwareFillModel,
    ProbabilisticFillModel, IdealFillModel,
};
pub use risk_engine::{RiskEngine, RiskConfig, RiskCheckResult};
pub use data_merge::{BarMergeIterator, TickMergeIterator};
