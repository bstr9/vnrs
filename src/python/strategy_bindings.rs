//! Python bindings for Strategy Engine
//! 
//! Allows Python strategies to interface with the Rust strategy engine

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

use crate::strategy::StrategyEngine;

/// Python strategy wrapper
///
/// **Deprecated**: Use the unified `Strategy` base class from `strategy` module instead.
/// `Strategy` supports Python subclassing directly (e.g., `class MyStrategy(Strategy): ...`)
/// and does not require manual callback registration via `set_on_*` methods.
#[deprecated(
    since = "0.5.0",
    note = "Use the unified `Strategy` base class instead. See `trade_engine.Strategy`."
)]
#[pyclass]
pub struct PyStrategy {
    #[pyo3(get, set)]
    pub strategy_name: String,
    
    #[pyo3(get, set)]
    pub vt_symbols: Vec<String>,
    
    #[pyo3(get, set)]
    pub strategy_type: String,
    
    #[pyo3(get)]
    pub inited: bool,
    
    #[pyo3(get)]
    pub trading: bool,
    
    engine: Option<Py<PyAny>>,
    
    on_init_callback: Option<Py<PyAny>>,
    on_start_callback: Option<Py<PyAny>>,
    on_stop_callback: Option<Py<PyAny>>,
    on_tick_callback: Option<Py<PyAny>>,
    on_bar_callback: Option<Py<PyAny>>,
    on_bars_callback: Option<Py<PyAny>>,
    on_order_callback: Option<Py<PyAny>>,
    on_trade_callback: Option<Py<PyAny>>,
    
    positions: HashMap<String, f64>,
    
    parameters: HashMap<String, String>,
}

#[pymethods]
impl PyStrategy {
    #[new]
    #[pyo3(signature = (strategy_name, vt_symbols, strategy_type="spot", **kwargs))]
    fn new(
        strategy_name: String,
        vt_symbols: Vec<String>,
        strategy_type: Option<&str>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> Self {
        let mut parameters = HashMap::new();
        
        if let Some(kw) = kwargs {
            for (key, value) in kw.iter() {
                if let Ok(key_str) = key.extract::<String>() {
                    if let Ok(val_str) = value.extract::<String>() {
                        parameters.insert(key_str, val_str);
                    }
                }
            }
        }

        Self {
            strategy_name,
            vt_symbols,
            strategy_type: strategy_type.unwrap_or("spot").to_string(),
            inited: false,
            trading: false,
            engine: None,
            on_init_callback: None,
            on_start_callback: None,
            on_stop_callback: None,
            on_tick_callback: None,
            on_bar_callback: None,
            on_bars_callback: None,
            on_order_callback: None,
            on_trade_callback: None,
            positions: HashMap::new(),
            parameters,
        }
    }

    fn set_on_init(&mut self, callback: Py<PyAny>) {
        self.on_init_callback = Some(callback);
    }

    fn set_on_start(&mut self, callback: Py<PyAny>) {
        self.on_start_callback = Some(callback);
    }

    fn set_on_stop(&mut self, callback: Py<PyAny>) {
        self.on_stop_callback = Some(callback);
    }

    fn set_on_tick(&mut self, callback: Py<PyAny>) {
        self.on_tick_callback = Some(callback);
    }

    fn set_on_bar(&mut self, callback: Py<PyAny>) {
        self.on_bar_callback = Some(callback);
    }

    fn set_on_bars(&mut self, callback: Py<PyAny>) {
        self.on_bars_callback = Some(callback);
    }

    fn set_on_order(&mut self, callback: Py<PyAny>) {
        self.on_order_callback = Some(callback);
    }

    fn set_on_trade(&mut self, callback: Py<PyAny>) {
        self.on_trade_callback = Some(callback);
    }

    fn get_pos(&self, vt_symbol: &str) -> f64 {
        self.positions.get(vt_symbol).copied().unwrap_or(0.0)
    }

    fn get_positions(&self) -> HashMap<String, f64> {
        self.positions.clone()
    }

    fn write_log(&self, msg: &str) {
        tracing::info!("[{}] {}", self.strategy_name, msg);
    }

    fn get_parameter(&self, key: &str) -> Option<String> {
        self.parameters.get(key).cloned()
    }

    fn set_parameter(&mut self, key: String, value: String) {
        self.parameters.insert(key, value);
    }

    /// Set the engine reference for order routing
    fn set_engine(&mut self, engine: Py<PyAny>) {
        self.engine = Some(engine);
    }

    /// Buy (long open)
    #[pyo3(signature = (vt_symbol, price, volume, _lock=false))]
    fn buy(&self, py: Python, vt_symbol: &str, price: f64, volume: f64, _lock: Option<bool>) -> Vec<String> {
        tracing::info!("[{}] BUY {} @ {} x{}", self.strategy_name, vt_symbol, price, volume);
        if let Some(ref engine) = self.engine {
            match engine.call_method1(py, "buy", (vt_symbol, price, volume)) {
                Ok(result) => match result.extract::<Vec<String>>(py) {
                    Ok(ids) => return ids,
                    Err(e) => tracing::error!("[{}] Failed to extract buy order IDs: {}", self.strategy_name, e),
                },
                Err(e) => tracing::error!("[{}] Failed to call buy on engine: {}", self.strategy_name, e),
            }
        }
        Vec::new()
    }

    /// Sell (long close)
    #[pyo3(signature = (vt_symbol, price, volume, _lock=false))]
    fn sell(&self, py: Python, vt_symbol: &str, price: f64, volume: f64, _lock: Option<bool>) -> Vec<String> {
        tracing::info!("[{}] SELL {} @ {} x{}", self.strategy_name, vt_symbol, price, volume);
        if let Some(ref engine) = self.engine {
            match engine.call_method1(py, "sell", (vt_symbol, price, volume)) {
                Ok(result) => match result.extract::<Vec<String>>(py) {
                    Ok(ids) => return ids,
                    Err(e) => tracing::error!("[{}] Failed to extract sell order IDs: {}", self.strategy_name, e),
                },
                Err(e) => tracing::error!("[{}] Failed to call sell on engine: {}", self.strategy_name, e),
            }
        }
        Vec::new()
    }

    /// Short (short open, futures only)
    #[pyo3(signature = (vt_symbol, price, volume, _lock=false))]
    fn short(&self, py: Python, vt_symbol: &str, price: f64, volume: f64, _lock: Option<bool>) -> Vec<String> {
        if self.strategy_type == "spot" {
            tracing::warn!("[{}] Short not supported for spot trading", self.strategy_name);
            return Vec::new();
        }
        tracing::info!("[{}] SHORT {} @ {} x{}", self.strategy_name, vt_symbol, price, volume);
        if let Some(ref engine) = self.engine {
            match engine.call_method1(py, "short", (vt_symbol, price, volume)) {
                Ok(result) => match result.extract::<Vec<String>>(py) {
                    Ok(ids) => return ids,
                    Err(e) => tracing::error!("[{}] Failed to extract short order IDs: {}", self.strategy_name, e),
                },
                Err(e) => tracing::error!("[{}] Failed to call short on engine: {}", self.strategy_name, e),
            }
        }
        Vec::new()
    }

    /// Cover (short close, futures only)
    #[pyo3(signature = (vt_symbol, price, volume, _lock=false))]
    fn cover(&self, py: Python, vt_symbol: &str, price: f64, volume: f64, _lock: Option<bool>) -> Vec<String> {
        if self.strategy_type == "spot" {
            tracing::warn!("[{}] Cover not supported for spot trading", self.strategy_name);
            return Vec::new();
        }
        tracing::info!("[{}] COVER {} @ {} x{}", self.strategy_name, vt_symbol, price, volume);
        if let Some(ref engine) = self.engine {
            match engine.call_method1(py, "cover", (vt_symbol, price, volume)) {
                Ok(result) => match result.extract::<Vec<String>>(py) {
                    Ok(ids) => return ids,
                    Err(e) => tracing::error!("[{}] Failed to extract cover order IDs: {}", self.strategy_name, e),
                },
                Err(e) => tracing::error!("[{}] Failed to call cover on engine: {}", self.strategy_name, e),
            }
        }
        Vec::new()
    }

    /// Cancel order
    fn cancel_order(&self, py: Python, vt_orderid: &str) {
        tracing::info!("[{}] Cancel order: {}", self.strategy_name, vt_orderid);
        if let Some(ref engine) = self.engine {
            if let Err(e) = engine.call_method1(py, "cancel_order", (vt_orderid,)) {
                tracing::error!("[{}] Failed to call cancel_order on engine: {}", self.strategy_name, e);
            }
        }
    }

    /// Cancel all orders
    fn cancel_all(&self) {
        tracing::info!("[{}] Cancel all orders", self.strategy_name);
    }
}

/// Python Strategy Engine wrapper
///
/// **Deprecated**: Use the unified `Strategy` base class and `PythonEngineWrapper` instead.
#[deprecated(
    since = "0.5.0",
    note = "Use the unified `Strategy` base class and `PythonEngineWrapper` instead."
)]
#[pyclass]
pub struct PyStrategyEngine {
    engine: Arc<StrategyEngine>,
    rt: Runtime,
    strategies: HashMap<String, Py<PyStrategy>>,
}

#[pymethods]
impl PyStrategyEngine {
    #[new]
    fn new() -> PyResult<Self> {
        let rt = Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e))
        })?;

        let main_engine = Arc::new(crate::trader::MainEngine::new());
        let event_engine = Arc::new(crate::event::EventEngine::new(1));

        Ok(Self {
            engine: Arc::new(StrategyEngine::new(main_engine, event_engine)),
            rt,
            strategies: HashMap::new(),
        })
    }

    fn add_strategy(&mut self, py_strategy: &Bound<'_, PyStrategy>, settings: &Bound<'_, PyDict>) -> PyResult<()> {
        let mut setting_map = HashMap::new();
        for (key, value) in settings.iter() {
            if let (Ok(k), Ok(v)) = (key.extract::<String>(), value.extract::<String>()) {
                setting_map.insert(k, serde_json::Value::String(v));
            }
        }

        let strategy_name = py_strategy.borrow().strategy_name.clone();
        tracing::info!("Adding Python strategy: {}", strategy_name);
        self.strategies.insert(strategy_name, py_strategy.clone().unbind());
        
        Ok(())
    }

    fn init_strategy(&self, strategy_name: &str) -> PyResult<()> {
        self.rt.block_on(async {
            self.engine.init_strategy(strategy_name).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Init failed: {}", e))
        })
    }

    fn start_strategy(&self, strategy_name: &str) -> PyResult<()> {
        self.rt.block_on(async {
            self.engine.start_strategy(strategy_name).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Start failed: {}", e))
        })
    }

    fn stop_strategy(&self, strategy_name: &str) -> PyResult<()> {
        self.rt.block_on(async {
            self.engine.stop_strategy(strategy_name).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Stop failed: {}", e))
        })
    }

    fn get_all_strategies(&self) -> PyResult<Vec<String>> {
        Ok(self.rt.block_on(async {
            self.engine.get_all_strategy_names().await
        }))
    }

    fn get_strategy_info(&self, strategy_name: &str) -> PyResult<HashMap<String, String>> {
        self.rt.block_on(async {
            self.engine.get_strategy_info(strategy_name).await
        }).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("Strategy {} not found", strategy_name))
        })
    }
}

/// Register Python module
pub fn register_strategy_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyStrategy>()?;
    m.add_class::<PyStrategyEngine>()?;
    Ok(())
}
