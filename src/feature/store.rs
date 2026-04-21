//! FeatureStore — the main facade for the feature store system.
//!
//! `FeatureStore` is the primary interface for feature management, combining:
//! - [`OnlineStore`] for real-time sub-microsecond feature serving
//! - [`OfflineStore`] for persistent Parquet-based backtesting storage
//! - [`FeatureRegistry`] for definition, versioning, and lineage
//! - [`SnapshotManager`] for time-travel snapshot support
//!
//! # Example
//!
//! ```rust,no_run
//! use trade_engine::feature::FeatureStore;
//! use trade_engine::trader::BarData;
//!
//! let store = FeatureStore::new();
//!
//! // Materialize features from a bar
//! // store.materialize(&bar);
//!
//! // Get latest features for an entity
//! // let features = store.get_online("btcusdt.binance");
//! ```

use chrono::Utc;

use super::offline::OfflineStore;
use super::online::OnlineStore;
use super::registry::FeatureRegistry;
use super::snapshot::{FeatureSnapshot, SnapshotManager};
use super::types::{FeatureDefinition, FeatureId, FeatureValue, FeatureVector};
use crate::trader::object::BarData;

/// The main facade for the feature store system.
///
/// Combines online store, offline store, registry, and snapshot
/// manager into a single cohesive interface.
pub struct FeatureStore {
    /// Real-time online feature store (DashMap-based)
    online: OnlineStore,
    /// Persistent offline feature store (Parquet-based, requires alpha)
    offline: OfflineStore,
    /// Feature definition registry with versioning and lineage
    registry: FeatureRegistry,
    /// Snapshot manager for time-travel queries
    snapshots: SnapshotManager,
}

impl FeatureStore {
    /// Create a new FeatureStore with default configuration.
    pub fn new() -> Self {
        Self {
            online: OnlineStore::new(),
            offline: OfflineStore::with_default_path(),
            registry: FeatureRegistry::new(),
            snapshots: SnapshotManager::new(),
        }
    }

    /// Create a new FeatureStore with a custom offline store path.
    pub fn with_offline_path(path: impl Into<String>) -> Self {
        Self {
            online: OnlineStore::new(),
            offline: OfflineStore::new(path),
            registry: FeatureRegistry::new(),
            snapshots: SnapshotManager::new(),
        }
    }

    // ==================== Online Store Operations ====================

    /// Get the latest feature vector for an entity from the online store.
    ///
    /// This is the primary method for real-time feature serving.
    /// Returns `None` if the entity has no features in the online store.
    pub fn get_online(&self, entity: &str) -> Option<FeatureVector> {
        self.online.get(entity)
    }

    /// Put a feature vector into the online store.
    pub fn put_online(&self, entity: String, vector: FeatureVector) {
        self.online.put(entity, vector);
    }

    /// Remove an entity from the online store.
    pub fn remove_online(&self, entity: &str) -> Option<FeatureVector> {
        self.online.remove(entity)
    }

    /// List all entities in the online store.
    pub fn online_entities(&self) -> Vec<String> {
        self.online.entities()
    }

    // ==================== Offline Store Operations ====================

    /// Save feature vectors to offline Parquet storage.
    ///
    /// Requires the `alpha` feature. Returns an error if alpha is not enabled.
    pub fn save_offline(&self, path: &str, vectors: &[FeatureVector]) -> Result<(), String> {
        self.offline.save(path, vectors)
    }

    /// Load feature vectors from offline Parquet storage.
    ///
    /// Requires the `alpha` feature. Returns an error if alpha is not enabled.
    pub fn load_offline(
        &self,
        path: &str,
        entity: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<FeatureVector>, String> {
        self.offline.load(path, entity, start, end)
    }

    // ==================== Time-Travel ====================

    /// Get feature vector for an entity at a specific timestamp.
    ///
    /// Checks the online store first. If the online store's timestamp
    /// doesn't match, falls back to the offline store (requires alpha).
    /// Also checks snapshots for time-travel queries.
    pub fn get_at(&self, entity: &str, timestamp: &str) -> Option<FeatureVector> {
        // Try online store first
        if let Some(online_vec) = self.online.get(entity) {
            let ts = chrono::DateTime::parse_from_rfc3339(timestamp)
                .map(|dt| dt.to_utc())
                .unwrap_or_else(|_| Utc::now());
            if online_vec.timestamp <= ts {
                return Some(online_vec);
            }
        }

        // Try snapshots
        let ts = chrono::DateTime::parse_from_rfc3339(timestamp)
            .map(|dt| dt.to_utc())
            .unwrap_or_else(|_| Utc::now());
        if let Some(snap) = self.snapshots.get_before(ts) {
            if let Some(vec) = snap.get_entity(entity) {
                return Some(vec.clone());
            }
        }

        // Try offline store (may fail if alpha not enabled)
        #[allow(clippy::let_and_return)]
        let offline_result = self
            .offline
            .load(entity, entity, timestamp, timestamp)
            .ok()
            .and_then(|v| v.into_iter().next());

        offline_result
    }

    // ==================== Feature Materialization ====================

    /// Compute features from a `BarData` and store them in the online store.
    ///
    /// Computes the following features from bar data:
    /// - `close`: Close price
    /// - `volume`: Bar volume
    /// - `returns`: Simple return (close / prev_close - 1)
    /// - `volatility`: Approximate volatility (high - low) / close
    /// - `vwap`: Volume-weighted average price (turnover / volume)
    ///
    /// If a previous feature vector exists for this entity, `returns` is
    /// computed using the previous close as the reference price.
    pub fn materialize(&self, bar: &BarData) {
        let entity = bar.vt_symbol().to_lowercase();
        let mut features = std::collections::HashMap::new();

        // Close price
        features.insert(
            FeatureId::new(format!("{entity}_close")),
            FeatureValue::Float64(bar.close_price),
        );

        // Volume
        features.insert(
            FeatureId::new(format!("{entity}_volume")),
            FeatureValue::Float64(bar.volume),
        );

        // Returns (simple return)
        let prev_close = self
            .online
            .get(&entity)
            .and_then(|v| v.get_f64(&FeatureId::new(format!("{entity}_close"))));
        let returns = if let Some(prev) = prev_close {
            if prev != 0.0 {
                bar.close_price / prev - 1.0
            } else {
                0.0
            }
        } else {
            0.0
        };
        features.insert(
            FeatureId::new(format!("{entity}_returns")),
            FeatureValue::Float64(returns),
        );

        // Volatility approximation: (high - low) / close
        let volatility = if bar.close_price != 0.0 {
            (bar.high_price - bar.low_price) / bar.close_price
        } else {
            0.0
        };
        features.insert(
            FeatureId::new(format!("{entity}_volatility")),
            FeatureValue::Float64(volatility),
        );

        // VWAP: turnover / volume
        let vwap = if bar.volume != 0.0 {
            bar.turnover / bar.volume
        } else {
            bar.close_price
        };
        features.insert(
            FeatureId::new(format!("{entity}_vwap")),
            FeatureValue::Float64(vwap),
        );

        let vector = FeatureVector {
            entity: entity.clone(),
            timestamp: bar.datetime,
            features,
        };

        self.online.put(entity, vector);
    }

    // ==================== Snapshot Operations ====================

    /// Create a snapshot of the current online store state.
    ///
    /// The snapshot is tagged with the given name and stored
    /// in the snapshot manager for time-travel queries.
    pub fn snapshot(&mut self, tag: impl Into<String>) -> FeatureSnapshot {
        let vectors = self.online.snapshot();
        let snap = FeatureSnapshot::new(tag, vectors);
        self.snapshots.add(snap.clone());
        snap
    }

    /// Get a snapshot by tag.
    pub fn get_snapshot(&self, tag: &str) -> Option<&FeatureSnapshot> {
        self.snapshots.get_by_tag(tag)
    }

    // ==================== Registry Operations ====================

    /// Register a feature definition.
    pub fn register_feature(&mut self, definition: FeatureDefinition) {
        self.registry.register(definition);
    }

    /// Get a feature definition by id (latest version).
    pub fn get_feature_definition(&self, id: &FeatureId) -> Option<&FeatureDefinition> {
        self.registry.get(id)
    }

    /// List all registered feature definitions.
    pub fn list_features(&self) -> Vec<&FeatureDefinition> {
        self.registry.list()
    }

    /// Get the dependency lineage for a feature.
    pub fn feature_lineage(&self, id: &FeatureId) -> Vec<FeatureId> {
        self.registry.lineage(id)
    }

    /// List all versions of a feature.
    pub fn feature_versions(&self, id: &FeatureId) -> Vec<u32> {
        self.registry.versions(id)
    }

    // ==================== Accessors ====================

    /// Get a reference to the online store.
    pub fn online_store(&self) -> &OnlineStore {
        &self.online
    }

    /// Get a reference to the offline store.
    pub fn offline_store(&self) -> &OfflineStore {
        &self.offline
    }

    /// Get a reference to the registry.
    pub fn registry(&self) -> &FeatureRegistry {
        &self.registry
    }

    /// Get a reference to the snapshot manager.
    pub fn snapshot_manager(&self) -> &SnapshotManager {
        &self.snapshots
    }
}

impl Default for FeatureStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::FeatureType;
    use crate::trader::constant::{Exchange, Interval};

    fn make_bar(symbol: &str, close: f64, volume: f64, high: f64, low: f64) -> BarData {
        BarData {
            gateway_name: "test".to_string(),
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            volume,
            turnover: close * volume,
            open_interest: 0.0,
            open_price: close - 10.0,
            high_price: high,
            low_price: low,
            close_price: close,
            extra: None,
        }
    }

    #[test]
    fn test_feature_store_new() {
        let store = FeatureStore::new();
        assert!(store.online_entities().is_empty());
    }

    #[test]
    fn test_feature_store_materialize() {
        let store = FeatureStore::new();
        let bar = make_bar("BTCUSDT", 42000.0, 100.0, 42100.0, 41900.0);
        store.materialize(&bar);

        let entity = "btcusdt.binance";
        let result = store.get_online(entity);
        assert!(result.is_some());

        let fv = result.unwrap();
        assert_eq!(fv.get_f64(&FeatureId::new("btcusdt.binance_close")), Some(42000.0));
        assert_eq!(fv.get_f64(&FeatureId::new("btcusdt.binance_volume")), Some(100.0));

        // Returns should be 0 on first bar (no previous close)
        assert_eq!(fv.get_f64(&FeatureId::new("btcusdt.binance_returns")), Some(0.0));

        // Volatility = (42100 - 41900) / 42000 = 0.00476...
        let vol = fv.get_f64(&FeatureId::new("btcusdt.binance_volatility")).unwrap();
        assert!((vol - 0.004761904).abs() < 1e-5);

        // VWAP = turnover / volume = 42000 * 100 / 100 = 42000.0
        assert_eq!(fv.get_f64(&FeatureId::new("btcusdt.binance_vwap")), Some(42000.0));
    }

    #[test]
    fn test_feature_store_materialize_returns() {
        let store = FeatureStore::new();

        // First bar: no previous close, returns = 0
        let bar1 = make_bar("BTCUSDT", 100.0, 50.0, 105.0, 95.0);
        store.materialize(&bar1);

        // Second bar: returns = 110 / 100 - 1 = 0.1
        let bar2 = make_bar("BTCUSDT", 110.0, 60.0, 115.0, 105.0);
        store.materialize(&bar2);

        let result = store.get_online("btcusdt.binance").unwrap();
        let returns = result.get_f64(&FeatureId::new("btcusdt.binance_returns")).unwrap();
        assert!((returns - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_feature_store_put_get_online() {
        let store = FeatureStore::new();
        let mut fv = FeatureVector::new("ethusdt.binance".to_string(), Utc::now());
        fv.insert(FeatureId::new("close"), FeatureValue::Float64(3000.0));

        store.put_online("ethusdt.binance".to_string(), fv);
        let result = store.get_online("ethusdt.binance");
        assert!(result.is_some());
        assert_eq!(result.unwrap().get_f64(&FeatureId::new("close")), Some(3000.0));
    }

    #[test]
    fn test_feature_store_remove_online() {
        let store = FeatureStore::new();
        let fv = FeatureVector::new("ethusdt.binance".to_string(), Utc::now());
        store.put_online("ethusdt.binance".to_string(), fv);

        let removed = store.remove_online("ethusdt.binance");
        assert!(removed.is_some());
        assert!(store.get_online("ethusdt.binance").is_none());
    }

    #[test]
    fn test_feature_store_snapshot() {
        let mut store = FeatureStore::new();
        let bar = make_bar("BTCUSDT", 42000.0, 100.0, 42100.0, 41900.0);
        store.materialize(&bar);

        let snap = store.snapshot("pre_trade");
        assert_eq!(snap.tag, "pre_trade");
        assert!(!snap.is_empty());

        // Can retrieve by tag
        let retrieved = store.get_snapshot("pre_trade");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_feature_store_registry() {
        let mut store = FeatureStore::new();

        store.register_feature(
            FeatureDefinition::new("btcusdt_close", "bar.close_price", 1, FeatureType::Float64)
                .with_description("Close price"),
        );
        store.register_feature(
            FeatureDefinition::new("btcusdt_sma_20", "sma(close, 20)", 1, FeatureType::Float64)
                .with_dependency(FeatureId::new("btcusdt_close")),
        );

        let features = store.list_features();
        assert_eq!(features.len(), 2);

        let lineage = store.feature_lineage(&FeatureId::new("btcusdt_sma_20"));
        assert_eq!(lineage.len(), 2);
        assert!(lineage.contains(&FeatureId::new("btcusdt_sma_20")));
        assert!(lineage.contains(&FeatureId::new("btcusdt_close")));
    }

    #[test]
    fn test_feature_store_get_at_from_online() {
        let store = FeatureStore::new();
        let bar = make_bar("BTCUSDT", 42000.0, 100.0, 42100.0, 41900.0);
        store.materialize(&bar);

        // Query at a future time — should still return the online value
        let future_ts = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        let result = store.get_at("btcusdt.binance", &future_ts);
        assert!(result.is_some());
    }

    #[test]
    fn test_feature_store_get_at_missing() {
        let store = FeatureStore::new();
        let result = store.get_at("nonexistent", "2024-01-01T00:00:00Z");
        assert!(result.is_none());
    }

    #[test]
    fn test_feature_store_materialize_volatility_zero_close() {
        let store = FeatureStore::new();
        let bar = make_bar("BTCUSDT", 0.0, 100.0, 10.0, 0.0);
        store.materialize(&bar);

        let result = store.get_online("btcusdt.binance").unwrap();
        // With close=0, volatility should be 0 (division guard)
        assert_eq!(result.get_f64(&FeatureId::new("btcusdt.binance_volatility")), Some(0.0));
    }
}
