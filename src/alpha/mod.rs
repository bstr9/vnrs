//! Alpha module for quantitative trading strategies
//! Provides tools for alpha research, factor analysis, and strategy backtesting

pub mod dataset;
pub mod lab;
pub mod logger;
pub mod model;
pub mod strategy;
pub mod types;

// Note: dataset and strategy both have a `template` submodule, causing ambiguous glob re-export.
// We allow this since the submodules are accessed via their parent paths (dataset::template, strategy::template).
#[allow(ambiguous_glob_reexports)]
pub use dataset::*;
pub use lab::*;
pub use logger::*;
pub use model::*;
pub use strategy::*;
pub use types::*;
