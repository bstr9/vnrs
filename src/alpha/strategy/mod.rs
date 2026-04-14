//! Strategy module for alpha research
//! Provides templates for alpha strategies and backtesting

pub mod backtesting;
pub mod template;

pub use backtesting::BacktestingEngine;
pub use template::AlphaStrategy;
