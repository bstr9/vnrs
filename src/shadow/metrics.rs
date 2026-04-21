//! Shadow Metrics — performance metrics for shadow model evaluation.
//!
//! Provides comprehensive metrics for evaluating shadow model performance,
//! including direction accuracy, virtual Sharpe ratio, max drawdown, and
//! rolling metrics for the last N predictions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::comparison::PredictionComparison;

// ---------------------------------------------------------------------------
// ShadowMetrics — core performance metrics
// ---------------------------------------------------------------------------

/// Performance metrics for a shadow model.
///
/// These metrics are computed from prediction comparisons and represent
/// the virtual performance "as if" the model's predictions had been traded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowMetrics {
    /// Total number of predictions made
    pub prediction_count: usize,
    /// Direction accuracy (0.0 to 1.0)
    pub direction_accuracy: f64,
    /// Virtual Sharpe ratio (based on predicted PnLs)
    pub virtual_sharpe: f64,
    /// Maximum virtual drawdown (negative value, e.g., -0.15 for 15%)
    pub max_virtual_drawdown: f64,
    /// Total virtual PnL (sum of actual PnLs)
    pub total_virtual_pnl: f64,
    /// Average virtual PnL per prediction
    pub avg_virtual_pnl: f64,
    /// Win rate (percentage of profitable predictions)
    pub win_rate: f64,
    /// Average confidence of predictions
    pub avg_confidence: f64,
    /// Timestamp when metrics were computed
    pub computed_at: DateTime<Utc>,
}

impl Default for ShadowMetrics {
    fn default() -> Self {
        Self {
            prediction_count: 0,
            direction_accuracy: 0.0,
            virtual_sharpe: 0.0,
            max_virtual_drawdown: 0.0,
            total_virtual_pnl: 0.0,
            avg_virtual_pnl: 0.0,
            win_rate: 0.0,
            avg_confidence: 0.0,
            computed_at: Utc::now(),
        }
    }
}

impl ShadowMetrics {
    /// Create a new empty metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute metrics from a list of comparisons.
    pub fn from_comparisons(comparisons: &[PredictionComparison]) -> Self {
        if comparisons.is_empty() {
            return Self::new();
        }

        let prediction_count = comparisons.len();

        // Direction accuracy
        let direction_correct_count: usize = comparisons
            .iter()
            .map(|c| if c.is_direction_correct() { 1 } else { 0 })
            .sum();
        let direction_accuracy = direction_correct_count as f64 / prediction_count as f64;

        // Virtual PnLs
        let virtual_pnls: Vec<f64> = comparisons.iter().map(|c| c.actual_pnl).collect();
        let total_virtual_pnl: f64 = virtual_pnls.iter().sum();
        let avg_virtual_pnl = total_virtual_pnl / prediction_count as f64;

        // Win rate
        let wins: usize = comparisons
            .iter()
            .map(|c| if c.was_profitable() { 1 } else { 0 })
            .sum();
        let win_rate = wins as f64 / prediction_count as f64;

        // Average confidence
        let total_confidence: f64 = comparisons.iter().map(|c| c.prediction.confidence).sum();
        let avg_confidence = total_confidence / prediction_count as f64;

        // Virtual Sharpe ratio
        let virtual_sharpe = Self::compute_sharpe(&virtual_pnls);

        // Max virtual drawdown
        let max_virtual_drawdown = Self::compute_max_drawdown(&virtual_pnls);

        Self {
            prediction_count,
            direction_accuracy,
            virtual_sharpe,
            max_virtual_drawdown,
            total_virtual_pnl,
            avg_virtual_pnl,
            win_rate,
            avg_confidence,
            computed_at: Utc::now(),
        }
    }

    /// Compute Sharpe ratio from a series of PnLs.
    ///
    /// Uses a simplified calculation: mean(pnls) / std(pnls)
    /// This assumes PnLs are already returns/differences, not raw prices.
    fn compute_sharpe(pnls: &[f64]) -> f64 {
        if pnls.is_empty() {
            return 0.0;
        }

        let n = pnls.len() as f64;
        let mean: f64 = pnls.iter().sum::<f64>() / n;

        if pnls.len() < 2 {
            return 0.0;
        }

        let variance: f64 = pnls.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std = variance.sqrt();

        if std < 1e-10 {
            return 0.0;
        }

        // Annualize assuming daily returns (multiply by sqrt(252))
        // For simplicity, we return the raw ratio
        mean / std
    }

    /// Compute maximum drawdown from a series of PnLs.
    ///
    /// Returns the maximum peak-to-trough decline as a negative value.
    fn compute_max_drawdown(pnls: &[f64]) -> f64 {
        if pnls.is_empty() {
            return 0.0;
        }

        // Compute cumulative PnL curve
        let mut cumulative = 0.0;
        let mut peak = 0.0;
        let mut max_dd = 0.0;

        for pnl in pnls {
            cumulative += pnl;
            if cumulative > peak {
                peak = cumulative;
            }
            let dd = cumulative - peak;
            if dd < max_dd {
                max_dd = dd;
            }
        }

        max_dd
    }

    /// Check if metrics meet minimum thresholds.
    pub fn meets_thresholds(
        &self,
        min_accuracy: f64,
        min_sharpe: f64,
        max_drawdown: f64,
    ) -> bool {
        self.direction_accuracy >= min_accuracy
            && self.virtual_sharpe >= min_sharpe
            && self.max_virtual_drawdown >= max_drawdown // max_drawdown is negative, so >= means smaller drawdown
    }
}

// ---------------------------------------------------------------------------
// RollingMetrics — metrics for the last N predictions
// ---------------------------------------------------------------------------

/// Rolling metrics computed over the last N predictions.
///
/// Useful for detecting performance degradation or improvement over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollingMetrics {
    /// Window size (number of predictions)
    pub window_size: usize,
    /// Direction accuracy for the window
    pub direction_accuracy: f64,
    /// Virtual Sharpe for the window
    pub virtual_sharpe: f64,
    /// Win rate for the window
    pub win_rate: f64,
    /// Average virtual PnL for the window
    pub avg_virtual_pnl: f64,
    /// Timestamp when metrics were computed
    pub computed_at: DateTime<Utc>,
}

impl RollingMetrics {
    /// Compute rolling metrics from comparisons.
    ///
    /// Uses the last `window_size` comparisons, or all if fewer are available.
    pub fn from_comparisons(comparisons: &[PredictionComparison], window_size: usize) -> Self {
        let start = if comparisons.len() > window_size {
            comparisons.len() - window_size
        } else {
            0
        };
        let window = &comparisons[start..];

        if window.is_empty() {
            return Self {
                window_size,
                direction_accuracy: 0.0,
                virtual_sharpe: 0.0,
                win_rate: 0.0,
                avg_virtual_pnl: 0.0,
                computed_at: Utc::now(),
            };
        }

        let actual_window_size = window.len();
        let pnls: Vec<f64> = window.iter().map(|c| c.actual_pnl).collect();

        let direction_correct: usize = window
            .iter()
            .map(|c| if c.is_direction_correct() { 1 } else { 0 })
            .sum();
        let direction_accuracy = direction_correct as f64 / actual_window_size as f64;

        let wins: usize = window
            .iter()
            .map(|c| if c.was_profitable() { 1 } else { 0 })
            .sum();
        let win_rate = wins as f64 / actual_window_size as f64;

        let avg_virtual_pnl = pnls.iter().sum::<f64>() / actual_window_size as f64;
        let virtual_sharpe = ShadowMetrics::compute_sharpe(&pnls);

        Self {
            window_size: actual_window_size,
            direction_accuracy,
            virtual_sharpe,
            win_rate,
            avg_virtual_pnl,
            computed_at: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// ConfidenceDistribution — distribution of confidence levels
// ---------------------------------------------------------------------------

/// Distribution of confidence levels across predictions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceDistribution {
    /// Number of predictions with confidence < 0.3
    pub low_confidence: usize,
    /// Number of predictions with 0.3 <= confidence < 0.6
    pub medium_confidence: usize,
    /// Number of predictions with confidence >= 0.6
    pub high_confidence: usize,
    /// Total predictions
    pub total: usize,
}

impl ConfidenceDistribution {
    /// Compute confidence distribution from comparisons.
    pub fn from_comparisons(comparisons: &[PredictionComparison]) -> Self {
        let mut low = 0;
        let mut medium = 0;
        let mut high = 0;

        for c in comparisons {
            let conf = c.prediction.confidence;
            if conf < 0.3 {
                low += 1;
            } else if conf < 0.6 {
                medium += 1;
            } else {
                high += 1;
            }
        }

        Self {
            low_confidence: low,
            medium_confidence: medium,
            high_confidence: high,
            total: comparisons.len(),
        }
    }

    /// Get the percentage of high-confidence predictions.
    pub fn high_confidence_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        self.high_confidence as f64 / self.total as f64
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shadow::comparison::ShadowPrediction;
    use crate::trader::Direction;

    fn make_comparison(model_id: &str, predicted_pnl: f64, actual_pnl: f64, direction_correct: bool) -> PredictionComparison {
        let pred_dir = Direction::Long;
        let actual_dir = if direction_correct { pred_dir } else { Direction::Short };
        let pred = ShadowPrediction::new(model_id, "1.0.0", "BTCUSDT.BINANCE", pred_dir, predicted_pnl, 0.8);
        PredictionComparison::new(pred, actual_pnl, actual_dir)
    }

    #[test]
    fn test_shadow_metrics_default() {
        let m = ShadowMetrics::default();
        assert_eq!(m.prediction_count, 0);
        assert!((m.direction_accuracy - 0.0).abs() < f64::EPSILON);
        assert!((m.virtual_sharpe - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_shadow_metrics_from_empty_comparisons() {
        let comparisons: Vec<PredictionComparison> = Vec::new();
        let m = ShadowMetrics::from_comparisons(&comparisons);
        assert_eq!(m.prediction_count, 0);
    }

    #[test]
    fn test_shadow_metrics_direction_accuracy() {
        let comparisons = vec![
            make_comparison("m", 100.0, 80.0, true),  // correct
            make_comparison("m", 100.0, 80.0, true),  // correct
            make_comparison("m", 100.0, -20.0, false), // incorrect
        ];
        let m = ShadowMetrics::from_comparisons(&comparisons);
        assert!((m.direction_accuracy - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_shadow_metrics_win_rate() {
        let comparisons = vec![
            make_comparison("m", 100.0, 80.0, true),   // win
            make_comparison("m", 100.0, -20.0, false), // loss
            make_comparison("m", 100.0, 50.0, true),   // win
        ];
        let m = ShadowMetrics::from_comparisons(&comparisons);
        assert!((m.win_rate - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_shadow_metrics_total_pnl() {
        let comparisons = vec![
            make_comparison("m", 100.0, 80.0, true),
            make_comparison("m", 100.0, -20.0, false),
        ];
        let m = ShadowMetrics::from_comparisons(&comparisons);
        assert!((m.total_virtual_pnl - 60.0).abs() < f64::EPSILON);
        assert!((m.avg_virtual_pnl - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_shadow_metrics_meets_thresholds() {
        let comparisons = vec![
            make_comparison("m", 100.0, 80.0, true),
            make_comparison("m", 100.0, 80.0, true),
        ];
        let m = ShadowMetrics::from_comparisons(&comparisons);

        // direction_accuracy = 1.0, so should meet 0.5
        assert!(m.meets_thresholds(0.5, -100.0, -100.0));
        // direction_accuracy = 1.0, should not meet 1.1
        assert!(!m.meets_thresholds(1.1, -100.0, -100.0));
    }

    #[test]
    fn test_sharpe_ratio_calculation() {
        // All same PnLs -> zero std -> zero Sharpe
        let pnls = vec![100.0, 100.0, 100.0];
        let sharpe = ShadowMetrics::compute_sharpe(&pnls);
        assert!((sharpe - 0.0).abs() < f64::EPSILON);

        // Varied PnLs
        let pnls = vec![100.0, 50.0, 150.0, 75.0];
        let sharpe = ShadowMetrics::compute_sharpe(&pnls);
        // Mean = 93.75, should have positive Sharpe
        assert!(sharpe > 0.0);
    }

    #[test]
    fn test_max_drawdown_calculation() {
        // Increasing PnLs -> no drawdown
        let pnls = vec![10.0, 20.0, 30.0];
        let dd = ShadowMetrics::compute_max_drawdown(&pnls);
        assert!((dd - 0.0).abs() < f64::EPSILON);

        // Decreasing PnLs -> full drawdown
        let pnls = vec![-10.0, -20.0, -30.0];
        let dd = ShadowMetrics::compute_max_drawdown(&pnls);
        assert!((dd - (-60.0)).abs() < f64::EPSILON);

        // Mixed: cumulative = [100, 50, 0, 30], peak=100, trough=0 -> dd=-100
        let pnls = vec![100.0, -50.0, -50.0, 30.0];
        let dd = ShadowMetrics::compute_max_drawdown(&pnls);
        assert!((dd - (-100.0)).abs() < f64::EPSILON);

        // Peak then partial recovery: cumulative = [100, 70, 30, 20], peak=100, trough=20 -> dd=-80
        let pnls = vec![100.0, -30.0, -40.0, -10.0];
        let dd = ShadowMetrics::compute_max_drawdown(&pnls);
        assert!((dd - (-80.0)).abs() < f64::EPSILON);

        // No drawdown: monotonically increasing cumulative
        let pnls = vec![50.0, 30.0, 20.0]; // cumulative = [50, 80, 100]
        let dd = ShadowMetrics::compute_max_drawdown(&pnls);
        assert!((dd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_rolling_metrics() {
        let comparisons: Vec<PredictionComparison> = (0..10)
            .map(|i| make_comparison("m", 100.0, if i < 5 { 80.0 } else { -20.0 }, i < 5))
            .collect();

        // All comparisons: 5 correct, 5 incorrect -> 0.5 accuracy
        let m = RollingMetrics::from_comparisons(&comparisons, 10);
        assert!((m.direction_accuracy - 0.5).abs() < 1e-9);

        // Last 5: 0 correct (last 5 have direction_correct = false) -> 0.0 accuracy
        let m = RollingMetrics::from_comparisons(&comparisons, 5);
        assert!((m.direction_accuracy - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_confidence_distribution() {
        let comparisons: Vec<PredictionComparison> = (0..10)
            .map(|i| {
                let conf = if i < 3 { 0.2 } else if i < 7 { 0.5 } else { 0.8 };
                let pred = ShadowPrediction::new("m", "1.0.0", "BTCUSDT.BINANCE", Direction::Long, 100.0, conf);
                PredictionComparison::new(pred, 80.0, Direction::Long)
            })
            .collect();

        let dist = ConfidenceDistribution::from_comparisons(&comparisons);
        assert_eq!(dist.low_confidence, 3);
        assert_eq!(dist.medium_confidence, 4);
        assert_eq!(dist.high_confidence, 3);
        assert_eq!(dist.total, 10);
        assert!((dist.high_confidence_pct() - 0.3).abs() < 1e-9);
    }
}
