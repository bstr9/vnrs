//! Strategy module for alpha research
//! Provides templates for alpha strategies and backtesting

pub mod template;
pub mod backtesting;

pub use template::AlphaStrategy;
pub use backtesting::BacktestingEngine;