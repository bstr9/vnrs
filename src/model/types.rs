//! Type definitions for the Model Registry module.
//!
//! Contains the core data types for model lifecycle management:
//! stage state machine, model metadata, inference results, and health status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// ---------------------------------------------------------------------------
// ModelStage — lifecycle state machine
// ---------------------------------------------------------------------------

/// Lifecycle stages for a registered model.
///
/// Valid transitions:
/// - Development → Staging
/// - Staging → Shadow
/// - Shadow → Canary
/// - Canary → Production
/// - Any → Archived
///
/// All other transitions are invalid and return an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStage {
    Development,
    Staging,
    Shadow,
    Canary,
    Production,
    Archived,
}

impl ModelStage {
    /// Returns an ordered list of the "forward" progression stages.
    pub fn progression() -> &'static [ModelStage] {
        &[
            ModelStage::Development,
            ModelStage::Staging,
            ModelStage::Shadow,
            ModelStage::Canary,
            ModelStage::Production,
        ]
    }

    /// Check whether transitioning from `self` to `target` is valid.
    pub fn can_transition_to(&self, target: &ModelStage) -> bool {
        if matches!(target, ModelStage::Archived) {
            return true;
        }
        if matches!(self, ModelStage::Archived) {
            return false;
        }
        let progression = Self::progression();
        let from_idx = progression.iter().position(|s| s == self);
        let to_idx = progression.iter().position(|s| s == target);
        match (from_idx, to_idx) {
            (Some(from), Some(to)) => to == from + 1,
            _ => false,
        }
    }

    /// Attempt transition, returning `Ok(())` if valid or a descriptive error.
    pub fn validate_transition(&self, target: &ModelStage) -> Result<(), String> {
        if self.can_transition_to(target) {
            Ok(())
        } else {
            Err(format!(
                "Invalid stage transition: {:?} -> {:?}. Valid transitions: Development→Staging, Staging→Shadow, Shadow→Canary, Canary→Production, Any→Archived",
                self, target
            ))
        }
    }
}

impl fmt::Display for ModelStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelStage::Development => write!(f, "development"),
            ModelStage::Staging => write!(f, "staging"),
            ModelStage::Shadow => write!(f, "shadow"),
            ModelStage::Canary => write!(f, "canary"),
            ModelStage::Production => write!(f, "production"),
            ModelStage::Archived => write!(f, "archived"),
        }
    }
}

// ---------------------------------------------------------------------------
// ModelMetrics — quantitative evaluation of a model
// ---------------------------------------------------------------------------

/// Performance metrics for a model version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    /// Classification / regression accuracy (0.0–1.0).
    pub accuracy: f64,
    /// Risk-adjusted return metric.
    pub sharpe_ratio: f64,
    /// Maximum peak-to-trough drawdown (negative value).
    pub max_drawdown: f64,
    /// Average inference latency in milliseconds.
    pub latency_ms: f64,
}

impl Default for ModelMetrics {
    fn default() -> Self {
        Self {
            accuracy: 0.0,
            sharpe_ratio: 0.0,
            max_drawdown: 0.0,
            latency_ms: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// ModelEntry — metadata record for a registered model version
// ---------------------------------------------------------------------------

/// A single registered model version in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    /// Unique model identifier (e.g. "btc_predictor_v2").
    pub model_id: String,
    /// Semantic version string (e.g. "1.0.0").
    pub version: String,
    /// Current lifecycle stage.
    pub stage: ModelStage,
    /// Filesystem path (or URI) to the model artifact.
    pub artifact_path: String,
    /// Latest evaluation metrics.
    pub metrics: ModelMetrics,
    /// Feature IDs this model consumes.
    pub feature_ids: Vec<String>,
    /// Timestamp: when the model was registered.
    pub created_at: DateTime<Utc>,
    /// Timestamp: when the model was last updated (stage transition, metrics update, etc.).
    pub updated_at: DateTime<Utc>,
}

impl ModelEntry {
    /// Create a new `ModelEntry` in the `Development` stage with current timestamps.
    pub fn new(model_id: impl Into<String>, version: impl Into<String>, artifact_path: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            model_id: model_id.into(),
            version: version.into(),
            stage: ModelStage::Development,
            artifact_path: artifact_path.into(),
            metrics: ModelMetrics::default(),
            feature_ids: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Composite key for lookups: `"model_id:version"`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.model_id, self.version)
    }
}

// ---------------------------------------------------------------------------
// Prediction — inference result
// ---------------------------------------------------------------------------

/// Result of a model prediction call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    /// The model that produced this prediction.
    pub model_id: String,
    /// Version of the model.
    pub version: String,
    /// Named output values (e.g. `{"signal": 0.85, "direction": 1.0}`).
    pub output: HashMap<String, f64>,
    /// Confidence score (0.0–1.0), if applicable.
    pub confidence: f64,
    /// End-to-end inference latency in microseconds.
    pub latency_us: u64,
}

// ---------------------------------------------------------------------------
// HealthStatus — serving health
// ---------------------------------------------------------------------------

/// Health status of a model server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Server is fully operational.
    Healthy,
    /// Server is operational but with degraded performance.
    Degraded,
    /// Server is unable to serve predictions.
    Unhealthy,
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_forward_transitions() {
        assert!(ModelStage::Development.can_transition_to(&ModelStage::Staging));
        assert!(ModelStage::Staging.can_transition_to(&ModelStage::Shadow));
        assert!(ModelStage::Shadow.can_transition_to(&ModelStage::Canary));
        assert!(ModelStage::Canary.can_transition_to(&ModelStage::Production));
    }

    #[test]
    fn test_any_to_archived() {
        assert!(ModelStage::Development.can_transition_to(&ModelStage::Archived));
        assert!(ModelStage::Staging.can_transition_to(&ModelStage::Archived));
        assert!(ModelStage::Shadow.can_transition_to(&ModelStage::Archived));
        assert!(ModelStage::Canary.can_transition_to(&ModelStage::Archived));
        assert!(ModelStage::Production.can_transition_to(&ModelStage::Archived));
    }

    #[test]
    fn test_archived_cannot_transition() {
        assert!(!ModelStage::Archived.can_transition_to(&ModelStage::Development));
        assert!(!ModelStage::Archived.can_transition_to(&ModelStage::Production));
    }

    #[test]
    fn test_invalid_transitions() {
        // Skip stages
        assert!(!ModelStage::Development.can_transition_to(&ModelStage::Shadow));
        assert!(!ModelStage::Development.can_transition_to(&ModelStage::Canary));
        assert!(!ModelStage::Development.can_transition_to(&ModelStage::Production));
        // Backward
        assert!(!ModelStage::Staging.can_transition_to(&ModelStage::Development));
        assert!(!ModelStage::Production.can_transition_to(&ModelStage::Canary));
        // Same stage
        assert!(!ModelStage::Development.can_transition_to(&ModelStage::Development));
    }

    #[test]
    fn test_validate_transition_ok() {
        assert!(ModelStage::Development.validate_transition(&ModelStage::Staging).is_ok());
    }

    #[test]
    fn test_validate_transition_err() {
        let err = ModelStage::Development.validate_transition(&ModelStage::Production).unwrap_err();
        assert!(err.contains("Invalid stage transition"));
    }

    #[test]
    fn test_model_entry_key() {
        let entry = ModelEntry::new("my_model", "0.1.0", "/models/my_model_v0.1.0.onnx");
        assert_eq!(entry.key(), "my_model:0.1.0");
    }

    #[test]
    fn test_model_entry_default_stage() {
        let entry = ModelEntry::new("m", "1.0", "/path");
        assert_eq!(entry.stage, ModelStage::Development);
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(format!("{}", HealthStatus::Healthy), "healthy");
        assert_eq!(format!("{}", HealthStatus::Degraded), "degraded");
        assert_eq!(format!("{}", HealthStatus::Unhealthy), "unhealthy");
    }

    #[test]
    fn test_model_stage_display() {
        assert_eq!(format!("{}", ModelStage::Development), "development");
        assert_eq!(format!("{}", ModelStage::Archived), "archived");
    }

    #[test]
    fn test_model_metrics_default() {
        let m = ModelMetrics::default();
        assert_eq!(m.accuracy, 0.0);
        assert_eq!(m.sharpe_ratio, 0.0);
        assert_eq!(m.max_drawdown, 0.0);
        assert_eq!(m.latency_ms, 0.0);
    }

    #[test]
    fn test_prediction_serialization() {
        let pred = Prediction {
            model_id: "test".into(),
            version: "1.0".into(),
            output: HashMap::from([("signal".into(), 0.85)]),
            confidence: 0.92,
            latency_us: 150,
        };
        let json = serde_json::to_string(&pred).unwrap();
        let back: Prediction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model_id, "test");
        assert_eq!(back.latency_us, 150);
    }
}
