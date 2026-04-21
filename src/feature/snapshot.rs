//! Time-travel snapshot system for feature store.
//!
//! Snapshots capture the state of the online store at a point in time,
//! enabling time-travel queries for backtesting and reproducibility.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::FeatureVector;

/// A named snapshot of the feature store at a point in time.
///
/// Snapshots capture all entity feature vectors and are tagged
/// with a human-readable name and timestamp for retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSnapshot {
    /// Human-readable tag for this snapshot (e.g., "pre_open", "after_fill")
    pub tag: String,
    /// When this snapshot was created
    pub created_at: DateTime<Utc>,
    /// The feature vectors captured in this snapshot
    pub vectors: Vec<FeatureVector>,
}

impl FeatureSnapshot {
    /// Create a new snapshot with the given tag and vectors.
    pub fn new(tag: impl Into<String>, vectors: Vec<FeatureVector>) -> Self {
        Self {
            tag: tag.into(),
            created_at: Utc::now(),
            vectors,
        }
    }

    /// Get a feature vector for an entity from this snapshot.
    pub fn get_entity(&self, entity: &str) -> Option<&FeatureVector> {
        self.vectors.iter().find(|v| v.entity == entity)
    }

    /// Get the number of entities in this snapshot.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

/// Manager for feature store snapshots with time-travel support.
///
/// Maintains an ordered list of snapshots and supports retrieval
/// by tag or by timestamp.
pub struct SnapshotManager {
    snapshots: Vec<FeatureSnapshot>,
}

impl SnapshotManager {
    /// Create a new empty snapshot manager.
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }

    /// Add a snapshot.
    pub fn add(&mut self, snapshot: FeatureSnapshot) {
        self.snapshots.push(snapshot);
    }

    /// Get a snapshot by tag.
    pub fn get_by_tag(&self, tag: &str) -> Option<&FeatureSnapshot> {
        self.snapshots.iter().find(|s| s.tag == tag)
    }

    /// Get the most recent snapshot before a given timestamp.
    ///
    /// Useful for time-travel: "what were the features at time T?"
    pub fn get_before(&self, timestamp: DateTime<Utc>) -> Option<&FeatureSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.created_at <= timestamp)
            .last()
    }

    /// Get the latest snapshot.
    pub fn latest(&self) -> Option<&FeatureSnapshot> {
        self.snapshots.last()
    }

    /// List all snapshot tags.
    pub fn tags(&self) -> Vec<&str> {
        self.snapshots.iter().map(|s| s.tag.as_str()).collect()
    }

    /// Get the number of snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Check if there are no snapshots.
    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::types::{FeatureId, FeatureValue};
    use std::collections::HashMap;

    fn make_vector(entity: &str, close: f64) -> FeatureVector {
        let mut features = HashMap::new();
        features.insert(FeatureId::new("close"), FeatureValue::Float64(close));
        FeatureVector {
            entity: entity.to_string(),
            timestamp: Utc::now(),
            features,
        }
    }

    #[test]
    fn test_snapshot_new() {
        let vectors = vec![
            make_vector("btcusdt", 42000.0),
            make_vector("ethusdt", 3000.0),
        ];
        let snap = FeatureSnapshot::new("pre_open", vectors);

        assert_eq!(snap.tag, "pre_open");
        assert_eq!(snap.len(), 2);
        assert!(snap.created_at <= Utc::now());
    }

    #[test]
    fn test_snapshot_get_entity() {
        let vectors = vec![
            make_vector("btcusdt", 42000.0),
            make_vector("ethusdt", 3000.0),
        ];
        let snap = FeatureSnapshot::new("test", vectors);

        let btc = snap.get_entity("btcusdt");
        assert!(btc.is_some());
        assert_eq!(btc.unwrap().get_f64(&FeatureId::new("close")), Some(42000.0));

        assert!(snap.get_entity("missing").is_none());
    }

    #[test]
    fn test_snapshot_manager_add_get() {
        let mut mgr = SnapshotManager::new();
        mgr.add(FeatureSnapshot::new("snap1", vec![make_vector("btcusdt", 42000.0)]));
        mgr.add(FeatureSnapshot::new("snap2", vec![make_vector("btcusdt", 42500.0)]));

        assert_eq!(mgr.len(), 2);
        assert!(mgr.get_by_tag("snap1").is_some());
        assert!(mgr.get_by_tag("snap2").is_some());
        assert!(mgr.get_by_tag("nonexistent").is_none());
    }

    #[test]
    fn test_snapshot_manager_latest() {
        let mut mgr = SnapshotManager::new();
        assert!(mgr.latest().is_none());

        mgr.add(FeatureSnapshot::new("first", vec![]));
        mgr.add(FeatureSnapshot::new("second", vec![]));

        let latest = mgr.latest().unwrap();
        assert_eq!(latest.tag, "second");
    }

    #[test]
    fn test_snapshot_manager_tags() {
        let mut mgr = SnapshotManager::new();
        mgr.add(FeatureSnapshot::new("alpha", vec![]));
        mgr.add(FeatureSnapshot::new("beta", vec![]));

        let tags = mgr.tags();
        assert_eq!(tags, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_snapshot_manager_get_before() {
        let mut mgr = SnapshotManager::new();

        let mut snap1 = FeatureSnapshot::new("old", vec![]);
        snap1.created_at = Utc::now() - chrono::Duration::hours(2);
        let mut snap2 = FeatureSnapshot::new("recent", vec![]);
        snap2.created_at = Utc::now() - chrono::Duration::hours(1);

        mgr.add(snap1);
        mgr.add(snap2);

        let cutoff = Utc::now() - chrono::Duration::minutes(90);
        let result = mgr.get_before(cutoff);
        assert!(result.is_some());
        assert_eq!(result.unwrap().tag, "old");
    }
}
