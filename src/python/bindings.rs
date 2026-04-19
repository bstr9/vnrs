use crate::python::{OrderFactory, PyOrder, PythonEngine, Strategy};
use crate::strategy::StrategyEngine;
use crate::trader::constant::{Direction, Offset, OrderType};
use crate::trader::MainEngine;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
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
    _py: Python,
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

    // Register portfolio facade module
    crate::python::portfolio::register_portfolio_module(m)?;

    // Register message bus module
    crate::python::message_bus::register_message_bus_module(m)?;

    // Register risk manager module
    crate::python::risk_manager::register_risk_module(m)?;

    // Register sync bar generator module
    crate::python::sync_bar_bindings::register_sync_bar_module(m)?;

    // Register deprecated strategy classes (kept for backward compatibility)
    #[allow(deprecated)]
    {
        crate::python::strategy_bindings::register_strategy_module(m)?;
    }

    Ok(())
}

/// Wrapper for PythonEngine to make it compatible with PyO3
#[pyclass]
pub struct PythonEngineWrapper {
    inner: std::sync::Mutex<PythonEngine>,
    #[allow(dead_code)]
    rt: Runtime,
}

#[pymethods]
impl PythonEngineWrapper {
    #[new]
    fn new() -> PyResult<Self> {
        let rt = Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?;

        Ok(PythonEngineWrapper {
            inner: std::sync::Mutex::new(PythonEngine::new(MainEngine::new())),
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

    fn on_tick(&self, _py: Python, tick_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = tick_dict
            .get_item("symbol")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing required key: symbol"))?
            .extract()?;

        Ok(())
    }

    fn on_bar(&self, _py: Python, bar_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = bar_dict
            .get_item("symbol")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing required key: symbol"))?
            .extract()?;
        Ok(())
    }

    fn on_trade(&self, _py: Python, trade_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = trade_dict
            .get_item("symbol")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing required key: symbol"))?
            .extract()?;
        Ok(())
    }

    fn on_order(&self, _py: Python, order_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = order_dict
            .get_item("symbol")?
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("Missing required key: symbol"))?
            .extract()?;
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
}
