//! Core types for the FeatureStore module.
//!
//! Provides the fundamental type definitions used across the feature store:
//! - [`FeatureId`]: Typed identifier for features (e.g., "btcusdt_close_price_1m")
//! - [`FeatureType`]: Data type enum for feature values
//! - [`FeatureValue`]: Runtime value wrapper for feature data
//! - [`FeatureVector`]: Entity-level feature collection with timestamp
//! - [`FeatureDefinition`]: Feature metadata with versioning and lineage

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Typed identifier for a feature.
///
/// Uses newtype pattern around `String` for type safety.
/// Convention: `{symbol}_{feature_name}_{window}` (e.g., "btcusdt_close_price_1m").
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct FeatureId(pub String);

impl FeatureId {
    /// Create a new FeatureId from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the inner string reference.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FeatureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FeatureId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FeatureId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Data type enum for feature values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureType {
    /// 64-bit floating point (default for most ML features)
    Float64,
    /// 64-bit signed integer
    Int64,
    /// Boolean flag
    Bool,
    /// String/categorical
    String,
}

impl Default for FeatureType {
    fn default() -> Self {
        Self::Float64
    }
}

/// Runtime value wrapper for feature data.
///
/// Supports the same types as [`FeatureType`] for type-safe storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeatureValue {
    /// 64-bit float
    Float64(f64),
    /// 64-bit signed integer
    Int64(i64),
    /// Boolean
    Bool(bool),
    /// String/categorical
    String(String),
}

impl FeatureValue {
    /// Extract f64 value if this is a Float64 variant.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float64(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract i64 value if this is an Int64 variant.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int64(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract bool value if this is a Bool variant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract string reference if this is a String variant.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(v) => Some(v),
            _ => None,
        }
    }

    /// Get the FeatureType for this value.
    pub fn feature_type(&self) -> FeatureType {
        match self {
            Self::Float64(_) => FeatureType::Float64,
            Self::Int64(_) => FeatureType::Int64,
            Self::Bool(_) => FeatureType::Bool,
            Self::String(_) => FeatureType::String,
        }
    }
}

impl From<f64> for FeatureValue {
    fn from(v: f64) -> Self {
        Self::Float64(v)
    }
}

impl From<i64> for FeatureValue {
    fn from(v: i64) -> Self {
        Self::Int64(v)
    }
}

impl From<bool> for FeatureValue {
    fn from(v: bool) -> Self {
        Self::Bool(v)
    }
}

impl From<String> for FeatureValue {
    fn from(v: String) -> Self {
        Self::String(v)
    }
}

/// Entity-level feature collection with timestamp.
///
/// A `FeatureVector` represents all feature values for a specific entity
/// (e.g., a trading symbol) at a given point in time. This is the primary
/// unit of storage and retrieval in the feature store.
///
/// # Example
///
/// ```rust
/// use trade_engine::feature::{FeatureId, FeatureVector, FeatureValue};
/// use chrono::Utc;
/// use std::collections::HashMap;
///
/// let mut features = HashMap::new();
/// features.insert(FeatureId::new("btcusdt_close"), FeatureValue::Float64(42000.0));
/// features.insert(FeatureId::new("btcusdt_volume"), FeatureValue::Float64(1234.5));
///
/// let vector = FeatureVector {
///     entity: "btcusdt.binance".to_string(),
///     timestamp: Utc::now(),
///     features,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureVector {
    /// Entity identifier (e.g., "btcusdt.binance", "ethusdt.okx")
    pub entity: String,
    /// Timestamp of this feature vector
    pub timestamp: DateTime<Utc>,
    /// Feature values keyed by FeatureId
    pub features: HashMap<FeatureId, FeatureValue>,
}

impl FeatureVector {
    /// Create a new empty FeatureVector for an entity at a given time.
    pub fn new(entity: String, timestamp: DateTime<Utc>) -> Self {
        Self {
            entity,
            timestamp,
            features: HashMap::new(),
        }
    }

    /// Insert a feature value.
    pub fn insert(&mut self, id: FeatureId, value: FeatureValue) {
        self.features.insert(id, value);
    }

    /// Get a feature value by id.
    pub fn get(&self, id: &FeatureId) -> Option<&FeatureValue> {
        self.features.get(id)
    }

    /// Get a feature value as f64.
    pub fn get_f64(&self, id: &FeatureId) -> Option<f64> {
        self.features.get(id).and_then(|v| v.as_f64())
    }

    /// Number of features in this vector.
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Check if the feature vector is empty.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

/// Feature definition with versioning and lineage.
///
/// A `FeatureDefinition` describes how a feature is computed, what its
/// dependencies are, and tracks its version history. This enables:
/// - Feature versioning for reproducibility
/// - Lineage tracking for debugging
/// - Training/serving consistency guarantees
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDefinition {
    /// Unique feature identifier
    pub id: FeatureId,
    /// Expression or description of how this feature is computed
    /// (e.g., "close_price / sma(close, 20) - 1.0")
    pub expression: String,
    /// Version number for this feature definition
    pub version: u32,
    /// List of feature ids this feature depends on (lineage)
    pub dependencies: Vec<FeatureId>,
    /// Data type of the feature value
    pub dtype: FeatureType,
    /// Human-readable description
    pub description: String,
}

impl FeatureDefinition {
    /// Create a new FeatureDefinition.
    pub fn new(
        id: impl Into<String>,
        expression: impl Into<String>,
        version: u32,
        dtype: FeatureType,
    ) -> Self {
        Self {
            id: FeatureId::new(id),
            expression: expression.into(),
            version,
            dependencies: Vec::new(),
            dtype,
            description: String::new(),
        }
    }

    /// Add a dependency to this feature definition.
    pub fn with_dependency(mut self, dep: FeatureId) -> Self {
        self.dependencies.push(dep);
        self
    }

    /// Set the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_id_new() {
        let id = FeatureId::new("btcusdt_close_price_1m");
        assert_eq!(id.as_str(), "btcusdt_close_price_1m");
    }

    #[test]
    fn test_feature_id_display() {
        let id = FeatureId::new("btcusdt_close_price_1m");
        assert_eq!(format!("{id}"), "btcusdt_close_price_1m");
    }

    #[test]
    fn test_feature_id_from_str() {
        let id: FeatureId = "btcusdt_close".into();
        assert_eq!(id.as_str(), "btcusdt_close");
    }

    #[test]
    fn test_feature_id_equality() {
        let id1 = FeatureId::new("btcusdt_close");
        let id2 = FeatureId::new("btcusdt_close");
        let id3 = FeatureId::new("btcusdt_volume");
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_feature_value_float() {
        let v = FeatureValue::Float64(42.0);
        assert_eq!(v.as_f64(), Some(42.0));
        assert_eq!(v.feature_type(), FeatureType::Float64);
    }

    #[test]
    fn test_feature_value_int() {
        let v = FeatureValue::Int64(100);
        assert_eq!(v.as_i64(), Some(100));
        assert_eq!(v.feature_type(), FeatureType::Int64);
    }

    #[test]
    fn test_feature_value_bool() {
        let v = FeatureValue::Bool(true);
        assert_eq!(v.as_bool(), Some(true));
        assert_eq!(v.feature_type(), FeatureType::Bool);
    }

    #[test]
    fn test_feature_value_string() {
        let v = FeatureValue::String("hello".to_string());
        assert_eq!(v.as_str(), Some("hello"));
        assert_eq!(v.feature_type(), FeatureType::String);
    }

    #[test]
    fn test_feature_value_wrong_type() {
        let v = FeatureValue::Float64(42.0);
        assert_eq!(v.as_i64(), None);
        assert_eq!(v.as_bool(), None);
        assert_eq!(v.as_str(), None);
    }

    #[test]
    fn test_feature_value_from_conversions() {
        let v1: FeatureValue = 42.0_f64.into();
        assert_eq!(v1.as_f64(), Some(42.0));

        let v2: FeatureValue = 100_i64.into();
        assert_eq!(v2.as_i64(), Some(100));

        let v3: FeatureValue = true.into();
        assert_eq!(v3.as_bool(), Some(true));

        let v4: FeatureValue = String::from("test").into();
        assert_eq!(v4.as_str(), Some("test"));
    }

    #[test]
    fn test_feature_vector_new() {
        let fv = FeatureVector::new("btcusdt.binance".to_string(), Utc::now());
        assert_eq!(fv.entity, "btcusdt.binance");
        assert!(fv.is_empty());
        assert_eq!(fv.len(), 0);
    }

    #[test]
    fn test_feature_vector_insert_get() {
        let mut fv = FeatureVector::new("btcusdt.binance".to_string(), Utc::now());
        fv.insert(FeatureId::new("close"), FeatureValue::Float64(42000.0));
        fv.insert(FeatureId::new("volume"), FeatureValue::Float64(1234.5));

        assert_eq!(fv.len(), 2);
        assert_eq!(fv.get_f64(&FeatureId::new("close")), Some(42000.0));
        assert_eq!(fv.get_f64(&FeatureId::new("volume")), Some(1234.5));
        assert_eq!(fv.get_f64(&FeatureId::new("unknown")), None);
    }

    #[test]
    fn test_feature_definition_new() {
        let def = FeatureDefinition::new(
            "btcusdt_returns",
            "close / prev_close - 1.0",
            1,
            FeatureType::Float64,
        );
        assert_eq!(def.id.as_str(), "btcusdt_returns");
        assert_eq!(def.version, 1);
        assert!(def.dependencies.is_empty());
    }

    #[test]
    fn test_feature_definition_builder() {
        let def = FeatureDefinition::new(
            "btcusdt_momentum",
            "close - sma(close, 20)",
            2,
            FeatureType::Float64,
        )
        .with_dependency(FeatureId::new("btcusdt_close"))
        .with_dependency(FeatureId::new("btcusdt_sma_20"))
        .with_description("Price momentum vs 20-period SMA");

        assert_eq!(def.dependencies.len(), 2);
        assert_eq!(def.description, "Price momentum vs 20-period SMA");
    }
}
