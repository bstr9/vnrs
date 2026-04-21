//! Shadow Engine - manages shadow deployments, routes predictions, records comparisons.
//!
//! The ShadowEngine is the central orchestrator for shadow deployment. It manages
//! shadow models, records their predictions without executing trades, and compares
//! predictions against actual market outcomes.
//!
//! # Architecture
//!
//! ```text
//! Market Data
//!     |
//!     v
//! ShadowEngine
//!     |-- ShadowModel (Shadow stage) -> record prediction, NO trade
//!     |-- ShadowModel (Canary stage) -> small allocation trade
//!     |-- ShadowModel (Production stage) -> full trade
//!     |
//!     v
//! PredictionComparison (after market moves)
//!     |
//!     v
//! PromotionPolicy.evaluate() -> Promote / Reject / NeedMoreData
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::{debug, info, warn};

use super::comparison::{ComparisonStore, PredictionComparison, ShadowPrediction};
use super::metrics::{ConfidenceDistribution, RollingMetrics, ShadowMetrics};
use super::promotion::{PromotionDecision, PromotionPolicy};
use crate::trader::Direction;

// ---------------------------------------------------------------------------
// ShadowStage - deployment stage for a shadow model
// ---------------------------------------------------------------------------

/// Deployment stage for a shadow model.
///
/// Models progress through stages: Shadow -> Canary -> Production.
/// - **Shadow**: Model receives data and produces predictions, but NO trades are executed.
/// - **Canary**: Model executes trades with a small allocation.
/// - **Production**: Model executes trades with full allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowStage {
    /// Record predictions, no trading
    Shadow,
    /// Small allocation trading
    Canary,
    /// Full trading
    Production,
}

impl std::fmt::Display for ShadowStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShadowStage::Shadow => write!(f, "shadow"),
            ShadowStage::Canary => write!(f, "canary"),
            ShadowStage::Production => write!(f, "production"),
        }
    }
}

// ---------------------------------------------------------------------------
// ShadowModel - a single shadow-tracked model
// ---------------------------------------------------------------------------

/// A model being tracked in shadow deployment.
///
/// Contains the model's current stage, accumulated predictions, and
/// computed metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowModel {
    /// Model identifier
    pub model_id: String,
    /// Model version
    pub version: String,
    /// Current deployment stage
    pub stage: ShadowStage,
    /// Accumulated predictions
    pub predictions: Vec<ShadowPrediction>,
    /// Last computed metrics
    pub metrics: ShadowMetrics,
    /// VT symbols this model tracks
    pub vt_symbols: Vec<String>,
    /// When the model was added to shadow deployment
    pub added_at: chrono::DateTime<Utc>,
    /// When the model was last updated
    pub updated_at: chrono::DateTime<Utc>,
}

impl ShadowModel {
    /// Create a new shadow model in the Shadow stage.
    pub fn new(model_id: impl Into<String>, version: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            model_id: model_id.into(),
            version: version.into(),
            stage: ShadowStage::Shadow,
            predictions: Vec::new(),
            metrics: ShadowMetrics::default(),
            vt_symbols: Vec::new(),
            added_at: now,
            updated_at: now,
        }
    }

    /// Create a new shadow model with specified stage.
    pub fn with_stage(mut self, stage: ShadowStage) -> Self {
        self.stage = stage;
        self
    }

    /// Add a VT symbol this model tracks.
    pub fn with_vt_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.vt_symbols.push(symbol.into());
        self
    }

    /// Record a prediction.
    pub fn record_prediction(&mut self, prediction: ShadowPrediction) {
        debug!(
            model_id = %self.model_id,
            direction = ?prediction.predicted_direction,
            pnl = prediction.predicted_pnl,
            "Recording shadow prediction"
        );
        self.predictions.push(prediction);
        self.updated_at = Utc::now();
    }

    /// Get the number of predictions.
    pub fn prediction_count(&self) -> usize {
        self.predictions.len()
    }

    /// Check if this model is in shadow mode (no trading).
    pub fn is_shadow(&self) -> bool {
        self.stage == ShadowStage::Shadow
    }

    /// Transition to the next stage.
    ///
    /// Returns an error for invalid transitions.
    pub fn transition_to(&mut self, new_stage: ShadowStage) -> Result<(), String> {
        match (self.stage, new_stage) {
            (ShadowStage::Shadow, ShadowStage::Canary) => {}
            (ShadowStage::Canary, ShadowStage::Production) => {}
            (current, target) => {
                return Err(format!(
                    "Invalid shadow stage transition: {} -> {}. Valid: Shadow->Canary, Canary->Production",
                    current, target
                ));
            }
        }
        info!(model_id = %self.model_id, from = %self.stage, to = %new_stage, "Transitioning shadow model stage");
        self.stage = new_stage;
        self.updated_at = Utc::now();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ShadowEngine - main orchestrator
// ---------------------------------------------------------------------------

/// Central engine for managing shadow deployments.
///
/// Manages shadow models, records predictions, compares against actual outcomes,
/// and evaluates promotion eligibility.
pub struct ShadowEngine {
    /// Shadow models being tracked
    models: RwLock<HashMap<String, ShadowModel>>,
    /// Comparison results
    comparisons: RwLock<ComparisonStore>,
    /// Promotion policy
    promotion_policy: PromotionPolicy,
}

impl ShadowEngine {
    /// Create a new ShadowEngine with the default promotion policy.
    pub fn new() -> Self {
        Self {
            models: RwLock::new(HashMap::new()),
            comparisons: RwLock::new(ComparisonStore::new()),
            promotion_policy: PromotionPolicy::default(),
        }
    }

    /// Create a new ShadowEngine with a custom promotion policy.
    pub fn with_policy(policy: PromotionPolicy) -> Self {
        Self {
            models: RwLock::new(HashMap::new()),
            comparisons: RwLock::new(ComparisonStore::new()),
            promotion_policy: policy,
        }
    }

    // -----------------------------------------------------------------------
    // Model management
    // -----------------------------------------------------------------------

    /// Register a new model for shadow deployment.
    ///
    /// The model starts in the Shadow stage. Returns an error if the model
    /// is already registered.
    pub fn register_model(&self, model_id: &str, version: &str) -> Result<(), String> {
        let key = format!("{}:{}", model_id, version);
        let mut models = self.models.write().map_err(|e| e.to_string())?;

        if models.contains_key(&key) {
            return Err(format!("Shadow model already registered: {}", key));
        }

        let model = ShadowModel::new(model_id, version);
        info!(model_id, version, "Registered shadow model");
        models.insert(key, model);
        Ok(())
    }

    /// Register a model with a specific stage.
    pub fn register_model_with_stage(
        &self,
        model_id: &str,
        version: &str,
        stage: ShadowStage,
    ) -> Result<(), String> {
        let key = format!("{}:{}", model_id, version);
        let mut models = self.models.write().map_err(|e| e.to_string())?;

        if models.contains_key(&key) {
            return Err(format!("Shadow model already registered: {}", key));
        }

        let model = ShadowModel::new(model_id, version).with_stage(stage);
        info!(model_id, version, stage = %stage, "Registered shadow model with stage");
        models.insert(key, model);
        Ok(())
    }

    /// Remove a model from shadow deployment.
    pub fn remove_model(&self, model_id: &str, version: &str) -> Result<(), String> {
        let key = format!("{}:{}", model_id, version);
        let mut models = self.models.write().map_err(|e| e.to_string())?;

        if models.remove(&key).is_none() {
            return Err(format!("Shadow model not found: {}", key));
        }

        info!(model_id, version, "Removed shadow model");
        Ok(())
    }

    /// Get a shadow model by ID and version.
    pub fn get_model(&self, model_id: &str, version: &str) -> Option<ShadowModel> {
        let key = format!("{}:{}", model_id, version);
        let models = self.models.read().ok()?;
        models.get(&key).cloned()
    }

    /// List all shadow models.
    pub fn list_models(&self) -> Vec<ShadowModel> {
        let models = match self.models.read() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        models.values().cloned().collect()
    }

    /// List shadow models by stage.
    pub fn list_models_by_stage(&self, stage: ShadowStage) -> Vec<ShadowModel> {
        let models = match self.models.read() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };
        models
            .values()
            .filter(|m| m.stage == stage)
            .cloned()
            .collect()
    }

    /// Check if a model is registered.
    pub fn model_exists(&self, model_id: &str, version: &str) -> bool {
        let key = format!("{}:{}", model_id, version);
        self.models
            .read()
            .map(|m| m.contains_key(&key))
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Prediction recording
    // -----------------------------------------------------------------------

    /// Record a shadow prediction for a model.
    ///
    /// In Shadow stage, the prediction is recorded but NO trade is executed.
    /// In Canary/Production stages, the prediction is still recorded for
    /// comparison purposes, but trades may also be executed externally.
    pub fn record_prediction(
        &self,
        model_id: &str,
        version: &str,
        prediction: ShadowPrediction,
    ) -> Result<(), String> {
        let key = format!("{}:{}", model_id, version);
        let mut models = self.models.write().map_err(|e| e.to_string())?;

        let model = models
            .get_mut(&key)
            .ok_or_else(|| format!("Shadow model not found: {}", key))?;

        if model.is_shadow() {
            debug!(
                model_id,
                "Shadow model prediction recorded (NO trade executed)"
            );
        } else {
            debug!(
                model_id,
                stage = %model.stage,
                "Non-shadow model prediction recorded"
            );
        }

        model.record_prediction(prediction);
        Ok(())
    }

    /// Quick method to record a simple prediction.
    pub fn predict(
        &self,
        model_id: &str,
        version: &str,
        vt_symbol: &str,
        direction: Direction,
        predicted_pnl: f64,
        confidence: f64,
    ) -> Result<ShadowPrediction, String> {
        let prediction = ShadowPrediction::new(
            model_id,
            version,
            vt_symbol,
            direction,
            predicted_pnl,
            confidence,
        );
        self.record_prediction(model_id, version, prediction.clone())?;
        Ok(prediction)
    }

    // -----------------------------------------------------------------------
    // Comparison / outcome recording
    // -----------------------------------------------------------------------

    /// Record an actual outcome and compare it with a prediction.
    ///
    /// This is the core of shadow deployment: comparing what the model predicted
    /// vs what actually happened.
    pub fn record_outcome(
        &self,
        prediction: ShadowPrediction,
        actual_pnl: f64,
        actual_direction: Direction,
    ) -> PredictionComparison {
        let comparison = PredictionComparison::new(prediction, actual_pnl, actual_direction);

        debug!(
            model_id = %comparison.prediction.model_id,
            direction_correct = comparison.is_direction_correct(),
            pnl_delta = comparison.pnl_delta,
            "Recorded shadow outcome comparison"
        );

        if let Ok(mut comparisons) = self.comparisons.write() {
            comparisons.add(comparison.clone());
        }

        comparison
    }

    /// Record outcomes for all uncommitted predictions of a model.
    ///
    /// Takes a closure that maps from (prediction -> actual_pnl, actual_direction).
    pub fn resolve_predictions<F>(
        &self,
        model_id: &str,
        version: &str,
        resolver: F,
    ) -> Result<Vec<PredictionComparison>, String>
    where
        F: Fn(&ShadowPrediction) -> (f64, Direction),
    {
        let key = format!("{}:{}", model_id, version);
        let models = self.models.read().map_err(|e| e.to_string())?;
        let model = models
            .get(&key)
            .ok_or_else(|| format!("Shadow model not found: {}", key))?;

        let mut results = Vec::new();
        let mut comparisons = self.comparisons.write().map_err(|e| e.to_string())?;

        for prediction in &model.predictions {
            let (actual_pnl, actual_direction) = resolver(prediction);
            let comparison = PredictionComparison::new(
                prediction.clone(),
                actual_pnl,
                actual_direction,
            );
            comparisons.add(comparison.clone());
            results.push(comparison);
        }

        Ok(results)
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    /// Compute and update metrics for a model.
    pub fn update_metrics(&self, model_id: &str, version: &str) -> Result<ShadowMetrics, String> {
        let comparisons = self.comparisons.read().map_err(|e| e.to_string())?;
        let model_comparisons = comparisons.by_model(model_id);

        let metrics = ShadowMetrics::from_comparisons(
            &model_comparisons
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        );

        // Update model metrics
        let key = format!("{}:{}", model_id, version);
        drop(comparisons);
        let mut models = self.models.write().map_err(|e| e.to_string())?;
        if let Some(model) = models.get_mut(&key) {
            model.metrics = metrics.clone();
        }

        Ok(metrics)
    }

    /// Get rolling metrics for a model.
    pub fn rolling_metrics(
        &self,
        model_id: &str,
        window_size: usize,
    ) -> Result<RollingMetrics, String> {
        let comparisons = self.comparisons.read().map_err(|e| e.to_string())?;
        let model_comparisons: Vec<_> = comparisons
            .by_model(model_id)
            .into_iter()
            .cloned()
            .collect();
        Ok(RollingMetrics::from_comparisons(&model_comparisons, window_size))
    }

    /// Get confidence distribution for a model.
    pub fn confidence_distribution(
        &self,
        model_id: &str,
    ) -> Result<ConfidenceDistribution, String> {
        let comparisons = self.comparisons.read().map_err(|e| e.to_string())?;
        let model_comparisons: Vec<_> = comparisons
            .by_model(model_id)
            .into_iter()
            .cloned()
            .collect();
        Ok(ConfidenceDistribution::from_comparisons(&model_comparisons))
    }

    /// Get all comparisons for a model.
    pub fn get_comparisons(&self, model_id: &str) -> Vec<PredictionComparison> {
        let comparisons = match self.comparisons.read() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        comparisons
            .by_model(model_id)
            .into_iter()
            .cloned()
            .collect()
    }

    // -----------------------------------------------------------------------
    // Promotion
    // -----------------------------------------------------------------------

    /// Evaluate promotion eligibility for a model.
    ///
    /// Uses the configured promotion policy to determine whether the model
    /// should be promoted to the next stage.
    pub fn evaluate_promotion(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<PromotionDecision, String> {
        let key = format!("{}:{}", model_id, version);

        // Get current metrics
        let metrics = {
            let models = self.models.read().map_err(|e| e.to_string())?;
            let model = models
                .get(&key)
                .ok_or_else(|| format!("Shadow model not found: {}", key))?;
            model.metrics.clone()
        };

        // If metrics are stale (no comparisons), try to update
        if metrics.prediction_count == 0 {
            drop(metrics);
            let updated = self.update_metrics(model_id, version)?;
            Ok(self.promotion_policy.evaluate(&updated))
        } else {
            Ok(self.promotion_policy.evaluate(&metrics))
        }
    }

    /// Promote a model to the next stage if eligible.
    ///
    /// Returns the promotion decision. If promotion is approved, the model's
    /// stage is updated.
    pub fn try_promote(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<PromotionDecision, String> {
        let decision = self.evaluate_promotion(model_id, version)?;

        if decision.is_promote() {
            let key = format!("{}:{}", model_id, version);
            let mut models = self.models.write().map_err(|e| e.to_string())?;
            if let Some(model) = models.get_mut(&key) {
                let next_stage = match model.stage {
                    ShadowStage::Shadow => ShadowStage::Canary,
                    ShadowStage::Canary => ShadowStage::Production,
                    ShadowStage::Production => {
                        warn!(model_id, "Model already in Production, cannot promote further");
                        return Ok(decision);
                    }
                };
                model.transition_to(next_stage)?;
            }
        }

        Ok(decision)
    }

    /// Get the current promotion policy.
    pub fn promotion_policy(&self) -> &PromotionPolicy {
        &self.promotion_policy
    }

    // -----------------------------------------------------------------------
    // Utility
    // -----------------------------------------------------------------------

    /// Get the total number of shadow models.
    pub fn model_count(&self) -> usize {
        self.models
            .read()
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Get the total number of comparisons.
    pub fn comparison_count(&self) -> usize {
        self.comparisons
            .read()
            .map(|c| c.len())
            .unwrap_or(0)
    }

    /// Clear all comparisons.
    pub fn clear_comparisons(&self) {
        if let Ok(mut comparisons) = self.comparisons.write() {
            comparisons.clear();
        }
    }
}

impl Default for ShadowEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shadow_stage_display() {
        assert_eq!(format!("{}", ShadowStage::Shadow), "shadow");
        assert_eq!(format!("{}", ShadowStage::Canary), "canary");
        assert_eq!(format!("{}", ShadowStage::Production), "production");
    }

    #[test]
    fn test_shadow_model_creation() {
        let model = ShadowModel::new("test_model", "1.0.0");
        assert_eq!(model.model_id, "test_model");
        assert_eq!(model.version, "1.0.0");
        assert_eq!(model.stage, ShadowStage::Shadow);
        assert!(model.is_shadow());
        assert_eq!(model.prediction_count(), 0);
    }

    #[test]
    fn test_shadow_model_with_stage() {
        let model = ShadowModel::new("test_model", "1.0.0").with_stage(ShadowStage::Canary);
        assert_eq!(model.stage, ShadowStage::Canary);
        assert!(!model.is_shadow());
    }

    #[test]
    fn test_shadow_model_transition_valid() {
        let mut model = ShadowModel::new("test_model", "1.0.0");
        assert!(model.transition_to(ShadowStage::Canary).is_ok());
        assert_eq!(model.stage, ShadowStage::Canary);
        assert!(model.transition_to(ShadowStage::Production).is_ok());
        assert_eq!(model.stage, ShadowStage::Production);
    }

    #[test]
    fn test_shadow_model_transition_invalid() {
        let mut model = ShadowModel::new("test_model", "1.0.0");
        assert!(model.transition_to(ShadowStage::Production).is_err());
        assert!(model.transition_to(ShadowStage::Shadow).is_err());
    }

    #[test]
    fn test_shadow_model_record_prediction() {
        let mut model = ShadowModel::new("test_model", "1.0.0");
        let pred = ShadowPrediction::new(
            "test_model",
            "1.0.0",
            "BTCUSDT.BINANCE",
            Direction::Long,
            100.0,
            0.8,
        );
        model.record_prediction(pred);
        assert_eq!(model.prediction_count(), 1);
    }

    #[test]
    fn test_shadow_engine_register_model() {
        let engine = ShadowEngine::new();
        assert!(engine.register_model("model_a", "1.0.0").is_ok());
        assert!(engine.model_exists("model_a", "1.0.0"));
        assert_eq!(engine.model_count(), 1);
    }

    #[test]
    fn test_shadow_engine_register_duplicate() {
        let engine = ShadowEngine::new();
        assert!(engine.register_model("model_a", "1.0.0").is_ok());
        let err = engine.register_model("model_a", "1.0.0").unwrap_err();
        assert!(err.contains("already registered"));
    }

    #[test]
    fn test_shadow_engine_remove_model() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();
        assert!(engine.remove_model("model_a", "1.0.0").is_ok());
        assert!(!engine.model_exists("model_a", "1.0.0"));
        assert!(engine.remove_model("model_a", "1.0.0").is_err());
    }

    #[test]
    fn test_shadow_engine_record_prediction() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        let pred = engine
            .predict(
                "model_a",
                "1.0.0",
                "BTCUSDT.BINANCE",
                Direction::Long,
                100.0,
                0.85,
            )
            .unwrap();

        assert_eq!(pred.model_id, "model_a");
        let model = engine.get_model("model_a", "1.0.0").unwrap();
        assert_eq!(model.prediction_count(), 1);
    }

    #[test]
    fn test_shadow_engine_record_prediction_missing_model() {
        let engine = ShadowEngine::new();
        let result = engine.predict(
            "nonexistent",
            "1.0.0",
            "BTCUSDT.BINANCE",
            Direction::Long,
            100.0,
            0.85,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_shadow_engine_record_outcome() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        let pred = engine
            .predict(
                "model_a",
                "1.0.0",
                "BTCUSDT.BINANCE",
                Direction::Long,
                100.0,
                0.85,
            )
            .unwrap();

        let comparison = engine.record_outcome(pred, 80.0, Direction::Long);
        assert!(comparison.is_direction_correct());
        assert!((comparison.actual_pnl - 80.0).abs() < f64::EPSILON);
        assert_eq!(engine.comparison_count(), 1);
    }

    #[test]
    fn test_shadow_engine_record_outcome_wrong_direction() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        let pred = engine
            .predict(
                "model_a",
                "1.0.0",
                "BTCUSDT.BINANCE",
                Direction::Long,
                100.0,
                0.85,
            )
            .unwrap();

        let comparison = engine.record_outcome(pred, -50.0, Direction::Short);
        assert!(!comparison.is_direction_correct());
    }

    #[test]
    fn test_shadow_engine_list_models_by_stage() {
        let engine = ShadowEngine::new();
        engine.register_model("shadow_model", "1.0.0").unwrap();
        engine
            .register_model_with_stage("canary_model", "1.0.0", ShadowStage::Canary)
            .unwrap();

        let shadow_models = engine.list_models_by_stage(ShadowStage::Shadow);
        let canary_models = engine.list_models_by_stage(ShadowStage::Canary);

        assert_eq!(shadow_models.len(), 1);
        assert_eq!(canary_models.len(), 1);
    }

    #[test]
    fn test_shadow_engine_update_metrics() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        // Record multiple predictions and outcomes
        for i in 0..5 {
            let pred = engine
                .predict(
                    "model_a",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    0.8,
                )
                .unwrap();
            let actual_dir = if i < 3 { Direction::Long } else { Direction::Short };
            engine.record_outcome(pred, 50.0, actual_dir);
        }

        let metrics = engine.update_metrics("model_a", "1.0.0").unwrap();
        assert_eq!(metrics.prediction_count, 5);
        // 3 out of 5 correct direction
        assert!((metrics.direction_accuracy - 0.6).abs() < 1e-9);
    }

    #[test]
    fn test_shadow_engine_evaluate_promotion_need_more_data() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        // Only a few predictions
        let pred = engine
            .predict(
                "model_a",
                "1.0.0",
                "BTCUSDT.BINANCE",
                Direction::Long,
                100.0,
                0.8,
            )
            .unwrap();
        engine.record_outcome(pred, 80.0, Direction::Long);

        let decision = engine.evaluate_promotion("model_a", "1.0.0").unwrap();
        assert!(decision.needs_more_data());
    }

    #[test]
    fn test_shadow_engine_try_promote_success() {
        let policy = PromotionPolicy::lenient().with_min_predictions(2);
        let engine = ShadowEngine::with_policy(policy);
        engine.register_model("model_a", "1.0.0").unwrap();

        // Record enough successful predictions
        for _ in 0..3 {
            let pred = engine
                .predict(
                    "model_a",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    0.8,
                )
                .unwrap();
            engine.record_outcome(pred, 80.0, Direction::Long);
        }

        engine.update_metrics("model_a", "1.0.0").unwrap();
        let decision = engine.try_promote("model_a", "1.0.0").unwrap();
        assert!(decision.is_promote());

        let model = engine.get_model("model_a", "1.0.0").unwrap();
        assert_eq!(model.stage, ShadowStage::Canary);
    }

    #[test]
    fn test_shadow_engine_try_promote_reject() {
        let policy = PromotionPolicy::lenient()
            .with_min_predictions(2)
            .with_min_accuracy(0.9);
        let engine = ShadowEngine::with_policy(policy);
        engine.register_model("model_a", "1.0.0").unwrap();

        // Record predictions with mixed outcomes
        for i in 0..3 {
            let pred = engine
                .predict(
                    "model_a",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    0.8,
                )
                .unwrap();
            let actual_dir = if i < 2 { Direction::Long } else { Direction::Short };
            engine.record_outcome(pred, 50.0, actual_dir);
        }

        engine.update_metrics("model_a", "1.0.0").unwrap();
        let decision = engine.try_promote("model_a", "1.0.0").unwrap();
        assert!(decision.is_reject());
    }

    #[test]
    fn test_shadow_engine_rolling_metrics() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        for i in 0..10 {
            let pred = engine
                .predict(
                    "model_a",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    0.8,
                )
                .unwrap();
            let actual_dir = if i < 5 { Direction::Long } else { Direction::Short };
            engine.record_outcome(pred, 50.0, actual_dir);
        }

        let rolling = engine.rolling_metrics("model_a", 5).unwrap();
        assert_eq!(rolling.window_size, 5);
        // Last 5: all wrong direction -> accuracy = 0.0
        assert!((rolling.direction_accuracy - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_shadow_engine_confidence_distribution() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        for i in 0..6 {
            let conf = if i < 2 { 0.2 } else { 0.8 };
            let pred = ShadowPrediction::new(
                "model_a",
                "1.0.0",
                "BTCUSDT.BINANCE",
                Direction::Long,
                100.0,
                conf,
            );
            engine
                .record_prediction("model_a", "1.0.0", pred)
                .unwrap();
            engine.record_outcome(
                ShadowPrediction::new(
                    "model_a",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    conf,
                ),
                80.0,
                Direction::Long,
            );
        }

        let dist = engine.confidence_distribution("model_a").unwrap();
        assert_eq!(dist.low_confidence, 2);
        assert_eq!(dist.high_confidence, 4);
    }

    #[test]
    fn test_shadow_engine_get_comparisons() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        for _ in 0..3 {
            let pred = engine
                .predict(
                    "model_a",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    0.8,
                )
                .unwrap();
            engine.record_outcome(pred, 80.0, Direction::Long);
        }

        let comparisons = engine.get_comparisons("model_a");
        assert_eq!(comparisons.len(), 3);
    }

    #[test]
    fn test_shadow_engine_clear_comparisons() {
        let engine = ShadowEngine::new();
        engine.register_model("model_a", "1.0.0").unwrap();

        let pred = engine
            .predict(
                "model_a",
                "1.0.0",
                "BTCUSDT.BINANCE",
                Direction::Long,
                100.0,
                0.8,
            )
            .unwrap();
        engine.record_outcome(pred, 80.0, Direction::Long);
        assert_eq!(engine.comparison_count(), 1);

        engine.clear_comparisons();
        assert_eq!(engine.comparison_count(), 0);
    }

    #[test]
    fn test_shadow_engine_full_lifecycle() {
        let policy = PromotionPolicy::lenient().with_min_predictions(3);
        let engine = ShadowEngine::with_policy(policy);

        // 1. Register model
        engine.register_model("lifecycle_model", "1.0.0").unwrap();
        assert_eq!(engine.get_model("lifecycle_model", "1.0.0").unwrap().stage, ShadowStage::Shadow);

        // 2. Record predictions (shadow mode - no trades)
        for i in 0..5 {
            let pred = engine
                .predict(
                    "lifecycle_model",
                    "1.0.0",
                    "BTCUSDT.BINANCE",
                    Direction::Long,
                    100.0,
                    0.8,
                )
                .unwrap();

            // Simulate actual outcome
            let actual_dir = if i < 4 { Direction::Long } else { Direction::Short };
            engine.record_outcome(pred, 80.0, actual_dir);
        }

        // 3. Update metrics
        let metrics = engine.update_metrics("lifecycle_model", "1.0.0").unwrap();
        assert_eq!(metrics.prediction_count, 5);
        assert!((metrics.direction_accuracy - 0.8).abs() < 1e-9);

        // 4. Try promotion: Shadow -> Canary
        let decision = engine.try_promote("lifecycle_model", "1.0.0").unwrap();
        assert!(decision.is_promote());
        assert_eq!(
            engine.get_model("lifecycle_model", "1.0.0").unwrap().stage,
            ShadowStage::Canary
        );

        // 5. Try promotion: Canary -> Production
        let decision = engine.try_promote("lifecycle_model", "1.0.0").unwrap();
        assert!(decision.is_promote());
        assert_eq!(
            engine.get_model("lifecycle_model", "1.0.0").unwrap().stage,
            ShadowStage::Production
        );

        // 6. Cannot promote further
        let decision = engine.try_promote("lifecycle_model", "1.0.0").unwrap();
        assert!(decision.is_promote()); // still promote-eligible but stage unchanged
        assert_eq!(
            engine.get_model("lifecycle_model", "1.0.0").unwrap().stage,
            ShadowStage::Production
        );
    }
}
