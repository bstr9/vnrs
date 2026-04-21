//! Promotion Policy - defines when a shadow model can be promoted.
//!
//! Provides policy configuration and evaluation logic for promoting shadow
//! models to canary or production stages based on performance metrics.

use serde::{Deserialize, Serialize};

use super::metrics::ShadowMetrics;

// ---------------------------------------------------------------------------
// PromotionPolicy - policy configuration
// ---------------------------------------------------------------------------

/// Policy for promoting shadow models to higher stages.
///
/// Defines the minimum criteria a shadow model must meet before it can be
/// considered for promotion to canary or production.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromotionPolicy {
    /// Minimum number of predictions before promotion is considered.
    pub min_predictions: usize,
    /// Minimum direction accuracy (0.0 to 1.0).
    pub min_accuracy: f64,
    /// Minimum virtual Sharpe ratio.
    pub min_virtual_sharpe: f64,
    /// Maximum acceptable virtual drawdown (negative value, e.g., -0.20 for 20%).
    pub max_virtual_drawdown: f64,
    /// Minimum win rate (0.0 to 1.0).
    pub min_win_rate: f64,
    /// Minimum average confidence.
    pub min_avg_confidence: f64,
}

impl Default for PromotionPolicy {
    fn default() -> Self {
        Self {
            min_predictions: 50,
            min_accuracy: 0.55,
            min_virtual_sharpe: 0.5,
            max_virtual_drawdown: -0.20,
            min_win_rate: 0.50,
            min_avg_confidence: 0.40,
        }
    }
}

impl PromotionPolicy {
    /// Create a new promotion policy with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a strict promotion policy.
    pub fn strict() -> Self {
        Self {
            min_predictions: 100,
            min_accuracy: 0.65,
            min_virtual_sharpe: 1.0,
            max_virtual_drawdown: -0.10,
            min_win_rate: 0.60,
            min_avg_confidence: 0.60,
        }
    }

    /// Create a lenient promotion policy.
    pub fn lenient() -> Self {
        Self {
            min_predictions: 20,
            min_accuracy: 0.50,
            min_virtual_sharpe: 0.0,
            max_virtual_drawdown: -0.30,
            min_win_rate: 0.40,
            min_avg_confidence: 0.30,
        }
    }

    /// Set minimum predictions.
    pub fn with_min_predictions(mut self, n: usize) -> Self {
        self.min_predictions = n;
        self
    }

    /// Set minimum accuracy.
    pub fn with_min_accuracy(mut self, acc: f64) -> Self {
        self.min_accuracy = acc;
        self
    }

    /// Set minimum virtual Sharpe.
    pub fn with_min_sharpe(mut self, sharpe: f64) -> Self {
        self.min_virtual_sharpe = sharpe;
        self
    }

    /// Set maximum virtual drawdown.
    pub fn with_max_drawdown(mut self, dd: f64) -> Self {
        self.max_virtual_drawdown = dd;
        self
    }

    /// Evaluate whether a model is eligible for promotion.
    pub fn evaluate(&self, metrics: &ShadowMetrics) -> PromotionDecision {
        if metrics.prediction_count < self.min_predictions {
            return PromotionDecision::NeedMoreData {
                current: metrics.prediction_count,
                required: self.min_predictions,
            };
        }

        if metrics.direction_accuracy < self.min_accuracy {
            return PromotionDecision::Reject {
                reason: format!(
                    "Direction accuracy {:.2}% below minimum {:.2}%",
                    metrics.direction_accuracy * 100.0,
                    self.min_accuracy * 100.0
                ),
            };
        }

        if metrics.virtual_sharpe < self.min_virtual_sharpe {
            return PromotionDecision::Reject {
                reason: format!(
                    "Virtual Sharpe {:.3} below minimum {:.3}",
                    metrics.virtual_sharpe, self.min_virtual_sharpe
                ),
            };
        }

        if metrics.max_virtual_drawdown < self.max_virtual_drawdown {
            return PromotionDecision::Reject {
                reason: format!(
                    "Max drawdown {:.2}% exceeds maximum {:.2}%",
                    metrics.max_virtual_drawdown.abs() * 100.0,
                    self.max_virtual_drawdown.abs() * 100.0
                ),
            };
        }

        if metrics.win_rate < self.min_win_rate {
            return PromotionDecision::Reject {
                reason: format!(
                    "Win rate {:.2}% below minimum {:.2}%",
                    metrics.win_rate * 100.0,
                    self.min_win_rate * 100.0
                ),
            };
        }

        if metrics.avg_confidence < self.min_avg_confidence {
            return PromotionDecision::Reject {
                reason: format!(
                    "Average confidence {:.2}% below minimum {:.2}%",
                    metrics.avg_confidence * 100.0,
                    self.min_avg_confidence * 100.0
                ),
            };
        }

        PromotionDecision::Promote {
            score: self.compute_score(metrics),
        }
    }

    /// Compute a promotion score (0.0 to 1.0).
    ///
    /// Higher score indicates better performance.
    fn compute_score(&self, metrics: &ShadowMetrics) -> f64 {
        let accuracy_score = (metrics.direction_accuracy - self.min_accuracy)
            .max(0.0)
            .min(1.0 - self.min_accuracy)
            / (1.0 - self.min_accuracy).max(0.01);

        let sharpe_score = (metrics.virtual_sharpe - self.min_virtual_sharpe)
            .max(0.0)
            .min(2.0 - self.min_virtual_sharpe)
            / (2.0 - self.min_virtual_sharpe).max(0.01);

        let drawdown_range = self.max_virtual_drawdown.abs();
        let drawdown_score = (metrics.max_virtual_drawdown.abs() - drawdown_range)
            .max(0.0)
            .min(drawdown_range)
            / drawdown_range.max(0.01);

        (accuracy_score + sharpe_score + drawdown_score) / 3.0
    }
}

// ---------------------------------------------------------------------------
// PromotionDecision - result of evaluation
// ---------------------------------------------------------------------------

/// Result of evaluating a shadow model for promotion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PromotionDecision {
    /// Model meets criteria and can be promoted.
    Promote {
        /// Promotion score (0.0 to 1.0, higher is better).
        score: f64,
    },
    /// Model does not meet criteria.
    Reject {
        /// Reason for rejection.
        reason: String,
    },
    /// Not enough data to evaluate.
    NeedMoreData {
        /// Current number of predictions.
        current: usize,
        /// Required number of predictions.
        required: usize,
    },
}

impl PromotionDecision {
    /// Check if the decision is to promote.
    pub fn is_promote(&self) -> bool {
        matches!(self, PromotionDecision::Promote { .. })
    }

    /// Check if the decision is to reject.
    pub fn is_reject(&self) -> bool {
        matches!(self, PromotionDecision::Reject { .. })
    }

    /// Check if more data is needed.
    pub fn needs_more_data(&self) -> bool {
        matches!(self, PromotionDecision::NeedMoreData { .. })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metrics(
        count: usize,
        accuracy: f64,
        sharpe: f64,
        drawdown: f64,
        win_rate: f64,
        confidence: f64,
    ) -> ShadowMetrics {
        ShadowMetrics {
            prediction_count: count,
            direction_accuracy: accuracy,
            virtual_sharpe: sharpe,
            max_virtual_drawdown: drawdown,
            total_virtual_pnl: 0.0,
            avg_virtual_pnl: 0.0,
            win_rate,
            avg_confidence: confidence,
            computed_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_promotion_policy_default() {
        let p = PromotionPolicy::default();
        assert_eq!(p.min_predictions, 50);
        assert!((p.min_accuracy - 0.55).abs() < f64::EPSILON);
        assert!((p.min_virtual_sharpe - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_promotion_policy_strict() {
        let p = PromotionPolicy::strict();
        assert_eq!(p.min_predictions, 100);
        assert!((p.min_accuracy - 0.65).abs() < f64::EPSILON);
    }

    #[test]
    fn test_promotion_policy_lenient() {
        let p = PromotionPolicy::lenient();
        assert_eq!(p.min_predictions, 20);
        assert!((p.min_accuracy - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn test_promotion_policy_builder() {
        let p = PromotionPolicy::new()
            .with_min_predictions(30)
            .with_min_accuracy(0.6)
            .with_min_sharpe(0.8)
            .with_max_drawdown(-0.15);

        assert_eq!(p.min_predictions, 30);
        assert!((p.min_accuracy - 0.6).abs() < f64::EPSILON);
        assert!((p.min_virtual_sharpe - 0.8).abs() < f64::EPSILON);
        assert!((p.max_virtual_drawdown - (-0.15)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_evaluate_need_more_data() {
        let policy = PromotionPolicy::default();
        let metrics = make_metrics(10, 1.0, 2.0, -0.05, 1.0, 1.0);

        let decision = policy.evaluate(&metrics);
        assert!(decision.needs_more_data());
    }

    #[test]
    fn test_evaluate_reject_accuracy() {
        let policy = PromotionPolicy::default();
        let metrics = make_metrics(100, 0.3, 2.0, -0.05, 1.0, 1.0);

        let decision = policy.evaluate(&metrics);
        assert!(decision.is_reject());
    }

    #[test]
    fn test_evaluate_reject_sharpe() {
        let policy = PromotionPolicy::default();
        let metrics = make_metrics(100, 0.8, 0.1, -0.05, 1.0, 1.0);

        let decision = policy.evaluate(&metrics);
        assert!(decision.is_reject());
    }

    #[test]
    fn test_evaluate_reject_drawdown() {
        let policy = PromotionPolicy::default();
        let metrics = make_metrics(100, 0.8, 2.0, -0.50, 1.0, 1.0);

        let decision = policy.evaluate(&metrics);
        assert!(decision.is_reject());
    }

    #[test]
    fn test_evaluate_promote() {
        let policy = PromotionPolicy::default();
        let metrics = make_metrics(100, 0.8, 1.5, -0.10, 0.7, 0.8);

        let decision = policy.evaluate(&metrics);
        assert!(decision.is_promote());
    }

    #[test]
    fn test_evaluate_all_criteria() {
        let policy = PromotionPolicy::default()
            .with_min_predictions(10)
            .with_min_accuracy(0.5)
            .with_min_sharpe(0.0)
            .with_max_drawdown(-0.30)
            .with_min_predictions(10);

        // All criteria met
        let metrics = make_metrics(20, 0.6, 0.5, -0.15, 0.6, 0.5);
        let decision = policy.evaluate(&metrics);
        assert!(decision.is_promote());

        // Win rate too low
        let metrics = make_metrics(20, 0.6, 0.5, -0.15, 0.3, 0.5);
        let decision = policy.evaluate(&metrics);
        assert!(decision.is_reject());
    }

    #[test]
    fn test_decision_helpers() {
        let promote = PromotionDecision::Promote { score: 0.8 };
        assert!(promote.is_promote());
        assert!(!promote.is_reject());
        assert!(!promote.needs_more_data());

        let reject = PromotionDecision::Reject { reason: "test".into() };
        assert!(!reject.is_promote());
        assert!(reject.is_reject());
        assert!(!reject.needs_more_data());

        let need_more = PromotionDecision::NeedMoreData { current: 10, required: 50 };
        assert!(!need_more.is_promote());
        assert!(!need_more.is_reject());
        assert!(need_more.needs_more_data());
    }
}
