//! Model Registry module — model lifecycle management, versioning, and serving.
//!
//! This module provides:
//!
//! - **`ModelStage`**: Lifecycle state machine (Development → Staging → Shadow → Canary → Production, Any → Archived)
//! - **`ModelEntry`**: Metadata record for a registered model version
//! - **`ModelMetrics`**: Performance metrics (accuracy, sharpe_ratio, max_drawdown, latency_ms)
//! - **`ModelRegistry`**: SQLite-backed or in-memory metadata store with stage transitions
//! - **`ModelServer`**: Async trait for model inference
//! - **`ZmqModelServer`**: ZMQ-based remote inference using the existing RPC infrastructure
//! - **`Prediction`**: Inference result with output map, confidence, and latency
//! - **`HealthStatus`**: Serving health indicator (Healthy / Degraded / Unhealthy)
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────────┐
//! │                  ModelRegistry                     │
//! │  (SQLite or in-memory backend)                     │
//! │  ┌─────────┐ ┌──────────┐ ┌────────────────────┐ │
//! │  │ register │ │transition│ │ update_metrics     │ │
//! │  └─────────┘ └──────────┘ └────────────────────┘ │
//! └───────────────────┬───────────────────────────────┘
//!                     │ metadata
//! ┌───────────────────▼───────────────────────────────┐
//! │              ModelServer (trait)                   │
//! │  ┌──────────────────┐  ┌──────────────────────┐  │
//! │  │ ZmqModelServer   │  │ (future: ONNX, etc.) │  │
//! │  │ (Python service) │  │                      │  │
//! │  └──────────────────┘  └──────────────────────┘  │
//! └───────────────────────────────────────────────────┘
//! ```
//!
//! # Feature Flag
//!
//! This module is gated behind the `model-registry` feature.
//! When the `sqlite` feature is also enabled, the registry can persist to a SQLite database.
//!
//! # Example
//!
//! ```rust,no_run
//! use trade_engine::model::{ModelRegistry, ModelEntry, ModelStage};
//!
//! let registry = ModelRegistry::new_in_memory();
//!
//! let entry = ModelEntry::new("btc_predictor", "1.0.0", "/models/btc_pred_v1.onnx");
//! registry.register(entry).unwrap();
//!
//! // Transition through lifecycle
//! registry.transition("btc_predictor", "1.0.0", ModelStage::Staging).unwrap();
//! registry.transition("btc_predictor", "1.0.0", ModelStage::Shadow).unwrap();
//!
//! // List models in staging
//! let staging = registry.list(Some(ModelStage::Staging));
//! assert!(staging.is_empty());
//! ```

pub mod registry;
pub mod server;
pub mod types;

// Re-export main types
pub use registry::ModelRegistry;
pub use server::{ModelServer, ZmqModelServer};
pub use types::{
    HealthStatus, ModelEntry, ModelMetrics, ModelStage, Prediction,
};
