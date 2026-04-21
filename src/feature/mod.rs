//! Feature Store — ML/AI Feature Management for Trading Systems
//!
//! The `feature` module provides a comprehensive feature store for managing
//! ML/AI features with:
//! - **Online store**: DashMap-based sub-microsecond concurrent reads
//! - **Offline store**: Parquet storage for backtesting (requires `alpha` feature)
//! - **Feature registry**: Definition, version, and lineage tracking
//! - **Time-travel snapshots**: Point-in-time feature state for backtesting
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      FeatureStore                           │
//! │  (Main facade combining all components)                     │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌───────────────┐  ┌───────────────┐  ┌────────────────┐ │
//! │  │  OnlineStore  │  │ OfflineStore  │  │FeatureRegistry │ │
//! │  │  (DashMap)    │  │  (Parquet)    │  │ (Versioning)   │ │
//! │  │  <1μs reads   │  │  (alpha)      │  │ (Lineage)      │ │
//! │  └───────────────┘  └───────────────┘  └────────────────┘ │
//! │  ┌───────────────────────────────────────────────────────┐│
//! │  │              SnapshotManager                           ││
//! │  │              (Time-Travel)                             ││
//! │  └───────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Features
//!
//! - `feature-store`: Enables the feature store module (requires `dashmap`)
//! - `alpha`: Enables Parquet-based offline store (also brings in `polars`, `arrow`)
//!
//! # Example
//!
//! ```rust,no_run
//! use trade_engine::feature::FeatureStore;
//! use trade_engine::feature::{FeatureDefinition, FeatureType};
//! use trade_engine::trader::BarData;
//!
//! // Create feature store
//! let mut store = FeatureStore::new();
//!
//! // Register feature definitions
//! store.register_feature(
//!     FeatureDefinition::new(
//!         "btcusdt_close",
//!         "bar.close_price",
//!         1,
//!         FeatureType::Float64,
//!     )
//! );
//!
//! // Materialize features from bar data
//! // store.materialize(&bar);
//!
//! // Get real-time features
//! // let features = store.get_online("btcusdt.binance");
//!
//! // Create time-travel snapshot
//! // let snap = store.snapshot("pre_trade");
//! ```

mod offline;
mod online;
mod registry;
mod snapshot;
mod store;
mod types;

// Re-export main types for convenience
pub use offline::OfflineStore;
pub use online::OnlineStore;
pub use registry::FeatureRegistry;
pub use snapshot::{FeatureSnapshot, SnapshotManager};
pub use store::FeatureStore;
pub use types::{FeatureDefinition, FeatureId, FeatureType, FeatureValue, FeatureVector};
