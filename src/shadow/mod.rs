//! Shadow Deployment Module - shadow models, predictions, and promotion.
//!
//! This module provides infrastructure for shadow deployment of ML models:
//! models receive live market data and produce predictions, but those predictions
//! are recorded without executing trades. This allows for:
//!
//! - Safe evaluation of new models in production environments
//! - Comparison of predicted vs actual outcomes
//! - Automated promotion decisions based on performance metrics
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │                        ShadowEngine                               │
//! │  ┌─────────────┐  ┌─────────────────┐  ┌────────────────────┐   │
//! │  │ ShadowModel │  │ PredictionComp. │  │ PromotionPolicy    │   │
//! │  │  - stage    │  │  - predicted    │  │  - min_predictions │   │
//! │  │  - metrics  │  │  - actual       │  │  - min_accuracy    │   │
//! │  └─────────────┘  └─────────────────┘  └────────────────────┘   │
//! └──────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Stages
//!
//! Models progress through three stages:
//! - **Shadow**: Record predictions, NO trading
//! - **Canary**: Small allocation trading
//! - **Production**: Full trading
//!
//! # Example
//!
//! ```rust,ignore
//! use trade_engine::shadow::{ShadowEngine, ShadowStage, PromotionPolicy};
//!
//! // Create engine with default promotion policy
//! let engine = ShadowEngine::new();
//!
//! // Register a new shadow model
//! engine.register_model("my_model", "1.0.0").unwrap();
//!
//! // Record predictions (no trades executed in Shadow stage)
//! let prediction = engine.predict(
//!     "my_model",
//!     "1.0.0",
//!     "BTCUSDT.BINANCE",
//!     Direction::Long,
//!     100.0,
//!     0.85,
//! ).unwrap();
//!
//! // Later, record the actual outcome
//! let comparison = engine.record_outcome(prediction, 80.0, Direction::Long);
//!
//! // Evaluate promotion eligibility
//! let decision = engine.evaluate_promotion("my_model", "1.0.0").unwrap();
//! ```

pub mod comparison;
pub mod engine;
pub mod metrics;
pub mod promotion;

// Re-export main types
pub use comparison::{ComparisonStore, PredictionComparison, ShadowPrediction};
pub use engine::{ShadowEngine, ShadowModel, ShadowStage};
pub use metrics::{ConfidenceDistribution, RollingMetrics, ShadowMetrics};
pub use promotion::{PromotionDecision, PromotionPolicy};
