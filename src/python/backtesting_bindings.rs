//! Python bindings for Backtesting Engine
//!
//! Allows Python strategies to be backtested using the Rust engine

use chrono::{DateTime, Utc};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::backtesting::{BacktestingEngine, BacktestingMode, BacktestingStatistics};
use crate::python::{OrderFactory, PortfolioFacade, PortfolioState, PyRiskManager};
use crate::trader::{BarData, Direction, Exchange, Interval, Offset, OrderRequest, OrderType};

use std::sync::{Arc, Mutex};

use pyo3::types::PyAnyMethods;

/// Python wrapper for BacktestingEngine
#[pyclass]
pub struct PyBacktestingEngine {
    engine: Mutex<BacktestingEngine>,
    runtime: tokio::runtime::Runtime,
    portfolio_state: Arc<Mutex<PortfolioState>>,
    risk_manager: Option<Py<PyRiskManager>>,
}

#[pymethods]
impl PyBacktestingEngine {
    #[new]
    fn new() -> PyResult<Self> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create tokio runtime: {}", e)))?;
        Ok(Self {
            engine: Mutex::new(BacktestingEngine::new()),
            runtime: rt,
            portfolio_state: Arc::new(Mutex::new(PortfolioState::default())),
            risk_manager: None,
        })
    }

    /// Set the risk manager for order validation
    fn set_risk_manager(slf: &Bound<'_, Self>, risk_manager: Py<PyRiskManager>) {
        slf.borrow_mut().risk_manager = Some(risk_manager);
    }

    /// Clear all backtesting data
    fn clear_data(&self) {
        self.engine.lock().unwrap_or_else(|e| e.into_inner()).clear_data();
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
    #[allow(clippy::too_many_arguments)]
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
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());

        // Parse datetime
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
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let rust_bars: Vec<BarData> = bars
            .into_iter()
            .map(|b| b.to_rust())
            .collect::<PyResult<Vec<_>>>()?;
        engine.set_history_data(rust_bars);
        Ok(())
    }

    /// Load historical data from CSV or database
    fn load_data(&self, py: Python) -> PyResult<()> {
        py.detach(|| {
            let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
            let rt = tokio::runtime::Handle::current();
            rt.block_on(engine.load_data())
                .map_err(pyo3::exceptions::PyRuntimeError::new_err)
        })
    }

    /// Get current position (signed quantity)
    fn get_position(&self) -> f64 {
        self.engine.lock().unwrap_or_else(|e| e.into_inner()).get_pos()
    }

    /// Calculate backtesting result
    fn calculate_result(&self, py: Python) -> PyResult<Py<PyDict>> {
        let engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
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
        let engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let stats = engine.calculate_statistics(output.unwrap_or(true));
        Ok(PyBacktestingStatistics { inner: stats })
    }

    /// Add strategy from an already-instantiated Python object
    fn add_strategy(
        slf: &Bound<'_, Self>,
        py: Python,
        strategy_instance: Py<PyAny>,
        strategy_name: String,
        vt_symbols: Vec<String>,
    ) -> PyResult<()> {
        use crate::python::strategy_adapter::PythonStrategyAdapter;

        // Inject engine reference (for buy/sell/short/cover convenience methods)
        let engine_ref: Py<PyAny> = slf.clone().into_any().unbind();
        strategy_instance.setattr(py, "engine", engine_ref.clone_ref(py))?;

        // Inject PortfolioFacade
        let portfolio_facade = PortfolioFacade::from_state(slf.borrow().portfolio_state.clone());
        let portfolio_py = Py::new(py, portfolio_facade)?;
        strategy_instance.setattr(py, "portfolio", portfolio_py)?;

        // Inject OrderFactory with engine reference to PyBacktestingEngine
        let order_factory = OrderFactory::from_engine(engine_ref, "");
        let factory_py = Py::new(py, order_factory)?;
        strategy_instance.setattr(py, "order_factory", factory_py)?;

        let adapter =
            PythonStrategyAdapter::from_py_object(strategy_instance, strategy_name, vt_symbols);

        slf.borrow()
            .engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .add_strategy(Box::new(adapter));
        Ok(())
    }

    /// Add strategy by instantiating a class with vnpy CtaTemplate signature
    /// vnpy CtaTemplate: __init__(self, engine, strategy_name, vt_symbol, setting)
    fn add_strategy_with_class(
        slf: &Bound<'_, Self>,
        py: Python,
        strategy_class: Py<PyAny>,
        strategy_name: String,
        vt_symbols: Vec<String>,
        setting: Py<PyDict>,
    ) -> PyResult<()> {
        use crate::python::strategy_adapter::PythonStrategyAdapter;

        // Use load_from_file logic but with a class instead of file path
        // vnpy signature: (engine, strategy_name, vt_symbol, setting)
        let vt_symbol = vt_symbols.first().cloned().unwrap_or_default();
        
        let py_instance = strategy_class.call1(py, (
            py.None(),  // engine placeholder (vnpy expects engine object)
            strategy_name.clone(),
            vt_symbol,
            setting.bind(py),
        )).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(
                format!("Failed to create strategy instance with vnpy signature (engine, strategy_name, vt_symbol, setting): {}", e)
            )
        })?;

        // Inject engine reference (for buy/sell/short/cover convenience methods)
        let engine_ref: Py<PyAny> = slf.clone().into_any().unbind();
        py_instance.setattr(py, "engine", engine_ref.clone_ref(py))?;

        // Inject PortfolioFacade
        let portfolio_facade = PortfolioFacade::from_state(slf.borrow().portfolio_state.clone());
        let portfolio_py = Py::new(py, portfolio_facade)?;
        py_instance.setattr(py, "portfolio", portfolio_py)?;

        // Inject OrderFactory with engine reference to PyBacktestingEngine
        let order_factory = OrderFactory::from_engine(engine_ref, "");
        let factory_py = Py::new(py, order_factory)?;
        py_instance.setattr(py, "order_factory", factory_py)?;

        let adapter = PythonStrategyAdapter::from_py_object(
            py_instance,
            strategy_name,
            vt_symbols,
        );

        slf.borrow()
            .engine
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .add_strategy(Box::new(adapter));
        Ok(())
    }

    /// Run backtesting
    fn run_backtesting(&self, _py: Python) -> PyResult<()> {
        let mut engine_guard = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        self.runtime.block_on(async {
            engine_guard
                .run_backtesting()
                .await
                .map_err(pyo3::exceptions::PyRuntimeError::new_err)
        })?;
        Ok(())
    }

    /// Send order
    #[pyo3(signature = (_strategy, direction, offset, price, volume, stop=false, _lock=false, _net=false))]
    #[allow(clippy::too_many_arguments)]
    fn send_order(
        &self,
        py: Python,
        _strategy: Py<PyAny>,
        direction: &Bound<'_, PyAny>,
        offset: &Bound<'_, PyAny>,
        price: f64,
        volume: f64,
        stop: bool,
        _lock: bool,
        _net: bool,
    ) -> PyResult<Vec<String>> {
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());

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
            post_only: false,
            reduce_only: false,
        };

        // Risk manager check
        if let Some(ref risk_manager) = self.risk_manager {
            let dir_str = match direction_enum {
                Direction::Long => "LONG",
                Direction::Short => "SHORT",
                Direction::Net => "NET",
            };
            let off_str = match offset_enum {
                Offset::None => "NONE",
                Offset::Open => "OPEN",
                Offset::Close => "CLOSE",
                Offset::CloseToday => "CLOSE_TODAY",
                Offset::CloseYesterday => "CLOSE_YESTERDAY",
            };
            let vt_symbol = engine.get_vt_symbol().to_string();
            let result = risk_manager.borrow(py).check_order(
                vt_symbol.as_str(),
                dir_str,
                off_str,
                price,
                volume,
                "LIMIT",
                0.0,
                0,
            )?;
            if !result.is_approved() {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Order rejected by risk manager: {}",
                    result.reason().unwrap_or("unknown reason")
                )));
            }
        }

        if stop {
            let vt_orderid = engine.send_stop_order(req);
            Ok(vec![vt_orderid])
        } else {
            let vt_orderid = engine.send_limit_order(req);
            Ok(vec![vt_orderid])
        }
    }

    /// Buy (long open) — convenience method matching Strategy.buy() signature
    fn buy(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let symbol = vt_symbol.split('.').next().unwrap_or(&vt_symbol);
        let req = OrderRequest {
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: Offset::Open,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
        };
        let vt_orderid = engine.send_limit_order(req);
        if vt_orderid.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![vt_orderid])
        }
    }

    /// Sell (long close) — convenience method matching Strategy.sell() signature
    fn sell(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let symbol = vt_symbol.split('.').next().unwrap_or(&vt_symbol);
        let req = OrderRequest {
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: Offset::Close,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
        };
        let vt_orderid = engine.send_limit_order(req);
        if vt_orderid.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![vt_orderid])
        }
    }

    /// Short (short open, futures only) — convenience method matching Strategy.short() signature
    fn short(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let symbol = vt_symbol.split('.').next().unwrap_or(&vt_symbol);
        let req = OrderRequest {
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: Offset::Open,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
        };
        let vt_orderid = engine.send_limit_order(req);
        if vt_orderid.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![vt_orderid])
        }
    }

    /// Cover (short close, futures only) — convenience method matching Strategy.cover() signature
    fn cover(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let mut engine = self.engine.lock().unwrap_or_else(|e| e.into_inner());
        let symbol = vt_symbol.split('.').next().unwrap_or(&vt_symbol);
        let req = OrderRequest {
            symbol: symbol.to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: Offset::Close,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
        };
        let vt_orderid = engine.send_limit_order(req);
        if vt_orderid.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![vt_orderid])
        }
    }

    /// Get current position quantity for a symbol
    fn get_pos(&self, _vt_symbol: Option<&str>) -> PyResult<f64> {
        Ok(self.engine.lock().unwrap_or_else(|e| e.into_inner()).get_pos())
    }

    /// Write log — matches Strategy.write_log() signature (single msg argument)
    fn write_log(&self, msg: String) {
        println!("[Strategy Log] {}", msg);
    }

    /// Send email — matches Strategy.send_email() signature (no-op in backtesting)
    fn send_email(&self, _msg: String) {
        // No-op in backtesting
    }

    /// Cancel order — matches Strategy.cancel_order() signature
    fn cancel_order(&self, vt_orderid: String) {
        self.engine.lock().unwrap_or_else(|e| e.into_inner()).cancel_order(&vt_orderid);
    }

    /// Load bar data
    fn load_bar(
        &self,
        vt_symbol: String,
        days: i32,
        interval: Bound<'_, PyAny>,
        _callback: Option<Py<PyAny>>,
        _use_database: bool,
    ) -> PyResult<Vec<Py<PyAny>>> {
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
    fn put_strategy_event(&self, _strategy: Py<PyAny>) {
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
    #[allow(clippy::too_many_arguments)]
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
