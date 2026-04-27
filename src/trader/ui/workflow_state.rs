//! Workflow state for cross-panel coordination.
//!
//! Provides shared state and action types that connect the research → backtest → deploy
//! pipeline across UI panels.

use std::sync::Mutex;

use crate::backtesting::BacktestingStatistics;
use crate::trader::Interval;
use crate::strategy::base::StrategySetting;

/// Summary of a completed backtest, stored for deployment and chart integration.
#[derive(Clone, Debug)]
pub struct BacktestSummary {
    pub strategy_name: String,
    pub strategy_class: String,
    pub vt_symbol: String,
    pub interval: Interval,
    pub rate: f64,
    pub slippage: f64,
    pub capital: f64,
    pub statistics: BacktestingStatistics,
}

/// Configuration for deploying a backtested strategy to live/paper trading.
#[derive(Clone, Debug)]
pub struct StrategyDeployConfig {
    pub strategy_name: String,
    pub strategy_class: String,
    pub vt_symbol: String,
    pub interval: Interval,
    pub rate: f64,
    pub slippage: f64,
    pub capital: f64,
    pub mode: DeployMode,
    pub strategy_setting: StrategySetting,
}

/// Deployment mode for strategy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeployMode {
    #[default]
    Paper,
    Live,
}

impl std::fmt::Display for DeployMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployMode::Paper => write!(f, "PAPER"),
            DeployMode::Live => write!(f, "LIVE"),
        }
    }
}

/// Cross-panel workflow actions that panels can request.
#[derive(Clone, Debug)]
pub enum WorkflowAction {
    /// Send an alpha model/dataset to backtesting panel for verification
    SendToBacktest {
        model_name: String,
        dataset_name: String,
        vt_symbol: String,
    },
    /// Deploy a backtested strategy to live/paper trading
    DeployToLive(StrategyDeployConfig),
    /// Auto-open a chart for the given `vt_symbol`
    OpenChart(String),
    /// Navigate to a specific central tab
    NavigateTo(super::main_window::CentralTab),
}

/// Shared workflow state accessible from all panels.
pub struct WorkflowState {
    /// Most recent backtest result summary
    pub backtest_summary: Option<BacktestSummary>,
    /// Pending workflow actions queued by panels
    pub pending_actions: Vec<WorkflowAction>,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            backtest_summary: None,
            pending_actions: Vec::new(),
        }
    }

    /// Push a workflow action for `MainWindow` to process
    pub fn push_action(&mut self, action: WorkflowAction) {
        self.pending_actions.push(action);
    }

    /// Drain all pending actions
    pub fn drain_actions(&mut self) -> Vec<WorkflowAction> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Store backtest summary after a successful backtest
    pub fn set_backtest_summary(&mut self, summary: BacktestSummary) {
        self.backtest_summary = Some(summary);
    }
}

/// Helper type alias for the shared workflow state used across panels.
pub type SharedWorkflowState = Arc<Mutex<WorkflowState>>;

use std::sync::Arc;
