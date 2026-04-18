//! PyPortfolioStatistics — PyO3 class exposing portfolio-level performance metrics to Python.
//!
//! Wraps `BacktestingStatistics` computed from the daily result history stored
//! in `PortfolioState`. Python strategies call `portfolio.statistics()` to get
//! an instance of this class.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::backtesting::BacktestingStatistics;

// ---------------------------------------------------------------------------
// PyPortfolioStatistics
// ---------------------------------------------------------------------------

/// Portfolio-level statistics exposed to Python.
///
/// ```python
/// stats = portfolio.statistics()
/// print(stats.sharpe_ratio, stats.max_drawdown, stats.total_net_pnl)
/// d = stats.to_dict()
/// ```
#[pyclass(name = "PortfolioStatistics")]
#[derive(Clone)]
pub struct PyPortfolioStatistics {
    inner: BacktestingStatistics,
}

impl PyPortfolioStatistics {
    pub fn from_statistics(stats: BacktestingStatistics) -> Self {
        Self { inner: stats }
    }
}

#[pymethods]
impl PyPortfolioStatistics {
    #[getter]
    fn start_date(&self) -> &str {
        &self.inner.start_date
    }

    #[getter]
    fn end_date(&self) -> &str {
        &self.inner.end_date
    }

    #[getter]
    fn total_days(&self) -> u32 {
        self.inner.total_days
    }

    #[getter]
    fn profit_days(&self) -> u32 {
        self.inner.profit_days
    }

    #[getter]
    fn loss_days(&self) -> u32 {
        self.inner.loss_days
    }

    #[getter]
    fn end_balance(&self) -> f64 {
        self.inner.end_balance
    }

    #[getter]
    fn max_drawdown(&self) -> f64 {
        self.inner.max_drawdown
    }

    #[getter]
    fn max_drawdown_percent(&self) -> f64 {
        self.inner.max_drawdown_percent
    }

    #[getter]
    fn total_net_pnl(&self) -> f64 {
        self.inner.total_net_pnl
    }

    #[getter]
    fn total_commission(&self) -> f64 {
        self.inner.total_commission
    }

    #[getter]
    fn total_slippage(&self) -> f64 {
        self.inner.total_slippage
    }

    #[getter]
    fn total_turnover(&self) -> f64 {
        self.inner.total_turnover
    }

    #[getter]
    fn total_trade_count(&self) -> u32 {
        self.inner.total_trade_count
    }

    #[getter]
    fn daily_net_pnl(&self) -> f64 {
        self.inner.daily_net_pnl
    }

    #[getter]
    fn daily_commission(&self) -> f64 {
        self.inner.daily_commission
    }

    #[getter]
    fn daily_slippage(&self) -> f64 {
        self.inner.daily_slippage
    }

    #[getter]
    fn daily_turnover(&self) -> f64 {
        self.inner.daily_turnover
    }

    #[getter]
    fn daily_trade_count(&self) -> f64 {
        self.inner.daily_trade_count
    }

    #[getter]
    fn daily_return(&self) -> f64 {
        self.inner.daily_return
    }

    #[getter]
    fn return_std(&self) -> f64 {
        self.inner.return_std
    }

    #[getter]
    fn sharpe_ratio(&self) -> f64 {
        self.inner.sharpe_ratio
    }

    #[getter]
    fn return_mean(&self) -> f64 {
        self.inner.return_mean
    }

    /// Convert to a Python dict for convenient access.
    fn to_dict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);

        dict.set_item("start_date", &self.inner.start_date)?;
        dict.set_item("end_date", &self.inner.end_date)?;
        dict.set_item("total_days", self.inner.total_days)?;
        dict.set_item("profit_days", self.inner.profit_days)?;
        dict.set_item("loss_days", self.inner.loss_days)?;
        dict.set_item("end_balance", self.inner.end_balance)?;
        dict.set_item("max_drawdown", self.inner.max_drawdown)?;
        dict.set_item("max_drawdown_percent", self.inner.max_drawdown_percent)?;
        dict.set_item("total_net_pnl", self.inner.total_net_pnl)?;
        dict.set_item("total_commission", self.inner.total_commission)?;
        dict.set_item("total_slippage", self.inner.total_slippage)?;
        dict.set_item("total_turnover", self.inner.total_turnover)?;
        dict.set_item("total_trade_count", self.inner.total_trade_count)?;
        dict.set_item("daily_net_pnl", self.inner.daily_net_pnl)?;
        dict.set_item("daily_commission", self.inner.daily_commission)?;
        dict.set_item("daily_slippage", self.inner.daily_slippage)?;
        dict.set_item("daily_turnover", self.inner.daily_turnover)?;
        dict.set_item("daily_trade_count", self.inner.daily_trade_count)?;
        dict.set_item("daily_return", self.inner.daily_return)?;
        dict.set_item("return_std", self.inner.return_std)?;
        dict.set_item("sharpe_ratio", self.inner.sharpe_ratio)?;
        dict.set_item("return_mean", self.inner.return_mean)?;

        Ok(dict.into())
    }

    fn __repr__(&self) -> String {
        format!(
            "PortfolioStatistics(start_date={}, end_date={}, total_days={}, profit_days={}, loss_days={}, \
             end_balance={:.2}, max_drawdown={:.2}, max_drawdown_percent={:.2}%, \
             total_net_pnl={:.2}, total_commission={:.2}, total_slippage={:.2}, \
             total_turnover={:.2}, total_trade_count={}, \
             daily_net_pnl={:.2}, daily_commission={:.2}, daily_slippage={:.2}, \
             daily_turnover={:.2}, daily_trade_count={:.2}, \
             daily_return={:.6}, return_std={:.6}, sharpe_ratio={:.4}, return_mean={:.6})",
            self.inner.start_date,
            self.inner.end_date,
            self.inner.total_days,
            self.inner.profit_days,
            self.inner.loss_days,
            self.inner.end_balance,
            self.inner.max_drawdown,
            self.inner.max_drawdown_percent,
            self.inner.total_net_pnl,
            self.inner.total_commission,
            self.inner.total_slippage,
            self.inner.total_turnover,
            self.inner.total_trade_count,
            self.inner.daily_net_pnl,
            self.inner.daily_commission,
            self.inner.daily_slippage,
            self.inner.daily_turnover,
            self.inner.daily_trade_count,
            self.inner.daily_return,
            self.inner.return_std,
            self.inner.sharpe_ratio,
            self.inner.return_mean,
        )
    }
}

// ---------------------------------------------------------------------------
// Registration helper (called from bindings.rs)
// ---------------------------------------------------------------------------

/// Register `PyPortfolioStatistics` with the PyO3 module.
pub fn register_portfolio_stats_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyPortfolioStatistics>()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_statistics() -> BacktestingStatistics {
        BacktestingStatistics {
            start_date: "2025-01-01".to_string(),
            end_date: "2025-01-31".to_string(),
            total_days: 22,
            profit_days: 14,
            loss_days: 8,
            end_balance: 110_000.0,
            max_drawdown: 5_000.0,
            max_drawdown_percent: 4.5,
            total_net_pnl: 10_000.0,
            total_commission: 500.0,
            total_slippage: 200.0,
            total_turnover: 1_000_000.0,
            total_trade_count: 50,
            daily_net_pnl: 454.545,
            daily_commission: 22.727,
            daily_slippage: 9.091,
            daily_turnover: 45_454.545,
            daily_trade_count: 2.273,
            daily_return: 0.0045,
            return_std: 0.012,
            sharpe_ratio: 1.85,
            return_mean: 1.134,
            // GAP 3 additions
            win_rate: 0.65,
            profit_factor: 2.5,
            avg_trade_pnl: 200.0,
            max_consecutive_wins: 5,
            max_consecutive_losses: 3,
            sortino_ratio: 2.1,
            calmar_ratio: 3.2,
            avg_winning_trade: 450.0,
            avg_losing_trade: -180.0,
            largest_winning_trade: 1200.0,
            largest_losing_trade: -500.0,
        }
    }

    #[test]
    fn test_from_statistics() {
        let stats = sample_statistics();
        let py_stats = PyPortfolioStatistics::from_statistics(stats);
        assert_eq!(py_stats.inner.total_days, 22);
        assert_eq!(py_stats.inner.profit_days, 14);
        assert_eq!(py_stats.inner.loss_days, 8);
    }

    #[test]
    fn test_getters() {
        let stats = sample_statistics();
        let py_stats = PyPortfolioStatistics::from_statistics(stats);

        assert_eq!(py_stats.start_date(), "2025-01-01");
        assert_eq!(py_stats.end_date(), "2025-01-31");
        assert_eq!(py_stats.total_days(), 22);
        assert_eq!(py_stats.profit_days(), 14);
        assert_eq!(py_stats.loss_days(), 8);
        assert!((py_stats.end_balance() - 110_000.0).abs() < 1e-10);
        assert!((py_stats.max_drawdown() - 5_000.0).abs() < 1e-10);
        assert!((py_stats.max_drawdown_percent() - 4.5).abs() < 1e-10);
        assert!((py_stats.total_net_pnl() - 10_000.0).abs() < 1e-10);
        assert!((py_stats.total_commission() - 500.0).abs() < 1e-10);
        assert!((py_stats.total_slippage() - 200.0).abs() < 1e-10);
        assert!((py_stats.total_turnover() - 1_000_000.0).abs() < 1e-10);
        assert_eq!(py_stats.total_trade_count(), 50);
        assert!((py_stats.daily_net_pnl() - 454.545).abs() < 0.01);
        assert!((py_stats.daily_commission() - 22.727).abs() < 0.01);
        assert!((py_stats.daily_slippage() - 9.091).abs() < 0.01);
        assert!((py_stats.daily_turnover() - 45_454.545).abs() < 0.01);
        assert!((py_stats.daily_trade_count() - 2.273).abs() < 0.01);
        assert!((py_stats.daily_return() - 0.0045).abs() < 1e-10);
        assert!((py_stats.return_std() - 0.012).abs() < 1e-10);
        assert!((py_stats.sharpe_ratio() - 1.85).abs() < 1e-10);
        assert!((py_stats.return_mean() - 1.134).abs() < 1e-10);
    }

    #[test]
    fn test_repr() {
        let stats = sample_statistics();
        let py_stats = PyPortfolioStatistics::from_statistics(stats);
        let repr = py_stats.__repr__();
        assert!(repr.starts_with("PortfolioStatistics("));
        assert!(repr.contains("start_date=2025-01-01"));
        assert!(repr.contains("end_date=2025-01-31"));
        assert!(repr.contains("total_days=22"));
        assert!(repr.contains("sharpe_ratio=1.8500"));
    }

    #[test]
    fn test_default_statistics() {
        let stats = BacktestingStatistics::default();
        let py_stats = PyPortfolioStatistics::from_statistics(stats);
        assert_eq!(py_stats.start_date(), "");
        assert_eq!(py_stats.end_date(), "");
        assert_eq!(py_stats.total_days(), 0);
        assert!((py_stats.end_balance() - 0.0).abs() < 1e-10);
        assert!((py_stats.sharpe_ratio() - 0.0).abs() < 1e-10);
    }
}
