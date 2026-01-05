//! Python bindings for Backtesting Engine
//!
//! Allows Python strategies to be backtested using the Rust engine

use chrono::{DateTime, Utc};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;

use crate::backtesting::{BacktestingEngine, BacktestingMode, BacktestingStatistics};
use crate::trader::{BarData, Direction, Exchange, Interval, Offset, OrderRequest, OrderType};

use std::cell::UnsafeCell;

/// Python wrapper for BacktestingEngine
#[pyclass]
pub struct PyBacktestingEngine {
    engine: UnsafeCell<BacktestingEngine>,
}

unsafe impl Send for PyBacktestingEngine {}
unsafe impl Sync for PyBacktestingEngine {}

#[pymethods]
impl PyBacktestingEngine {
    #[new]
    fn new() -> Self {
        Self {
            engine: UnsafeCell::new(BacktestingEngine::new()),
        }
    }

    /// Clear all backtesting data
    fn clear_data(&self) {
        unsafe { (*self.engine.get()).clear_data() };
    }

    /// Set backtesting parameters
    #[pyo3(signature = (
        vt_symbol,
        interval,
        start,
        end,
        rate,
        slippage,
        size,
        pricetick,
        capital,
        mode="bar"
    ))]
    fn set_parameters(
        &self,
        vt_symbol: String,
        interval: String,
        start: &str,
        end: &str,
        rate: f64,
        slippage: f64,
        size: f64,
        pricetick: f64,
        capital: f64,
        mode: Option<&str>,
    ) -> PyResult<()> {
        let engine = unsafe { &mut *self.engine.get() };

        // Parse interval
        let interval_enum = match interval.as_str() {
            "1m" => Interval::Minute,
            "15m" => Interval::Minute15,
            "1h" => Interval::Hour,
            "4h" => Interval::Hour4,
            "1d" => Interval::Daily,
            "1w" => Interval::Weekly,
            _ => Interval::Minute,
        };

        // Parse datetime
        let start_dt = DateTime::parse_from_rfc3339(&format!("{}T00:00:00+00:00", start))
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid start date: {}", e))
            })?
            .with_timezone(&Utc);

        let end_dt = DateTime::parse_from_rfc3339(&format!("{}T23:59:59+00:00", end))
            .map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("Invalid end date: {}", e))
            })?
            .with_timezone(&Utc);

        // Parse mode
        let mode_enum = match mode.unwrap_or("bar") {
            "tick" => BacktestingMode::Tick,
            _ => BacktestingMode::Bar,
        };

        engine.set_parameters(
            vt_symbol,
            interval_enum,
            start_dt,
            end_dt,
            rate,
            slippage,
            size,
            pricetick,
            capital,
            mode_enum,
        );

        Ok(())
    }

    /// Set history data from Python list of bars
    fn set_history_data(&self, bars: Vec<PyBarData>) -> PyResult<()> {
        let engine = unsafe { &mut *self.engine.get() };
        let rust_bars: Vec<BarData> = bars
            .into_iter()
            .map(|b| b.to_rust())
            .collect::<PyResult<Vec<_>>>()?;
        engine.set_history_data(rust_bars);
        Ok(())
    }

    /// Load historical data (placeholder - in production would load from database)
    fn load_data(&self) -> PyResult<()> {
        // In real implementation, this would load from database
        Ok(())
    }

    /// Get current position
    fn get_position(&self) -> f64 {
        unsafe { (*self.engine.get()).get_position() }
    }

    /// Calculate backtesting result
    fn calculate_result(&self, py: Python) -> PyResult<Py<PyDict>> {
        let engine = unsafe { &mut *self.engine.get() };
        let result = engine.calculate_result();
        let dict = PyDict::new(py);

        dict.set_item("start_capital", result.start_capital)?;
        dict.set_item("end_capital", result.end_capital)?;
        dict.set_item("total_return", result.total_return)?;
        dict.set_item("annual_return", result.annual_return)?;
        dict.set_item("max_drawdown", result.max_drawdown)?;
        dict.set_item("max_drawdown_percent", result.max_drawdown_percent)?;
        dict.set_item("sharpe_ratio", result.sharpe_ratio)?;
        dict.set_item("total_trade_count", result.total_trade_count)?;

        Ok(dict.into())
    }

    /// Calculate statistics
    #[pyo3(signature = (output=true))]
    fn calculate_statistics(&self, output: Option<bool>) -> PyResult<PyBacktestingStatistics> {
        let engine = unsafe { &mut *self.engine.get() };
        let stats = engine.calculate_statistics(output.unwrap_or(true));
        Ok(PyBacktestingStatistics { inner: stats })
    }

    /// Add strategy
    fn add_strategy(
        &self,
        _py: Python,
        strategy_class: PyObject,
        strategy_name: String,
        vt_symbols: Vec<String>,
        _setting: PyObject,
    ) -> PyResult<()> {
        use crate::python::strategy_adapter::PythonStrategyAdapter;

        let adapter =
            PythonStrategyAdapter::from_py_object(strategy_class, strategy_name, vt_symbols);

        unsafe { (*self.engine.get()).add_strategy(Box::new(adapter)) };
        Ok(())
    }

    /// Run backtesting
    fn run_backtesting(&self, _py: Python) -> PyResult<()> {
        // We need to run async code in sync context for Python
        // This is a simplified approach; proper async support would be better
        let rt = tokio::runtime::Runtime::new().unwrap();
        let engine_ptr = self.engine.get();
        // unsafe block usually within async/await boundaries is tricky,
        // but here block_on is synchronous.
        // We need to pass the raw pointer or reference ref?
        // Since we are blocking, self is alive.

        rt.block_on(async {
            // DANGER: We are creating &mut from UnsafeCell
            let engine = unsafe { &mut *engine_ptr };
            engine
                .run_backtesting()
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
        })?;
        Ok(())
    }

    /// Send order
    #[pyo3(signature = (strategy, direction, offset, price, volume, stop=false, lock=false, net=false))]
    fn send_order(
        &self,
        strategy: PyObject,
        direction: &Bound<'_, PyAny>,
        offset: &Bound<'_, PyAny>,
        price: f64,
        volume: f64,
        stop: bool,
        lock: bool,
        net: bool,
    ) -> PyResult<Vec<String>> {
        let engine = unsafe { &mut *self.engine.get() };

        // Parse direction
        let direction_str = direction
            .getattr("value")?
            .extract::<String>()
            .or_else(|_| direction.getattr("name")?.extract::<String>())?;

        let direction_enum = match direction_str.as_str() {
            "LONG" => Direction::Long,
            "SHORT" => Direction::Short,
            "NET" => Direction::Net,
            _ => Direction::Long,
        };

        // Parse offset
        let offset_str = offset
            .getattr("value")?
            .extract::<String>()
            .or_else(|_| offset.getattr("name")?.extract::<String>())?;

        let offset_enum = match offset_str.as_str() {
            "OPEN" => Offset::Open,
            "CLOSE" => Offset::Close,
            "CLOSETODAY" => Offset::CloseToday,
            "CLOSEYESTERDAY" => Offset::CloseYesterday,
            _ => Offset::Open,
        };

        let req = OrderRequest {
            symbol: engine
                .get_vt_symbol()
                .split('.')
                .next()
                .unwrap_or("")
                .to_string(),
            exchange: Exchange::Binance,
            direction: direction_enum,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: offset_enum,
            reference: "VNPY_STRATEGY".to_string(),
        };

        if stop {
            let vt_orderid = engine.send_stop_order(req);
            Ok(vec![vt_orderid])
        } else {
            let vt_orderid = engine.send_limit_order(req);
            Ok(vec![vt_orderid])
        }
    }

    /// Cancel order
    fn cancel_order(&self, _strategy: PyObject, vt_orderid: String) {
        unsafe { (*self.engine.get()).cancel_order(&vt_orderid) };
    }

    /// Write log
    fn write_log(&self, msg: String, _strategy: Option<PyObject>) {
        println!("[Strategy Log] {}", msg);
    }

    /// Load bar data
    fn load_bar(
        &self,
        vt_symbol: String,
        days: i32,
        interval: Bound<'_, PyAny>,
        _callback: Option<PyObject>,
        _use_database: bool,
    ) -> PyResult<Vec<PyObject>> {
        let interval_str = if let Ok(s) = interval.extract::<String>() {
            s
        } else {
            // Try to get .value property
            if let Ok(v) = interval.getattr("value") {
                v.extract::<String>().unwrap_or("1m".to_string())
            } else {
                "1m".to_string()
            }
        };

        println!(
            "load_bar: vt_symbol={}, days={}, interval={}",
            vt_symbol, days, interval_str
        );
        // Placeholder: return empty list
        Ok(Vec::new())
    }

    /// Put strategy event
    fn put_strategy_event(&self, _strategy: PyObject) {
        // Placeholder
    }
}

/// Python wrapper for BarData
#[pyclass]
#[derive(Clone)]
pub struct PyBarData {
    #[pyo3(get, set)]
    pub gateway_name: String,
    #[pyo3(get, set)]
    pub symbol: String,
    #[pyo3(get, set)]
    pub exchange: String,

    pub datetime: String, // Internal storage

    #[pyo3(get, set)]
    pub interval: String,
    #[pyo3(get, set)]
    pub open_price: f64,
    #[pyo3(get, set)]
    pub high_price: f64,
    #[pyo3(get, set)]
    pub low_price: f64,
    #[pyo3(get, set)]
    pub close_price: f64,
    #[pyo3(get, set)]
    pub volume: f64,
}

#[pymethods]
impl PyBarData {
    #[new]
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: String,
        datetime: String,
        interval: String,
        open_price: f64,
        high_price: f64,
        low_price: f64,
        close_price: f64,
        volume: f64,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            datetime,
            interval,
            open_price,
            high_price,
            low_price,
            close_price,
            volume,
        }
    }

    #[getter]
    fn get_datetime<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let dt_cls = py.import("datetime")?.getattr("datetime")?;
        match dt_cls.call_method1("fromisoformat", (&self.datetime,)) {
            Ok(dt) => Ok(dt),
            Err(_) => Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid datetime format: {}",
                self.datetime
            ))),
        }
    }

    #[setter]
    fn set_datetime(&mut self, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(s) = value.extract::<String>() {
            self.datetime = s;
        } else {
            self.datetime = value.call_method0("isoformat")?.extract::<String>()?;
        }
        Ok(())
    }
}

impl PyBarData {
    fn to_rust(&self) -> PyResult<BarData> {
        let exchange = match self.exchange.to_uppercase().as_str() {
            "BINANCE" => Exchange::Binance,
            "OKX" => Exchange::Global,   // Exchange::Okx not defined
            "BYBIT" => Exchange::Global, // Exchange::Bybit not defined
            _ => Exchange::Local,
        };

        let interval = match self.interval.as_str() {
            "1m" => Interval::Minute,
            "1h" => Interval::Hour,
            "1d" => Interval::Daily,
            _ => Interval::Minute,
        };

        Ok(BarData {
            gateway_name: "BACKTESTING".to_string(),
            symbol: self.symbol.clone(),
            exchange,
            datetime: chrono::DateTime::parse_from_rfc3339(&self.datetime)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
                .with_timezone(&chrono::Utc),
            interval: Some(interval),
            open_price: self.open_price,
            high_price: self.high_price,
            low_price: self.low_price,
            close_price: self.close_price,
            volume: self.volume,
            turnover: 0.0,
            open_interest: 0.0,
            extra: None,
        })
    }
}

/// Python wrapper for BacktestingStatistics
#[pyclass]
pub struct PyBacktestingStatistics {
    inner: BacktestingStatistics,
}

#[pymethods]
impl PyBacktestingStatistics {
    #[getter]
    fn start_date(&self) -> String {
        self.inner.start_date.clone()
    }

    #[getter]
    fn end_date(&self) -> String {
        self.inner.end_date.clone()
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
    fn sharpe_ratio(&self) -> f64 {
        self.inner.sharpe_ratio
    }

    #[getter]
    fn return_mean(&self) -> f64 {
        self.inner.return_mean
    }

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
        dict.set_item("sharpe_ratio", self.inner.sharpe_ratio)?;
        dict.set_item("return_mean", self.inner.return_mean)?;

        Ok(dict.into())
    }
}

/// Register backtesting module
pub fn register_backtesting_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBacktestingEngine>()?;
    m.add_class::<PyBarData>()?;
    m.add_class::<PyBacktestingStatistics>()?;
    Ok(())
}
