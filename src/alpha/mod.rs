//! Alpha module for quantitative trading strategies
//! Provides tools for alpha research, factor analysis, and strategy backtesting

pub mod dataset;
pub mod lab;
pub mod logger;
pub mod model;
pub mod strategy;
pub mod types;

// Explicit re-exports to avoid ambiguous glob re-exports
// (dataset::template and strategy::template both exist)
pub use dataset::{query_by_time, to_datetime, AlphaDataset, FeatureExpression, Segment};
pub use lab::AlphaLab;
pub use logger::AlphaLogger;
pub use model::{
    AlphaModel, EnsembleModel, GradientBoostingModel, LinearRegressionModel, RandomForestModel,
};
pub use strategy::{AlphaStrategy, BacktestingEngine};
pub use types::AlphaBarData;
