//! Reward Functions for RL Environment
//!
//! Provides the `RewardFunction` trait and implementations for computing
//! rewards from portfolio state transitions.
//!
//! # Reward Types
//!
//! - `PnlReward`: Simple PnL change between steps
//! - `SharpeReward`: Risk-adjusted return using rolling Sharpe ratio
//! - `RiskAdjustedReward`: PnL with drawdown penalty

use serde::{Deserialize, Serialize};
use std::sync::RwLock;

use super::action::ActionValue;
use super::observation::PortfolioSnapshot;

/// Trait for computing reward from portfolio state transitions.
///
/// Given the previous and current portfolio snapshots plus the action taken,
/// computes a scalar reward signal for the RL agent.
pub trait RewardFunction: Send + Sync {
    /// Compute the reward for a state transition.
    ///
    /// # Arguments
    /// * `prev` - Portfolio state before the step
    /// * `curr` - Portfolio state after the step
    /// * `action` - The action that was taken
    ///
    /// # Returns
    /// A scalar reward value
    fn compute(&self, prev: &PortfolioSnapshot, curr: &PortfolioSnapshot, action: &ActionValue) -> f64;

    /// Reset any internal state (e.g., running statistics).
    fn reset(&mut self) {}

    /// Human-readable name for this reward function.
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// PnlReward
// ---------------------------------------------------------------------------

/// Simple PnL-based reward.
///
/// Reward = (curr.equity - prev.equity) / prev.equity
///
/// This is the most basic reward: normalized equity change.
/// Optionally scales the reward by a multiplier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PnlReward {
    /// Reward scaling factor
    scale: f64,
}

impl PnlReward {
    /// Create a new PnlReward with default scale of 1.0.
    pub fn new() -> Self {
        Self { scale: 1.0 }
    }

    /// Create with custom scale.
    pub fn with_scale(scale: f64) -> Self {
        Self { scale }
    }
}

impl Default for PnlReward {
    fn default() -> Self {
        Self::new()
    }
}

impl RewardFunction for PnlReward {
    fn compute(&self, prev: &PortfolioSnapshot, curr: &PortfolioSnapshot, _action: &ActionValue) -> f64 {
        if prev.equity.abs() < f64::EPSILON {
            return 0.0;
        }
        let pnl = (curr.equity - prev.equity) / prev.equity;
        pnl * self.scale
    }

    fn name(&self) -> &str {
        "PnlReward"
    }
}

// ---------------------------------------------------------------------------
// SharpeReward
// ---------------------------------------------------------------------------

/// Sharpe-ratio-based reward.
///
/// Uses a rolling window of returns to compute an approximate Sharpe ratio.
/// The reward at each step is the incremental Sharpe contribution.
///
/// Sharpe ≈ mean(returns) / std(returns)
///
/// This encourages consistent positive returns rather than
/// large volatile swings.
#[derive(Debug, Serialize, Deserialize)]
pub struct SharpeReward {
    /// Rolling window size for Sharpe computation
    pub window: usize,
    /// Risk-free rate (annualized, will be converted per-step)
    pub risk_free_rate: f64,
    /// Reward scaling factor
    pub scale: f64,
    /// Recent returns for Sharpe computation (interior mutability)
    #[serde(skip)]
    returns: RwLock<Vec<f64>>,
}

impl Clone for SharpeReward {
    fn clone(&self) -> Self {
        let returns = self.returns.read().unwrap_or_else(|e| e.into_inner()).clone();
        Self {
            window: self.window,
            risk_free_rate: self.risk_free_rate,
            scale: self.scale,
            returns: RwLock::new(returns),
        }
    }
}

impl SharpeReward {
    /// Create a new SharpeReward with the given window size.
    pub fn new(window: usize) -> Self {
        Self {
            window,
            risk_free_rate: 0.0,
            scale: 1.0,
            returns: RwLock::new(Vec::with_capacity(window)),
        }
    }

    /// Create with custom risk-free rate and scale.
    pub fn with_params(window: usize, risk_free_rate: f64, scale: f64) -> Self {
        Self {
            window,
            risk_free_rate,
            scale,
            returns: RwLock::new(Vec::with_capacity(window)),
        }
    }

    /// Compute Sharpe ratio from the current returns buffer.
    fn sharpe_ratio(&self) -> f64 {
        let returns = self.returns.read().unwrap_or_else(|e| e.into_inner());
        if returns.len() < 2 {
            return 0.0;
        }

        let n = returns.len() as f64;
        let mean = returns.iter().sum::<f64>() / n;
        let variance = returns.iter()
            .map(|r| (r - mean).powi(2))
            .sum::<f64>() / n;

        let std_dev = variance.sqrt();
        if std_dev < f64::EPSILON {
            return 0.0;
        }

        (mean - self.risk_free_rate) / std_dev
    }
}

impl RewardFunction for SharpeReward {
    fn compute(&self, prev: &PortfolioSnapshot, curr: &PortfolioSnapshot, _action: &ActionValue) -> f64 {
        let ret = if prev.equity.abs() < f64::EPSILON {
            0.0
        } else {
            (curr.equity - prev.equity) / prev.equity
        };

        {
            let mut returns = self.returns.write().unwrap_or_else(|e| e.into_inner());
            returns.push(ret);
            if returns.len() > self.window {
                returns.remove(0);
            }
        }

        // Reward is the change in Sharpe ratio, or just the ratio if first computation
        self.sharpe_ratio() * self.scale
    }

    fn reset(&mut self) {
        let mut returns = self.returns.write().unwrap_or_else(|e| e.into_inner());
        returns.clear();
    }

    fn name(&self) -> &str {
        "SharpeReward"
    }
}

// ---------------------------------------------------------------------------
// RiskAdjustedReward
// ---------------------------------------------------------------------------

/// Risk-adjusted reward: PnL with drawdown penalty.
///
/// reward = pnl - λ * drawdown
///
/// Where:
/// - `pnl` = normalized equity change
/// - `drawdown` = current drawdown from peak
/// - `λ` (lambda) = risk aversion parameter
///
/// Higher λ penalizes drawdowns more aggressively.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskAdjustedReward {
    /// Risk aversion parameter (drawdown penalty weight)
    lambda: f64,
    /// Reward scaling factor
    scale: f64,
}

impl RiskAdjustedReward {
    /// Create a new RiskAdjustedReward with default lambda=2.0.
    pub fn new() -> Self {
        Self {
            lambda: 2.0,
            scale: 1.0,
        }
    }

    /// Create with custom lambda.
    pub fn with_lambda(lambda: f64) -> Self {
        Self {
            lambda,
            scale: 1.0,
        }
    }

    /// Create with custom lambda and scale.
    pub fn with_params(lambda: f64, scale: f64) -> Self {
        Self { lambda, scale }
    }
}

impl Default for RiskAdjustedReward {
    fn default() -> Self {
        Self::new()
    }
}

impl RewardFunction for RiskAdjustedReward {
    fn compute(&self, prev: &PortfolioSnapshot, curr: &PortfolioSnapshot, _action: &ActionValue) -> f64 {
        if prev.equity.abs() < f64::EPSILON {
            return 0.0;
        }

        let pnl = (curr.equity - prev.equity) / prev.equity;
        let drawdown = curr.current_drawdown();
        let reward = pnl - self.lambda * drawdown;

        reward * self.scale
    }

    fn name(&self) -> &str {
        "RiskAdjustedReward"
    }
}

// ---------------------------------------------------------------------------
// CompositeReward
// ---------------------------------------------------------------------------

/// Weighted combination of multiple reward functions.
///
/// reward = Σ (weight_i * reward_i)
///
/// Allows combining different reward signals (e.g., PnL + Sharpe)
/// with configurable weights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositeReward {
    /// Component reward functions with weights
    components: Vec<(f64, RewardFunctionType)>,
}

/// Enum of reward function types for composition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RewardFunctionType {
    /// Simple PnL reward
    Pnl(PnlReward),
    /// Sharpe ratio reward
    Sharpe(SharpeReward),
    /// Risk-adjusted reward
    RiskAdjusted(RiskAdjustedReward),
}

impl CompositeReward {
    /// Create a new empty composite reward.
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    /// Add a component with weight.
    pub fn add(mut self, weight: f64, reward: RewardFunctionType) -> Self {
        self.components.push((weight, reward));
        self
    }
}

impl Default for CompositeReward {
    fn default() -> Self {
        Self::new()
    }
}

impl RewardFunction for CompositeReward {
    fn compute(&self, prev: &PortfolioSnapshot, curr: &PortfolioSnapshot, action: &ActionValue) -> f64 {
        let mut total = 0.0;
        for (weight, reward_fn) in &self.components {
            let r = match reward_fn {
                RewardFunctionType::Pnl(r) => r.compute(prev, curr, action),
                RewardFunctionType::Sharpe(r) => r.compute(prev, curr, action),
                RewardFunctionType::RiskAdjusted(r) => r.compute(prev, curr, action),
            };
            total += weight * r;
        }
        total
    }

    fn reset(&mut self) {
        for (_, reward_fn) in &mut self.components {
            match reward_fn {
                RewardFunctionType::Pnl(r) => r.reset(),
                RewardFunctionType::Sharpe(r) => r.reset(),
                RewardFunctionType::RiskAdjusted(r) => r.reset(),
            }
        }
    }

    fn name(&self) -> &str {
        "CompositeReward"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::Direction;
    use chrono::Utc;

    fn make_snapshot(equity: f64) -> PortfolioSnapshot {
        PortfolioSnapshot {
            equity,
            cash: equity,
            position_value: 0.0,
            unrealized_pnl: 0.0,
            realized_pnl: 0.0,
            position_qty: 0.0,
            avg_entry_price: 0.0,
            position_direction: None,
            peak_equity: equity,
            max_drawdown: 0.0,
            trade_count: 0,
            timestamp: Utc::now(),
        }
    }

    fn make_snapshot_with_drawdown(equity: f64, peak: f64) -> PortfolioSnapshot {
        let mut snap = make_snapshot(peak);
        snap.equity = equity;
        snap.peak_equity = peak;
        snap.update_equity(equity);
        snap
    }

    // --- PnlReward ---

    #[test]
    fn test_pnl_reward_positive() {
        let reward_fn = PnlReward::new();
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        assert!((reward - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_pnl_reward_negative() {
        let reward_fn = PnlReward::new();
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(99_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        assert!((reward - (-0.01)).abs() < 1e-10);
    }

    #[test]
    fn test_pnl_reward_zero_equity() {
        let reward_fn = PnlReward::new();
        let prev = make_snapshot(0.0);
        let curr = make_snapshot(100.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(0));
        assert_eq!(reward, 0.0);
    }

    #[test]
    fn test_pnl_reward_with_scale() {
        let reward_fn = PnlReward::with_scale(100.0);
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        assert!((reward - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_pnl_reward_name() {
        let reward_fn = PnlReward::new();
        assert_eq!(reward_fn.name(), "PnlReward");
    }

    // --- SharpeReward ---

    #[test]
    fn test_sharpe_reward_single_step() {
        let mut reward_fn = SharpeReward::new(10);
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        // With only 1 return, Sharpe is 0 (need at least 2)
        assert_eq!(reward, 0.0);
    }

    #[test]
    fn test_sharpe_reward_multiple_steps() {
        let mut reward_fn = SharpeReward::new(10);
        let base = 100_000.0;
        let prev = make_snapshot(base);
        
        // Step 1: +1%
        let curr1 = make_snapshot(base * 1.01);
        let _r1 = reward_fn.compute(&prev, &curr1, &ActionValue::Discrete(1));
        
        // Step 2: +2% (varied positive returns → positive Sharpe)
        let curr2 = make_snapshot(base * 1.01 * 1.02);
        let r2 = reward_fn.compute(&curr1, &curr2, &ActionValue::Discrete(1));
        
        // With 2 positive returns (1% and 2%), mean > 0 and std > 0 → Sharpe > 0
        assert!(r2 > 0.0);
    }

    #[test]
    fn test_sharpe_reward_reset() {
        let mut reward_fn = SharpeReward::new(10);
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        
        reward_fn.reset();
        // After reset, returns buffer should be empty → Sharpe = 0
        assert_eq!(reward_fn.returns.read().unwrap_or_else(|e| e.into_inner()).len(), 0);
    }

    #[test]
    fn test_sharpe_reward_name() {
        let reward_fn = SharpeReward::new(10);
        assert_eq!(reward_fn.name(), "SharpeReward");
    }

    // --- RiskAdjustedReward ---

    #[test]
    fn test_risk_adjusted_reward_positive_pnl_no_drawdown() {
        let reward_fn = RiskAdjustedReward::new();
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        // PnL = 0.01, drawdown = 0 → reward = 0.01
        assert!((reward - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_risk_adjusted_reward_with_drawdown() {
        let reward_fn = RiskAdjustedReward::with_lambda(2.0);
        let prev = make_snapshot_with_drawdown(95_000.0, 100_000.0);
        let curr = make_snapshot_with_drawdown(96_000.0, 100_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        
        // PnL = (96k - 95k) / 95k ≈ 0.01053
        // Drawdown of curr = (100k - 96k) / 100k = 0.04
        // reward = 0.01053 - 2.0 * 0.04 = 0.01053 - 0.08 = -0.06947
        let expected_pnl = 1000.0 / 95000.0;
        let expected_dd = 4000.0 / 100000.0;
        let expected = expected_pnl - 2.0 * expected_dd;
        assert!((reward - expected).abs() < 1e-8);
    }

    #[test]
    fn test_risk_adjusted_reward_zero_equity() {
        let reward_fn = RiskAdjustedReward::new();
        let prev = make_snapshot(0.0);
        let curr = make_snapshot(100.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(0));
        assert_eq!(reward, 0.0);
    }

    #[test]
    fn test_risk_adjusted_reward_name() {
        let reward_fn = RiskAdjustedReward::new();
        assert_eq!(reward_fn.name(), "RiskAdjustedReward");
    }

    // --- CompositeReward ---

    #[test]
    fn test_composite_reward_empty() {
        let reward_fn = CompositeReward::new();
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        assert_eq!(reward, 0.0);
    }

    #[test]
    fn test_composite_reward_pnl_only() {
        let reward_fn = CompositeReward::new()
            .add(1.0, RewardFunctionType::Pnl(PnlReward::new()));
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        assert!((reward - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_composite_reward_weighted() {
        let reward_fn = CompositeReward::new()
            .add(0.7, RewardFunctionType::Pnl(PnlReward::new()))
            .add(0.3, RewardFunctionType::RiskAdjusted(RiskAdjustedReward::with_lambda(0.0)));
        let prev = make_snapshot(100_000.0);
        let curr = make_snapshot(101_000.0);
        let reward = reward_fn.compute(&prev, &curr, &ActionValue::Discrete(1));
        // Both compute 0.01; weighted = 0.7*0.01 + 0.3*0.01 = 0.01
        assert!((reward - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_composite_reward_name() {
        let reward_fn = CompositeReward::new();
        assert_eq!(reward_fn.name(), "CompositeReward");
    }
}
