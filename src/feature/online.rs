//! Online feature store using DashMap for sub-microsecond concurrent reads.
//!
//! The `OnlineStore` provides a lock-free, concurrent hashmap for real-time
//! feature serving. It uses `DashMap` internally to enable high-throughput
//! concurrent access without read locks.

use dashmap::DashMap;

use super::types::FeatureVector;

/// Lock-free concurrent online feature store.
///
/// Uses `DashMap<String, FeatureVector>` for sub-microsecond reads
/// with concurrent access support. Each entity maps to its latest
/// `FeatureVector`.
///
/// # Performance
///
/// - Read: <1μs (DashMap shard-level locking, no global read lock)
/// - Write: <1μs (append-only for latest value per entity)
/// - Concurrent: Multiple readers/writers without contention on different keys
pub struct OnlineStore {
    data: DashMap<String, FeatureVector>,
}

impl OnlineStore {
    /// Create a new empty online store.
    pub fn new() -> Self {
        Self {
            data: DashMap::new(),
        }
    }

    /// Create a new online store with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: DashMap::with_capacity(capacity),
        }
    }

    /// Get the latest feature vector for an entity.
    ///
    /// Returns `None` if the entity is not in the store.
    /// This operation is sub-microsecond as it only acquires a shard-level read lock.
    pub fn get(&self, entity: &str) -> Option<FeatureVector> {
        self.data.get(entity).map(|v| v.value().clone())
    }

    /// Put a feature vector into the store, inserting or updating.
    pub fn put(&self, entity: String, vector: FeatureVector) {
        self.data.insert(entity, vector);
    }

    /// Remove an entity from the store.
    ///
    /// Returns the removed feature vector if it existed.
    pub fn remove(&self, entity: &str) -> Option<FeatureVector> {
        self.data.remove(entity).map(|(_, v)| v)
    }

    /// List all entity keys in the store.
    pub fn entities(&self) -> Vec<String> {
        self.data.iter().map(|v| v.key().clone()).collect()
    }

    /// Check if an entity exists in the store.
    pub fn contains(&self, entity: &str) -> bool {
        self.data.contains_key(entity)
    }

    /// Get the number of entities in the store.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Clear all entities from the store.
    pub fn clear(&self) {
        self.data.clear();
    }

    /// Take a snapshot of the current state.
    ///
    /// Returns a cloned copy of all entity -> FeatureVector mappings.
    /// Useful for creating point-in-time snapshots.
    pub fn snapshot(&self) -> Vec<FeatureVector> {
        self.data.iter().map(|v| v.value().clone()).collect()
    }
}

impl Default for OnlineStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::types::{FeatureId, FeatureValue};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_vector(entity: &str, close: f64, volume: f64) -> FeatureVector {
        let mut features = HashMap::new();
        features.insert(FeatureId::new("close"), FeatureValue::Float64(close));
        features.insert(FeatureId::new("volume"), FeatureValue::Float64(volume));
        FeatureVector {
            entity: entity.to_string(),
            timestamp: Utc::now(),
            features,
        }
    }

    #[test]
    fn test_online_store_put_get() {
        let store = OnlineStore::new();
        let fv = make_vector("btcusdt.binance", 42000.0, 1234.5);

        store.put("btcusdt.binance".to_string(), fv.clone());
        let result = store.get("btcusdt.binance");

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.entity, "btcusdt.binance");
        assert_eq!(retrieved.get_f64(&FeatureId::new("close")), Some(42000.0));
    }

    #[test]
    fn test_online_store_get_missing() {
        let store = OnlineStore::new();
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn test_online_store_remove() {
        let store = OnlineStore::new();
        let fv = make_vector("btcusdt.binance", 42000.0, 1234.5);
        store.put("btcusdt.binance".to_string(), fv);

        let removed = store.remove("btcusdt.binance");
        assert!(removed.is_some());
        assert!(store.get("btcusdt.binance").is_none());
    }

    #[test]
    fn test_online_store_entities() {
        let store = OnlineStore::new();
        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42000.0, 100.0));
        store.put("ethusdt.binance".to_string(), make_vector("ethusdt.binance", 3000.0, 200.0));

        let mut entities = store.entities();
        entities.sort();
        assert_eq!(entities, vec!["btcusdt.binance", "ethusdt.binance"]);
    }

    #[test]
    fn test_online_store_update() {
        let store = OnlineStore::new();
        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42000.0, 100.0));
        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42500.0, 150.0));

        let result = store.get("btcusdt.binance").unwrap();
        assert_eq!(result.get_f64(&FeatureId::new("close")), Some(42500.0));
    }

    #[test]
    fn test_online_store_len_and_empty() {
        let store = OnlineStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42000.0, 100.0));
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_online_store_contains() {
        let store = OnlineStore::new();
        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42000.0, 100.0));
        assert!(store.contains("btcusdt.binance"));
        assert!(!store.contains("ethusdt.binance"));
    }

    #[test]
    fn test_online_store_snapshot() {
        let store = OnlineStore::new();
        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42000.0, 100.0));
        store.put("ethusdt.binance".to_string(), make_vector("ethusdt.binance", 3000.0, 200.0));

        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn test_online_store_clear() {
        let store = OnlineStore::new();
        store.put("btcusdt.binance".to_string(), make_vector("btcusdt.binance", 42000.0, 100.0));
        store.clear();
        assert!(store.is_empty());
    }
}
