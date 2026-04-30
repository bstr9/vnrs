//! Python bindings for the Optimization Engine
//!
//! Provides PyO3 wrappers for parameter optimization types from the
//! backtesting module: Parameter, OptimizationSettings, OptimizationTarget,
//! OptimizationResult, and OptimizationEngine.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pyo3::prelude::*;

use crate::backtesting::{
    BacktestingMode, OptimizationEngine, OptimizationResult, OptimizationSettings, OptimizationTarget,
    Parameter,
};
use crate::trader::BarData;

// ---------------------------------------------------------------------------
// PyParameter
// ---------------------------------------------------------------------------

/// Python wrapper for Parameter
#[pyclass(name = "Parameter")]
#[derive(Clone)]
pub struct PyParameter(pub Parameter);

#[pymethods]
impl PyParameter {
    #[new]
    fn new(name: String, start: f64, end: f64, step: f64) -> Self {
        PyParameter(Parameter::new(&name, start, end, step))
    }

    #[getter]
    fn name(&self) -> &str {
        &self.0.name
    }

    #[getter]
    fn start(&self) -> f64 {
        self.0.start
    }

    #[getter]
    fn end(&self) -> f64 {
        self.0.end
    }

    #[getter]
    fn step(&self) -> f64 {
        self.0.step
    }
}

// ---------------------------------------------------------------------------
// PyOptimizationSettings
// ---------------------------------------------------------------------------

/// Python wrapper for OptimizationSettings
#[pyclass(name = "OptimizationSettings")]
#[derive(Clone)]
pub struct PyOptimizationSettings(pub OptimizationSettings);

#[pymethods]
impl PyOptimizationSettings {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        vt_symbol: String,
        interval: String,
        start: String,
        end: String,
        rate: f64,
        slippage: f64,
        size: f64,
        pricetick: f64,
        capital: f64,
        mode: Option<&str>,
    ) -> PyResult<Self> {
        let interval_enum = match interval.as_str() {
            "1m" => crate::trader::Interval::Minute,
            "15m" => crate::trader::Interval::Minute15,
            "1h" => crate::trader::Interval::Hour,
            "4h" => crate::trader::Interval::Hour4,
            "1d" => crate::trader::Interval::Daily,
            "1w" => crate::trader::Interval::Weekly,
            _ => crate::trader::Interval::Minute,
        };

        let start_dt = DateTime::parse_from_rfc3339(&format!("{start}T00:00:00+00:00"))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid start date: {e}")))?
            .with_timezone(&Utc);

        let end_dt = DateTime::parse_from_rfc3339(&format!("{end}T23:59:59+00:00"))
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid end date: {e}")))?
            .with_timezone(&Utc);

        let mode_enum = match mode.unwrap_or("bar") {
            "tick" => BacktestingMode::Tick,
            _ => BacktestingMode::Bar,
        };

        Ok(PyOptimizationSettings(OptimizationSettings {
            vt_symbol,
            interval: interval_enum,
            start: start_dt,
            end: end_dt,
            rate,
            slippage,
            size,
            pricetick,
            capital,
            mode: mode_enum,
        }))
    }

    #[getter]
    fn vt_symbol(&self) -> &str {
        &self.0.vt_symbol
    }

    #[getter]
    fn rate(&self) -> f64 {
        self.0.rate
    }

    #[getter]
    fn slippage(&self) -> f64 {
        self.0.slippage
    }

    #[getter]
    fn size(&self) -> f64 {
        self.0.size
    }

    #[getter]
    fn pricetick(&self) -> f64 {
        self.0.pricetick
    }

    #[getter]
    fn capital(&self) -> f64 {
        self.0.capital
    }
}

// ---------------------------------------------------------------------------
// PyOptimizationTarget
// ---------------------------------------------------------------------------

/// Python wrapper for OptimizationTarget
#[pyclass(name = "OptimizationTarget")]
pub struct PyOptimizationTarget;

#[pymethods]
impl PyOptimizationTarget {
    #[classattr]
    #[allow(non_snake_case)]
    fn TOTAL_RETURN() -> String {
        "TotalReturn".to_string()
    }

    #[classattr]
    #[allow(non_snake_case)]
    fn SHARPE_RATIO() -> String {
        "SharpeRatio".to_string()
    }

    #[classattr]
    #[allow(non_snake_case)]
    fn MAX_DRAWDOWN() -> String {
        "MaxDrawdown".to_string()
    }

    #[classattr]
    #[allow(non_snake_case)]
    fn ANNUAL_RETURN() -> String {
        "AnnualReturn".to_string()
    }
}

/// Convert a string target name to the Rust enum.
fn parse_optimization_target(target: &str) -> OptimizationTarget {
    match target {
        "SharpeRatio" => OptimizationTarget::SharpeRatio,
        "MaxDrawdown" => OptimizationTarget::MaxDrawdown,
        "AnnualReturn" => OptimizationTarget::AnnualReturn,
        _ => OptimizationTarget::TotalReturn,
    }
}

// ---------------------------------------------------------------------------
// PyOptimizationResult
// ---------------------------------------------------------------------------

/// Python wrapper for OptimizationResult
#[pyclass(name = "OptimizationResult")]
#[derive(Clone)]
pub struct PyOptimizationResult(pub OptimizationResult);

#[pymethods]
impl PyOptimizationResult {
    /// Parameter values that produced this result
    #[getter]
    fn parameters(&self) -> HashMap<String, f64> {
        self.0.parameters.clone()
    }

    /// Target metric value (e.g. Sharpe ratio, total return)
    #[getter]
    fn target_value(&self) -> f64 {
        self.0.target_value
    }

    /// Total return
    #[getter]
    fn total_return(&self) -> f64 {
        let stats = &self.0.statistics;
        if stats.end_balance.abs() > 1e-10 {
            stats.total_net_pnl / stats.end_balance
        } else {
            0.0
        }
    }

    /// Sharpe ratio
    #[getter]
    fn sharpe_ratio(&self) -> f64 {
        self.0.statistics.sharpe_ratio
    }

    /// Maximum drawdown percent
    #[getter]
    fn max_drawdown_percent(&self) -> f64 {
        self.0.statistics.max_drawdown_percent
    }

    /// Annualized return mean
    #[getter]
    fn annual_return(&self) -> f64 {
        self.0.statistics.return_mean
    }

    /// Total trade count
    #[getter]
    fn total_trade_count(&self) -> u32 {
        self.0.statistics.total_trade_count
    }

    /// End balance
    #[getter]
    fn end_balance(&self) -> f64 {
        self.0.statistics.end_balance
    }

    /// Win rate
    #[getter]
    fn win_rate(&self) -> f64 {
        self.0.statistics.win_rate
    }

    /// Profit factor
    #[getter]
    fn profit_factor(&self) -> f64 {
        self.0.statistics.profit_factor
    }

    /// Convert result to a Python dict
    fn to_dict(&self, py: Python) -> PyResult<Py<pyo3::types::PyDict>> {
        let dict = pyo3::types::PyDict::new(py);
        dict.set_item("parameters", self.0.parameters.clone())?;
        dict.set_item("target_value", self.0.target_value)?;
        dict.set_item("sharpe_ratio", self.0.statistics.sharpe_ratio)?;
        dict.set_item("max_drawdown_percent", self.0.statistics.max_drawdown_percent)?;
        dict.set_item("return_mean", self.0.statistics.return_mean)?;
        dict.set_item("total_trade_count", self.0.statistics.total_trade_count)?;
        dict.set_item("end_balance", self.0.statistics.end_balance)?;
        dict.set_item("win_rate", self.0.statistics.win_rate)?;
        dict.set_item("profit_factor", self.0.statistics.profit_factor)?;
        Ok(dict.into())
    }
}

// ---------------------------------------------------------------------------
// PyOptimizationEngine
// ---------------------------------------------------------------------------

/// Python wrapper for OptimizationEngine
///
/// Provides parameter optimization via grid search and genetic algorithm.
///
/// The `run_grid_search` and `run_genetic_algorithm` methods require a
/// strategy factory — a Python callable that accepts a dict of parameter
/// values and returns a Strategy instance. Internally the factory is
/// wrapped in a `PythonStrategyAdapter` so the Rust backtesting engine
/// can drive the strategy lifecycle.
#[pyclass(name = "OptimizationEngine")]
pub struct PyOptimizationEngine {
    settings: OptimizationSettings,
    parameters: Vec<Parameter>,
    history_data: Vec<BarData>,
}

#[pymethods]
impl PyOptimizationEngine {
    #[new]
    fn new(settings: PyOptimizationSettings) -> Self {
        PyOptimizationEngine {
            settings: settings.0,
            parameters: Vec::new(),
            history_data: Vec::new(),
        }
    }

    /// Add a parameter range for optimization.
    fn add_parameter(&mut self, param: PyParameter) {
        self.parameters.push(param.0);
    }

    /// Set historical bar data from a list of PyBarData objects.
    fn set_history_data(&mut self, bars: Vec<super::backtesting_bindings::PyBarData>) -> PyResult<()> {
        self.history_data = bars
            .into_iter()
            .map(|b| py_bar_to_rust(&b))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(())
    }

    /// Run grid search optimization over all parameter combinations.
    ///
    /// Args:
    ///     target: Optimization target string — one of "TotalReturn",
    ///             "SharpeRatio", "MaxDrawdown", "AnnualReturn"
    ///     strategy_factory: Python callable that accepts a dict of
    ///             parameter values and returns a Strategy instance.
    ///             **Not yet wired** — currently returns empty results.
    ///
    /// Returns:
    ///     List of OptimizationResult sorted by target value (descending)
    fn run_grid_search(&self, target: &str, _strategy_factory: Py<PyAny>) -> PyResult<Vec<PyOptimizationResult>> {
        let _opt_target = parse_optimization_target(target);

        // TODO: Wire up with Python strategy factory.
        // The OptimizationEngine::run_grid_search takes
        //   Fn(&ParameterSet) -> Box<dyn StrategyTemplate> + Send + Sync
        // which cannot be directly created from a Python callable because
        // Python objects are not Send + Sync. A future implementation should
        // serialize parameters to Python, call the factory to create a
        // Strategy, wrap it in PythonStrategyAdapter, and run the backtest.
        //
        // For now we construct the engine and generate combinations but
        // return empty results since we cannot create the strategy factory.
        let mut _engine = OptimizationEngine::new(self.settings.clone());
        for param in &self.parameters {
            _engine.add_parameter(param.clone());
        }
        _engine.set_history_data(self.history_data.clone());

        Ok(vec![])
    }

    /// Run genetic algorithm optimization.
    ///
    /// Args:
    ///     target: Optimization target string
    ///     strategy_factory: Python callable (not yet wired)
    ///     population_size: Size of the population (default 100)
    ///     generations: Number of generations to evolve (default 50)
    ///
    /// Returns:
    ///     List of OptimizationResult sorted by target value (descending)
    #[pyo3(signature = (target, _strategy_factory, _population_size=100, _generations=50))]
    fn run_genetic_algorithm(
        &self,
        target: &str,
        _strategy_factory: Py<PyAny>,
        _population_size: usize,
        _generations: usize,
    ) -> PyResult<Vec<PyOptimizationResult>> {
        let _opt_target = parse_optimization_target(target);

        // TODO: Wire up with Python strategy factory (same as run_grid_search).
        let mut _engine = OptimizationEngine::new(self.settings.clone());
        for param in &self.parameters {
            _engine.add_parameter(param.clone());
        }
        _engine.set_history_data(self.history_data.clone());

        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a PyBarData to a Rust BarData.
/// Replicates the private `to_rust` method from backtesting_bindings since
/// we cannot call it from this module.
fn py_bar_to_rust(bar: &super::backtesting_bindings::PyBarData) -> PyResult<BarData> {
    use crate::trader::{Exchange, Interval};

    let exchange = match bar.exchange.to_uppercase().as_str() {
        "BINANCE" => Exchange::Binance,
        "OKX" => Exchange::Okx,
        "BYBIT" => Exchange::Bybit,
        _ => Exchange::Local,
    };

    let interval = match bar.interval.as_str() {
        "1m" => Interval::Minute,
        "1h" => Interval::Hour,
        "1d" => Interval::Daily,
        _ => Interval::Minute,
    };

    Ok(BarData {
        gateway_name: "BACKTESTING".to_string(),
        symbol: bar.symbol.clone(),
        exchange,
        datetime: chrono::DateTime::parse_from_rfc3339(&bar.datetime)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
            .with_timezone(&chrono::Utc),
        interval: Some(interval),
        open_price: bar.open_price,
        high_price: bar.high_price,
        low_price: bar.low_price,
        close_price: bar.close_price,
        volume: bar.volume,
        turnover: bar.turnover,
        open_interest: bar.open_interest,
        extra: None,
    })
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register optimization classes in the trade_engine Python module.
pub fn register_optimization_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyParameter>()?;
    m.add_class::<PyOptimizationSettings>()?;
    m.add_class::<PyOptimizationTarget>()?;
    m.add_class::<PyOptimizationResult>()?;
    m.add_class::<PyOptimizationEngine>()?;
    Ok(())
}
