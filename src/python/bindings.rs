use crate::python::{OrderFactory, PyInstrument, PyOrder, PythonEngine, PythonEngineBridge, PyStrategyContext, Strategy, PyArrayManager, MessageBus};
use crate::strategy::StrategyEngine;
use crate::trader::constant::{Direction, Offset, OrderType};
use crate::trader::alert::AlertLevel;
use crate::trader::MainEngine;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

/// Handle to a live StrategyEngine, allowing Python code to reference
/// the Rust-side strategy engine created by the GUI application.
#[pyclass]
pub struct StrategyEngineHandle {
    pub(crate) inner: Arc<StrategyEngine>,
}

impl StrategyEngineHandle {
    pub fn new(engine: Arc<StrategyEngine>) -> Self {
        StrategyEngineHandle { inner: engine }
    }
}

#[pymethods]
impl StrategyEngineHandle {
    /// Get all strategy names currently registered
    fn get_all_strategy_names(&self) -> PyResult<Vec<String>> {
        Ok(self.inner.get_all_strategy_names())
    }

    /// Reset a strategy: clears state and returns to Initialized.
    ///
    /// If the strategy is currently trading, it will be stopped first.
    /// After reset, the strategy can be started again with `start_strategy()`.
    fn reset_strategy(&self, strategy_name: String) -> PyResult<()> {
        let engine = self.inner.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.block_on(async {
                    engine.reset_strategy(&strategy_name).await
                }).map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "Failed to reset strategy '{}': {}",
                        strategy_name, e
                    ))
                })
            }
            Err(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "No tokio runtime available — must be called from within an async context",
            )),
        }
    }

    /// Restart a strategy: full stop → init → start cycle.
    fn restart_strategy(&self, strategy_name: String) -> PyResult<()> {
        let engine = self.inner.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.block_on(async {
                    engine.restart_strategy(&strategy_name).await
                }).map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!(
                        "Failed to restart strategy '{}': {}",
                        strategy_name, e
                    ))
                })
            }
            Err(_) => Err(pyo3::exceptions::PyRuntimeError::new_err(
                "No tokio runtime available — must be called from within an async context",
            )),
        }
    }
}

/// Create main engine from Python
#[pyfunction]
fn create_main_engine(py: Python) -> PyResult<Py<PyAny>> {
    let wrapper = PythonEngineWrapper::new()?;
    Ok(Py::new(py, wrapper)?.into_any())
}

/// Run the event loop
#[pyfunction]
fn run_event_loop() -> PyResult<()> {
    // In a real implementation, we would run the main trading event loop
    println!("Event loop running...");
    Ok(())
}

/// Add a Python strategy to the live StrategyEngine for real-time trading.
///
/// This function creates a `PythonStrategyAdapter` wrapping the Python strategy
/// and registers it with the live StrategyEngine so it receives market data events
/// (tick/bar/order/trade) through the normal StrategyEngine event routing path.
///
/// Args:
///     strategy: Python Strategy instance
///     strategy_engine: StrategyEngineHandle obtained from the application
///     setting: Optional dict of strategy settings
///
/// Returns:
///     List of strategy names that were added
#[pyfunction]
#[pyo3(signature = (strategy, strategy_engine, _setting=None))]
fn add_strategy_live(
    py: Python,
    strategy: Bound<'_, Strategy>,
    strategy_engine: &StrategyEngineHandle,
    _setting: Option<&Bound<'_, PyDict>>,
) -> PyResult<Vec<String>> {
    use crate::python::strategy_adapter::PythonStrategyAdapter;
    use crate::strategy::base::StrategySetting;

    let strategy_ref = strategy.borrow();
    let strategy_name = strategy_ref.strategy_name.clone();
    let vt_symbols = strategy_ref.vt_symbols.clone();
    drop(strategy_ref);

    // Create the adapter from the Python strategy object
    let py_obj: Py<PyAny> = strategy.clone().unbind().into_any();
    let adapter = PythonStrategyAdapter::from_py_object(
        py_obj,
        strategy_name.clone(),
        vt_symbols.clone(),
    );

    // Parse settings dict
    let strat_setting = StrategySetting::new();

    // Add to the live StrategyEngine
    let engine = strategy_engine.inner.clone();
    let name = strategy_name.clone();
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.block_on(async {
                engine.add_python_strategy(adapter, strat_setting).await
            }).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Failed to add strategy '{}' to live engine: {}",
                    name, e
                ))
            })?;
        }
        Err(_) => {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                "No tokio runtime available — must be called from within an async context",
            ));
        }
    }

    // Inject PyStrategyContext sharing the live StrategyEngine's caches
    if let Some((tick_cache, bar_cache, historical_bars)) =
        strategy_engine.inner.get_context_caches(&strategy_name)
    {
        let context = PyStrategyContext::from_caches(tick_cache, bar_cache, historical_bars);
        let context_py = Py::new(py, context)?;
        strategy.setattr("context", context_py)?;
    }

    tracing::info!("Python strategy '{}' added to live StrategyEngine", strategy_name);
    Ok(vec![strategy_name])
}

/// Python module for the trading engine
#[pymodule]
fn trade_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Strategy>()?;
    m.add_class::<PythonEngineWrapper>()?;
    m.add_class::<StrategyEngineHandle>()?;
    m.add_class::<PyOrder>()?;
    m.add_class::<OrderFactory>()?;
    m.add_class::<PyInstrument>()?;
    m.add_class::<PyStrategyContext>()?;
    m.add_class::<PyArrayManager>()?;
    m.add_class::<PyAlertMessage>()?;
    m.add_function(wrap_pyfunction!(create_main_engine, m)?)?;
    m.add_function(wrap_pyfunction!(run_event_loop, m)?)?;
    m.add_function(wrap_pyfunction!(add_strategy_live, m)?)?;

    // Direction class with LONG/SHORT/NET string attributes (vnpy compatible).
    // Uses a simple Python class so Direction.LONG == "LONG" works for
    // string comparisons (the backtesting engine passes directions as strings).
    let py = m.py();
    // Create Direction class via Python exec in the module's dict
    // We use a temporary dict, exec into it, then copy the result
    let ns = pyo3::types::PyDict::new(py);
    ns.set_item("Direction", py.None())?;  // placeholder
    let direction_code = "class Direction:\n    LONG = 'LONG'\n    SHORT = 'SHORT'\n    NET = 'NET'\n";
    let exec_fn = py.import("builtins")?.getattr("exec")?;
    let code_obj = py.import("builtins")?.getattr("compile")?.call1((direction_code, "<string>", "exec"))?;
    exec_fn.call1((code_obj, &ns))?;
    let direction_cls = ns.get_item("Direction")?
        .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyException, _>("Failed to create Direction class"))?;
    m.add("Direction", direction_cls)?;

    // Module-level LONG/SHORT/NET string constants for direct import
    m.add("LONG", "LONG")?;
    m.add("SHORT", "SHORT")?;
    m.add("NET", "NET")?;

    // CtaStrategy alias for backward compatibility (vnpy CtaTemplate compat)
    // Strategies do `from trade_engine import CtaStrategy` — this maps to Strategy
    let strategy_cls: Py<PyAny> = m.getattr("Strategy")?.into();
    m.add("CtaStrategy", strategy_cls.clone_ref(py))?;

    // Register backtesting module
    crate::python::backtesting_bindings::register_backtesting_module(m)?;

    // Register data types module
    crate::python::data_types::register_data_types_module(m)?;

    // Register portfolio facade module
    crate::python::portfolio::register_portfolio_module(m)?;

    // Register message bus module
    crate::python::message_bus::register_message_bus_module(m)?;

    // Register risk manager module
    crate::python::risk_manager::register_risk_module(m)?;

    // Register sync bar generator module
    crate::python::sync_bar_bindings::register_sync_bar_module(m)?;

    // Register array manager module
    crate::python::arraymanager::register_arraymanager_module(m)?;

    // Register alpha research module (requires both python and alpha features)
    #[cfg(all(feature = "python", feature = "alpha"))]
    crate::python::alpha_bindings::register_alpha_module(m)?;

    // Register deprecated strategy classes (kept for backward compatibility)
    #[allow(deprecated)]
    {
        crate::python::strategy_bindings::register_strategy_module(m)?;
    }

    // Register offset converter module
    crate::python::offset_converter::register_offset_converter_module(m)?;

    // Register stop order engine module
    crate::python::stop_order_engine::register_stop_order_engine_module(m)?;

    // Register bracket order engine module
    crate::python::bracket_order_engine::register_bracket_order_engine_module(m)?;

    // Register order emulator module
    crate::python::order_emulator::register_order_emulator_module(m)?;

    Ok(())
}

/// Python-accessible alert message
#[pyclass]
#[derive(Clone)]
pub struct PyAlertMessage {
    /// Alert severity level (Info, Warning, Critical)
    #[pyo3(get)]
    pub level: String,
    /// Short title/summary
    #[pyo3(get)]
    pub title: String,
    /// Detailed message body
    #[pyo3(get)]
    pub body: String,
    /// Source engine/gateway
    #[pyo3(get)]
    pub source: String,
    /// ISO 8601 timestamp
    #[pyo3(get)]
    pub timestamp: String,
    /// Related trading symbol (if any)
    #[pyo3(get)]
    pub vt_symbol: Option<String>,
}

impl PyAlertMessage {
    fn from_toast(toast: &crate::trader::Toast) -> Self {
        let level = match toast.level {
            AlertLevel::Info => "Info",
            AlertLevel::Warning => "Warning",
            AlertLevel::Critical => "Critical",
        };
        Self {
            level: level.to_string(),
            title: toast.title.clone(),
            body: toast.body.clone(),
            source: toast.source.clone(),
            timestamp: toast.timestamp.to_rfc3339(),
            vt_symbol: toast.vt_symbol.clone(),
        }
    }
}

#[pymethods]
impl PyAlertMessage {
    fn __repr__(&self) -> String {
        format!("AlertMessage(level='{}', title='{}', source='{}')", self.level, self.title, self.source)
    }
}

/// Wrapper for PythonEngine to make it compatible with PyO3
#[pyclass]
pub struct PythonEngineWrapper {
    inner: Arc<Mutex<PythonEngine>>,
    /// Keep MainEngine alive so the registered PythonEngineBridge continues
    /// receiving events. The bridge is owned by MainEngine.engines, so if
    /// MainEngine is dropped, event routing stops.
    #[allow(dead_code)]
    main_engine: Arc<MainEngine>,
    #[allow(dead_code)]
    rt: Runtime,
}

#[pymethods]
impl PythonEngineWrapper {
    #[new]
    fn new() -> PyResult<Self> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;

        let main_engine = MainEngine::new();
        let python_engine = PythonEngine::new_from_arc(main_engine.clone());
        let inner = Arc::new(Mutex::new(python_engine));

        // Register PythonEngineBridge with MainEngine so gateway events
        // flow: MainEngine → PythonEngineBridge.process_event() → PythonEngine.on_tick/on_bar/etc → strategy callbacks
        let bridge = PythonEngineBridge::from_shared(inner.clone());
        main_engine.add_engine(Arc::new(bridge));

        Ok(PythonEngineWrapper {
            inner,
            main_engine,
            rt,
        })
    }

    fn add_strategy(
        slf: &Bound<'_, Self>,
        py: Python,
        strategy: Bound<'_, Strategy>,
    ) -> PyResult<()> {
        let engine_ref: Py<PyAny> = slf.clone().into_any().unbind();
        slf.borrow()
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .add_strategy_py(py, strategy, engine_ref)
    }

    /// Set the live StrategyEngine for this PythonEngine.
    ///
    /// When set, Python strategies added via `add_strategy` will also be
    /// registered with the live StrategyEngine, enabling them to receive
    /// real-time market data events through the StrategyEngine's event routing.
    fn set_strategy_engine(&self, handle: &StrategyEngineHandle) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .set_strategy_engine(handle.inner.clone());
        Ok(())
    }

    fn init_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .init_strategy_py(py, &strategy_name)
    }

    fn start_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .start_strategy_py(py, &strategy_name)
    }

    fn stop_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stop_strategy_py(py, &strategy_name)
    }

    fn reset_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reset_strategy_py(py, &strategy_name)
    }

    fn restart_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .restart_strategy_py(py, &strategy_name)
    }

    fn on_tick(&self, py: Python, tick_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let tick = crate::python::data_converter::py_to_tick(py, tick_dict)?;
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_tick(py, &tick)?;
        Ok(())
    }

    fn on_bar(&self, py: Python, bar_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let bar = crate::python::data_converter::py_to_bar(py, bar_dict)?;
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_bar(py, &bar)?;
        Ok(())
    }

    fn on_trade(&self, py: Python, trade_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let trade = crate::python::data_converter::py_to_trade(py, trade_dict)?;
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_trade(py, &trade)?;
        Ok(())
    }

    fn on_order(&self, py: Python, order_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let order = crate::python::data_converter::py_to_order(py, order_dict)?;
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .on_order(py, &order)?;
        Ok(())
    }

    // Order management methods
    fn buy(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .buy(&vt_symbol, price, volume);
        Ok(result)
    }

    fn sell(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sell(&vt_symbol, price, volume);
        Ok(result)
    }

    fn short(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .short(&vt_symbol, price, volume);
        Ok(result)
    }

    fn cover(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cover(&vt_symbol, price, volume);
        Ok(result)
    }

    fn cancel_order(&self, vt_orderid: String) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel_order(&vt_orderid);
    }

    fn get_pos(&self, vt_symbol: String) -> PyResult<f64> {
        Ok(self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_pos(&vt_symbol))
    }

    fn write_log(&self, msg: String) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .write_log(&msg);
    }

    fn send_email(&self, msg: String) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send_email(&msg);
    }

    /// Send a typed order (called by PyOrder.submit()).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction_str: "BUY" or "SELL"
    ///     offset_str: "NONE", "OPEN", "CLOSE", "CLOSE_TODAY", "CLOSE_YESTERDAY"
    ///     price: Order price (0.0 for market orders)
    ///     volume: Order quantity
    ///     order_type_str: "MARKET", "LIMIT", "STOP"
    #[pyo3(signature = (vt_symbol, direction_str, offset_str, price, volume, order_type_str))]
    fn send_order_typed(
        &self,
        vt_symbol: &str,
        direction_str: &str,
        offset_str: &str,
        price: f64,
        volume: f64,
        order_type_str: &str,
    ) -> PyResult<Vec<String>> {
        let direction = match direction_str.to_uppercase().as_str() {
            "BUY" | "LONG" => Direction::Long,
            "SELL" | "SHORT" => Direction::Short,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid direction '{}'",
                    direction_str
                )));
            }
        };

        let offset = match offset_str.to_uppercase().as_str() {
            "NONE" => Offset::None,
            "OPEN" => Offset::Open,
            "CLOSE" => Offset::Close,
            "CLOSE_TODAY" => Offset::CloseToday,
            "CLOSE_YESTERDAY" => Offset::CloseYesterday,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid offset '{}'",
                    offset_str
                )));
            }
        };

        let order_type = match order_type_str.to_uppercase().as_str() {
            "MARKET" => OrderType::Market,
            "LIMIT" => OrderType::Limit,
            "STOP" => OrderType::Stop,
            "STOP_LIMIT" => OrderType::StopLimit,
            "PEGGED_BEST" => OrderType::PeggedBest,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid order_type '{}'",
                    order_type_str
                )));
            }
        };

        let result = self
            .inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send_order(vt_symbol, direction, offset, price, volume, order_type);
        Ok(result)
    }

    /// Create an OrderFactory bound to this engine.
    fn create_order_factory(slf: &Bound<'_, Self>) -> PyResult<OrderFactory> {
        let engine_ref: Py<PyAny> = slf.clone().into_any().unbind();
        Ok(OrderFactory::from_engine(engine_ref, ""))
    }

    /// Get instrument metadata for a symbol.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "btcusdt.binance")
    ///
    /// Returns:
    ///     PyInstrument if found, None otherwise
    fn get_instrument(&self, vt_symbol: String) -> PyResult<Option<PyInstrument>> {
        let contract = self.main_engine.get_contract(&vt_symbol);
        Ok(contract.map(|c| PyInstrument::from_contract_data(&c)))
    }

    /// Subscribe a strategy to market data for a symbol at runtime.
    ///
    /// Args:
    ///     strategy_name: Name of the strategy
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    fn subscribe(&self, strategy_name: String, vt_symbol: String) -> PyResult<()> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(se) = inner.get_strategy_engine() {
            let se: Arc<StrategyEngine> = (*se).clone();
            let strategy_name = strategy_name.clone();
            let vt_symbol = vt_symbol.clone();
            tokio::spawn(async move {
                if let Err(e) = se.dynamic_subscribe(&strategy_name, &vt_symbol).await {
                    tracing::error!("Failed to subscribe {}: {}", vt_symbol, e);
                }
            });
        }
        Ok(())
    }

    /// Unsubscribe a strategy from market data for a symbol at runtime.
    ///
    /// Args:
    ///     strategy_name: Name of the strategy
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    fn unsubscribe(&self, strategy_name: String, vt_symbol: String) -> PyResult<()> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(se) = inner.get_strategy_engine() {
            let se: Arc<StrategyEngine> = (*se).clone();
            let strategy_name = strategy_name.clone();
            let vt_symbol = vt_symbol.clone();
            tokio::spawn(async move {
                if let Err(e) = se.dynamic_unsubscribe(&strategy_name, &vt_symbol).await {
                    tracing::error!("Failed to unsubscribe {}: {}", vt_symbol, e);
                }
            });
        }
        Ok(())
    }

    /// Schedule a timer for a strategy.
    ///
    /// Args:
    ///     strategy_name: Name of the strategy
    ///     timer_id: Unique timer identifier within the strategy
    ///     seconds: Delay until first fire (and interval if repeat=True)
    ///     repeat: Whether the timer repeats
    #[pyo3(signature = (strategy_name, timer_id, seconds, repeat=false))]
    fn schedule_timer(&self, strategy_name: String, timer_id: String, seconds: f64, repeat: bool) -> PyResult<()> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(se) = inner.get_strategy_engine() {
            let se: Arc<StrategyEngine> = (*se).clone();
            se.schedule_timer(&strategy_name, &timer_id, seconds, repeat);
        }
        Ok(())
    }

    /// Cancel a timer for a strategy.
    ///
    /// Args:
    ///     strategy_name: Name of the strategy
    ///     timer_id: Timer identifier to cancel
    fn cancel_timer(&self, strategy_name: String, timer_id: String) -> PyResult<()> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(se) = inner.get_strategy_engine() {
            let se: Arc<StrategyEngine> = (*se).clone();
            se.cancel_timer(&strategy_name, &timer_id);
        }
        Ok(())
    }

    /// Get all active (undismissed) toast alerts.
    ///
    /// Returns a list of PyAlertMessage objects representing alerts
    /// that have been triggered but not yet dismissed by the user.
    fn get_active_toasts(&self) -> PyResult<Vec<PyAlertMessage>> {
        let toasts = self.main_engine.toast_manager().get_active_toasts();
        Ok(toasts.iter().map(PyAlertMessage::from_toast).collect())
    }

    /// Get recent toast alerts.
    ///
    /// Args:
    ///     limit: Maximum number of toasts to return (default 20)
    ///
    /// Returns a list of PyAlertMessage objects in reverse chronological order.
    #[pyo3(signature = (limit=20))]
    fn get_recent_toasts(&self, limit: usize) -> PyResult<Vec<PyAlertMessage>> {
        let toasts = self.main_engine.toast_manager().get_recent_toasts(limit);
        Ok(toasts.iter().map(PyAlertMessage::from_toast).collect())
    }

    /// Set the self-trade prevention (STP) mode.
    ///
    /// Args:
    ///     mode: One of "CancelTaker", "CancelMaker", "CancelBoth"
    fn set_stp_mode(&self, mode: String) -> PyResult<()> {
        use crate::trader::constant::StpMode;
        let stp_mode = match mode.as_str() {
            "CancelTaker" => StpMode::CancelTaker,
            "CancelMaker" => StpMode::CancelMaker,
            "CancelBoth" => StpMode::CancelBoth,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid STP mode '{}'. Must be one of: CancelTaker, CancelMaker, CancelBoth",
                    mode
                )));
            }
        };
        self.main_engine.set_stp_mode(stp_mode);
        Ok(())
    }

    /// Get the current self-trade prevention (STP) mode.
    ///
    /// Returns:
    ///     One of "CancelTaker", "CancelMaker", "CancelBoth"
    fn get_stp_mode(&self) -> PyResult<String> {
        Ok(self.main_engine.get_stp_mode().to_string())
    }

    /// Get an OffsetConverter bound to the live MainEngine's contract lookup.
    ///
    /// The returned converter uses the MainEngine's OmsEngine for contract
    /// resolution, so it knows which symbols require offset conversion
    /// (e.g., SHFE/INE futures). Position data is *not* shared — this is
    /// intended for query/preview use (e.g., checking if a symbol requires
    /// offset splitting before sending an order).
    ///
    /// Returns:
    ///     PyOffsetConverter instance
    fn offset_converter(&self) -> PyResult<crate::python::offset_converter::PyOffsetConverter> {
        crate::python::offset_converter::PyOffsetConverter::from_main_engine(&self.main_engine)
    }

    /// Get the StopOrderEngine for managing stop orders.
    ///
    /// The StopOrderEngine tracks conditional orders (stop-loss, take-profit,
    /// trailing stops) and triggers real orders when conditions are met.
    ///
    /// Returns:
    ///     StopOrderEngine instance
    fn get_stop_order_engine(&self) -> crate::python::stop_order_engine::PyStopOrderEngine {
        crate::python::stop_order_engine::PyStopOrderEngine::new(
            self.main_engine.stop_order_engine().clone(),
        )
    }

    /// Get the BracketOrderEngine for managing bracket/OCO/OTO orders.
    ///
    /// The BracketOrderEngine manages groups of contingent orders:
    /// - Bracket: entry + take-profit + stop-loss
    /// - OCO: one-cancels-other order pairs
    /// - OTO: one-triggers-other order pairs
    ///
    /// Returns:
    ///     BracketOrderEngine instance
    fn get_bracket_order_engine(&self) -> crate::python::bracket_order_engine::PyBracketOrderEngine {
        crate::python::bracket_order_engine::PyBracketOrderEngine::new(
            self.main_engine.bracket_order_engine().clone(),
        )
    }

    /// Get the OrderEmulator for managing emulated order types.
    ///
    /// The OrderEmulator locally simulates advanced order types not natively
    /// supported by exchanges: trailing stops, stop-limit, iceberg, MIT, LIT,
    /// and pegged-to-best orders.
    ///
    /// Returns:
    ///     OrderEmulator instance
    fn get_order_emulator(&self) -> crate::python::order_emulator::PyOrderEmulator {
        crate::python::order_emulator::PyOrderEmulator::new(
            self.main_engine.order_emulator().clone(),
        )
    }

    /// Get the shared MessageBus wrapping the MainEngine's Rust MessageBus.
    ///
    /// This allows Python code to use the same pub/sub bus as the Rust
    /// engine, so messages published from Rust are visible to Python
    /// strategies and vice versa.
    ///
    /// Returns:
    ///     MessageBus instance backed by MainEngine's Rust MessageBus
    fn get_message_bus(&self) -> PyResult<MessageBus> {
        let rust_bus = self.main_engine.get_message_bus().clone();
        Ok(MessageBus::from_rust_message_bus(rust_bus))
    }
}
