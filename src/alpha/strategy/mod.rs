//! Strategy module for alpha research
//! Provides templates for alpha strategies and backtesting
//!
//! # Adapter path (recommended)
//!
//! Use [`AlphaStrategyAdapter`] to wrap an [`AlphaStrategy`] into a [`StrategyTemplate`],
//! then run it through the standard [`BacktestingEngine`](crate::backtesting::BacktestingEngine)
//! for fill models, look-ahead bias prevention, and full statistics.
//!
//! # Legacy path (deprecated)
//!
//! The [`BacktestingEngine`] in this module is a minimal self-contained engine
//! without fill models or look-ahead prevention. Prefer the adapter path instead.

pub mod adapter;
pub mod backtesting;
pub mod template;

pub use adapter::AlphaStrategyAdapter;
#[deprecated(
    since = "0.4.0",
    note = "Use AlphaStrategyAdapter + standard BacktestingEngine instead. \
            The alpha-specific BacktestingEngine lacks fill models, look-ahead prevention, \
            and risk controls. See module-level docs."
)]
pub use backtesting::BacktestingEngine;
pub use template::AlphaStrategy;
