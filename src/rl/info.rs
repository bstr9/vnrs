//! Step and Episode Diagnostic Info
//!
//! Provides diagnostic data structures returned by the RL environment's
//! `step()` and at episode completion. These follow the Gymnasium (OpenAI Gym)
//! convention where `step()` returns `(observation, reward, terminated, truncated, info)`
//! and `info` carries auxiliary data.

use serde::{Deserialize, Serialize};

use super::observation::PortfolioSnapshot;

/// Diagnostic information returned by each `step()` call.
///
/// Contains execution details that are useful for logging, analysis,
/// and debugging but are NOT part of the observation space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInfo {
    /// Number of orders generated from the action
    pub orders_generated: usize,
    /// Number of orders that were actually filled this step
    pub orders_filled: usize,
    /// Total commission paid this step
    pub commission: f64,
    /// Total slippage cost this step
    pub slippage: f64,
    /// Mark-to-market PnL for this step (before commission/slippage)
    pub step_pnl: f64,
    /// Net PnL for this step (after commission/slippage)
    pub net_pnl: f64,
    /// Current drawdown from peak equity
    pub drawdown: f64,
    /// Whether the step resulted in a position change
    pub position_changed: bool,
    /// Any warning messages generated during this step
    pub warnings: Vec<String>,
}

impl StepInfo {
    /// Create an empty StepInfo with zeroed fields.
    pub fn empty() -> Self {
        Self {
            orders_generated: 0,
            orders_filled: 0,
            commission: 0.0,
            slippage: 0.0,
            step_pnl: 0.0,
            net_pnl: 0.0,
            drawdown: 0.0,
            position_changed: false,
            warnings: Vec::new(),
        }
    }
}

impl Default for StepInfo {
    fn default() -> Self {
        Self::empty()
    }
}

/// Summary information for a completed episode.
///
/// Produced when the episode terminates (end of data, max steps, or
/// terminal condition like margin call).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeInfo {
    /// Total number of steps in the episode
    pub total_steps: usize,
    /// Cumulative reward over the entire episode
    pub total_reward: f64,
    /// Final portfolio snapshot
    pub final_portfolio: PortfolioSnapshot,
    /// Maximum drawdown observed during the episode
    pub max_drawdown: f64,
    /// Total commission paid during the episode
    pub total_commission: f64,
    /// Total slippage cost during the episode
    pub total_slippage: f64,
    /// Total number of trades executed
    pub total_trades: usize,
    /// Whether the episode ended naturally (end of data) or by condition
    pub termination_reason: TerminationReason,
    /// Per-step rewards (optional, for detailed analysis)
    pub rewards: Vec<f64>,
}

/// Reason why the episode terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerminationReason {
    /// Reached the end of available data
    DataExhausted,
    /// Reached the configured maximum number of steps
    MaxStepsReached,
    /// Portfolio equity fell below the minimum threshold (margin call)
    MarginCall,
    /// Episode truncated (time limit or external signal)
    Truncated,
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminationReason::DataExhausted => write!(f, "DataExhausted"),
            TerminationReason::MaxStepsReached => write!(f, "MaxStepsReached"),
            TerminationReason::MarginCall => write!(f, "MarginCall"),
            TerminationReason::Truncated => write!(f, "Truncated"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step_info_default() {
        let info = StepInfo::default();
        assert_eq!(info.orders_generated, 0);
        assert_eq!(info.orders_filled, 0);
        assert_eq!(info.commission, 0.0);
        assert_eq!(info.step_pnl, 0.0);
        assert!(info.warnings.is_empty());
    }

    #[test]
    fn test_step_info_empty() {
        let info = StepInfo::empty();
        assert_eq!(info.net_pnl, 0.0);
        assert!(!info.position_changed);
        assert_eq!(info.drawdown, 0.0);
    }

    #[test]
    fn test_termination_reason_display() {
        assert_eq!(format!("{}", TerminationReason::DataExhausted), "DataExhausted");
        assert_eq!(format!("{}", TerminationReason::MaxStepsReached), "MaxStepsReached");
        assert_eq!(format!("{}", TerminationReason::MarginCall), "MarginCall");
        assert_eq!(format!("{}", TerminationReason::Truncated), "Truncated");
    }

    #[test]
    fn test_episode_info_serialization() {
        let info = EpisodeInfo {
            total_steps: 100,
            total_reward: 50.0,
            final_portfolio: PortfolioSnapshot::new(1_000_000.0),
            max_drawdown: 0.15,
            total_commission: 10.0,
            total_slippage: 5.0,
            total_trades: 20,
            termination_reason: TerminationReason::DataExhausted,
            rewards: vec![1.0, 2.0, 3.0],
        };
        let json = serde_json::to_string(&info).expect("serialization should succeed");
        let parsed: EpisodeInfo = serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(parsed.total_steps, 100);
        assert_eq!(parsed.total_reward, 50.0);
        assert_eq!(parsed.termination_reason, TerminationReason::DataExhausted);
    }
}
