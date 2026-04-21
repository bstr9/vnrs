//! Prediction Comparison — compares shadow predictions vs actual outcomes.
//!
//! This module provides the core data structures for tracking how well
//! a shadow model's predictions matched actual market outcomes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::trader::Direction;

// ---------------------------------------------------------------------------
// ShadowPrediction — a single prediction record
// ---------------------------------------------------------------------------

/// A single shadow prediction record.
///
/// Captures the model's prediction at a point in time, including the input
/// features, predicted action, expected PnL, and confidence level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowPrediction {
    /// Unique identifier for this prediction
    pub id: String,
    /// When the prediction was made (UTC)
    pub timestamp: DateTime<Utc>,
    /// Model that produced this prediction
    pub model_id: String,
    /// Model version
    pub version: String,
    /// Feature values used for prediction
    pub input_features: HashMap<String, f64>,
    /// Predicted direction (Long/Short)
    pub predicted_direction: Direction,
    /// Predicted PnL (positive = profit, negative = loss)
    pub predicted_pnl: f64,
    /// Confidence level [0.0, 1.0]
    pub confidence: f64,
    /// VT symbol this prediction is for
    pub vt_symbol: String,
    /// Optional: actual price at prediction time
    pub entry_price: Option<f64>,
    /// Optional: target price for the trade
    pub target_price: Option<f64>,
}

impl ShadowPrediction {
    /// Create a new shadow prediction.
    pub fn new(
        model_id: impl Into<String>,
        version: impl Into<String>,
        vt_symbol: impl Into<String>,
        predicted_direction: Direction,
        predicted_pnl: f64,
        confidence: f64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            model_id: model_id.into(),
            version: version.into(),
            input_features: HashMap::new(),
            predicted_direction,
            predicted_pnl,
            confidence,
            vt_symbol: vt_symbol.into(),
            entry_price: None,
            target_price: None,
        }
    }

    /// Add an input feature to the prediction.
    pub fn with_feature(mut self, name: impl Into<String>, value: f64) -> Self {
        self.input_features.insert(name.into(), value);
        self
    }

    /// Set the entry price.
    pub fn with_entry_price(mut self, price: f64) -> Self {
        self.entry_price = Some(price);
        self
    }

    /// Set the target price.
    pub fn with_target_price(mut self, price: f64) -> Self {
        self.target_price = Some(price);
        self
    }
}

// ---------------------------------------------------------------------------
// PredictionComparison — prediction vs actual outcome
// ---------------------------------------------------------------------------

/// Comparison between a shadow prediction and actual market outcome.
///
/// This is the core data structure for evaluating shadow model performance.
/// It records what the model predicted vs what actually happened, enabling
/// calculation of accuracy, PnL delta, and other metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionComparison {
    /// The original prediction
    pub prediction: ShadowPrediction,
    /// When the actual outcome was observed (UTC)
    pub outcome_timestamp: DateTime<Utc>,
    /// Actual PnL that would have been realized
    pub actual_pnl: f64,
    /// Actual direction the market moved
    pub actual_direction: Direction,
    /// Whether the predicted direction was correct (1.0 = correct, 0.0 = incorrect)
    pub direction_correct: f64,
    /// Difference between predicted and actual PnL (predicted - actual)
    pub pnl_delta: f64,
    /// Absolute error in PnL prediction
    pub pnl_error: f64,
}

impl PredictionComparison {
    /// Create a new comparison between a prediction and actual outcome.
    pub fn new(
        prediction: ShadowPrediction,
        actual_pnl: f64,
        actual_direction: Direction,
    ) -> Self {
        let direction_correct = if prediction.predicted_direction == actual_direction {
            1.0
        } else {
            0.0
        };
        let pnl_delta = prediction.predicted_pnl - actual_pnl;
        let pnl_error = pnl_delta.abs();

        Self {
            prediction,
            outcome_timestamp: Utc::now(),
            actual_pnl,
            actual_direction,
            direction_correct,
            pnl_delta,
            pnl_error,
        }
    }

    /// Check if the direction prediction was correct.
    pub fn is_direction_correct(&self) -> bool {
        self.direction_correct > 0.5
    }

    /// Check if the prediction was profitable (actual PnL > 0).
    pub fn was_profitable(&self) -> bool {
        self.actual_pnl > 0.0
    }

    /// Check if the prediction was within a certain PnL error tolerance.
    pub fn within_pnl_tolerance(&self, tolerance: f64) -> bool {
        self.pnl_error <= tolerance
    }
}

// ---------------------------------------------------------------------------
// ComparisonStore — storage for comparisons
// ---------------------------------------------------------------------------

/// In-memory store for prediction comparisons.
///
/// Provides efficient access to comparisons by model ID, with support
/// for filtering by time range and limiting results.
#[derive(Debug, Clone, Default)]
pub struct ComparisonStore {
    comparisons: Vec<PredictionComparison>,
}

impl ComparisonStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            comparisons: Vec::new(),
        }
    }

    /// Add a comparison to the store.
    pub fn add(&mut self, comparison: PredictionComparison) {
        self.comparisons.push(comparison);
    }

    /// Get all comparisons for a specific model.
    pub fn by_model(&self, model_id: &str) -> Vec<&PredictionComparison> {
        self.comparisons
            .iter()
            .filter(|c| c.prediction.model_id == model_id)
            .collect()
    }

    /// Get all comparisons for a specific model and version.
    pub fn by_model_version(&self, model_id: &str, version: &str) -> Vec<&PredictionComparison> {
        self.comparisons
            .iter()
            .filter(|c| c.prediction.model_id == model_id && c.prediction.version == version)
            .collect()
    }

    /// Get the most recent N comparisons for a model.
    pub fn recent(&self, model_id: &str, n: usize) -> Vec<&PredictionComparison> {
        let mut model_comparisons: Vec<_> = self
            .comparisons
            .iter()
            .filter(|c| c.prediction.model_id == model_id)
            .collect();
        // Sort by outcome timestamp descending
        model_comparisons.sort_by(|a, b| b.outcome_timestamp.cmp(&a.outcome_timestamp));
        model_comparisons.into_iter().take(n).collect()
    }

    /// Get all comparisons.
    pub fn all(&self) -> &[PredictionComparison] {
        &self.comparisons
    }

    /// Clear all comparisons.
    pub fn clear(&mut self) {
        self.comparisons.clear();
    }

    /// Get the total number of comparisons.
    pub fn len(&self) -> usize {
        self.comparisons.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.comparisons.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_prediction(model_id: &str, direction: Direction, pnl: f64) -> ShadowPrediction {
        ShadowPrediction::new(model_id, "1.0.0", "BTCUSDT.BINANCE", direction, pnl, 0.8)
    }

    #[test]
    fn test_shadow_prediction_creation() {
        let pred = ShadowPrediction::new(
            "model_a",
            "1.0.0",
            "BTCUSDT.BINANCE",
            Direction::Long,
            100.0,
            0.85,
        );
        assert_eq!(pred.model_id, "model_a");
        assert_eq!(pred.version, "1.0.0");
        assert_eq!(pred.predicted_direction, Direction::Long);
        assert!((pred.predicted_pnl - 100.0).abs() < f64::EPSILON);
        assert!((pred.confidence - 0.85).abs() < f64::EPSILON);
        assert!(!pred.id.is_empty());
    }

    #[test]
    fn test_shadow_prediction_with_features() {
        let pred = ShadowPrediction::new(
            "model_a",
            "1.0.0",
            "BTCUSDT.BINANCE",
            Direction::Long,
            100.0,
            0.85,
        )
        .with_feature("rsi", 70.0)
        .with_feature("macd", 0.5)
        .with_entry_price(50000.0);

        assert_eq!(pred.input_features.len(), 2);
        assert!((pred.input_features.get("rsi").copied().unwrap_or(0.0) - 70.0).abs() < f64::EPSILON);
        assert_eq!(pred.entry_price, Some(50000.0));
    }

    #[test]
    fn test_comparison_direction_correct() {
        let pred = make_prediction("model_a", Direction::Long, 100.0);
        let comp = PredictionComparison::new(pred, 80.0, Direction::Long);
        assert!(comp.is_direction_correct());
        assert!((comp.direction_correct - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_comparison_direction_incorrect() {
        let pred = make_prediction("model_a", Direction::Long, 100.0);
        let comp = PredictionComparison::new(pred, -50.0, Direction::Short);
        assert!(!comp.is_direction_correct());
        assert!((comp.direction_correct - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_comparison_pnl_calculations() {
        let pred = make_prediction("model_a", Direction::Long, 100.0);
        let comp = PredictionComparison::new(pred, 80.0, Direction::Long);

        // predicted_pnl - actual_pnl
        assert!((comp.pnl_delta - 20.0).abs() < f64::EPSILON);
        // |pnl_delta|
        assert!((comp.pnl_error - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_comparison_profitable() {
        let pred = make_prediction("model_a", Direction::Long, 100.0);
        let comp = PredictionComparison::new(pred, 80.0, Direction::Long);
        assert!(comp.was_profitable());

        let comp2 = PredictionComparison::new(
            make_prediction("model_a", Direction::Long, 100.0),
            -20.0,
            Direction::Short,
        );
        assert!(!comp2.was_profitable());
    }

    #[test]
    fn test_comparison_within_tolerance() {
        let pred = make_prediction("model_a", Direction::Long, 100.0);
        let comp = PredictionComparison::new(pred, 95.0, Direction::Long);

        assert!(comp.within_pnl_tolerance(10.0)); // error is 5
        assert!(!comp.within_pnl_tolerance(3.0)); // error is 5
    }

    #[test]
    fn test_comparison_store_add_and_get() {
        let mut store = ComparisonStore::new();
        let pred1 = make_prediction("model_a", Direction::Long, 100.0);
        let pred2 = make_prediction("model_b", Direction::Short, 50.0);

        store.add(PredictionComparison::new(pred1, 80.0, Direction::Long));
        store.add(PredictionComparison::new(pred2, 30.0, Direction::Short));

        assert_eq!(store.len(), 2);
        assert_eq!(store.by_model("model_a").len(), 1);
        assert_eq!(store.by_model("model_b").len(), 1);
    }

    #[test]
    fn test_comparison_store_by_model_version() {
        let mut store = ComparisonStore::new();
        let pred1 = make_prediction("model_a", Direction::Long, 100.0);
        let mut pred2 = make_prediction("model_a", Direction::Short, 50.0);
        pred2.version = "2.0.0".to_string();

        store.add(PredictionComparison::new(pred1, 80.0, Direction::Long));
        store.add(PredictionComparison::new(pred2, 30.0, Direction::Short));

        assert_eq!(store.by_model_version("model_a", "1.0.0").len(), 1);
        assert_eq!(store.by_model_version("model_a", "2.0.0").len(), 1);
        assert_eq!(store.by_model_version("model_a", "3.0.0").len(), 0);
    }

    #[test]
    fn test_comparison_store_clear() {
        let mut store = ComparisonStore::new();
        let pred = make_prediction("model_a", Direction::Long, 100.0);
        store.add(PredictionComparison::new(pred, 80.0, Direction::Long));

        assert_eq!(store.len(), 1);
        store.clear();
        assert!(store.is_empty());
    }
}
