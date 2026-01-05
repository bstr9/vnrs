//! Python bindings for Strategy Engine
//! 
//! Allows Python strategies to interface with the Rust strategy engine

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Runtime;

use crate::trader::{TickData, BarData, OrderData, TradeData, Direction, Offset, OrderType, Interval};
use crate::strategy::{StrategyEngine, StrategyTemplate, StrategyContext, StrategyType, StrategyState};
use super::data_converter;

/// Python strategy wrapper
#[pyclass]
pub struct PyStrategy {
    #[pyo3(get, set)]
    pub strategy_name: String,
    
    #[pyo3(get, set)]
    pub vt_symbols: Vec<String>,
    
    #[pyo3(get, set)]
    pub strategy_type: String, // "spot" or "futures"
    
    #[pyo3(get)]
    pub inited: bool,
    
    #[pyo3(get)]
    pub trading: bool,
    
    // Python callbacks (stored as Python objects)
    on_init_callback: Option<PyObject>,
    on_start_callback: Option<PyObject>,
    on_stop_callback: Option<PyObject>,
    on_tick_callback: Option<PyObject>,
    on_bar_callback: Option<PyObject>,
    on_bars_callback: Option<PyObject>,
    on_order_callback: Option<PyObject>,
    on_trade_callback: Option<PyObject>,
    
    // Position tracking
    positions: HashMap<String, f64>,
    
    // Parameters
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
        
        // Extract parameters from kwargs
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

    /// Set initialization callback
    fn set_on_init(&mut self, callback: PyObject) {
        self.on_init_callback = Some(callback);
    }

    /// Set start callback
    fn set_on_start(&mut self, callback: PyObject) {
        self.on_start_callback = Some(callback);
    }

    /// Set stop callback
    fn set_on_stop(&mut self, callback: PyObject) {
        self.on_stop_callback = Some(callback);
    }

    /// Set tick callback
    fn set_on_tick(&mut self, callback: PyObject) {
        self.on_tick_callback = Some(callback);
    }

    /// Set bar callback
    fn set_on_bar(&mut self, callback: PyObject) {
        self.on_bar_callback = Some(callback);
    }

    /// Set bars callback (multi-symbol)
    fn set_on_bars(&mut self, callback: PyObject) {
        self.on_bars_callback = Some(callback);
    }

    /// Set order callback
    fn set_on_order(&mut self, callback: PyObject) {
        self.on_order_callback = Some(callback);
    }

    /// Set trade callback
    fn set_on_trade(&mut self, callback: PyObject) {
        self.on_trade_callback = Some(callback);
    }

    /// Get current position
    fn get_pos(&self, vt_symbol: &str) -> f64 {
        self.positions.get(vt_symbol).copied().unwrap_or(0.0)
    }

    /// Get all positions
    fn get_positions(&self) -> HashMap<String, f64> {
        self.positions.clone()
    }

    /// Write log message
    fn write_log(&self, msg: &str) {
        tracing::info!("[{}] {}", self.strategy_name, msg);
    }

    /// Get parameter value
    fn get_parameter(&self, key: &str) -> Option<String> {
        self.parameters.get(key).cloned()
    }

    /// Set parameter value
    fn set_parameter(&mut self, key: String, value: String) {
        self.parameters.insert(key, value);
    }

    /// Buy order (spot or futures long)
    #[pyo3(signature = (vt_symbol, price, volume, lock=false))]
    fn buy(&self, vt_symbol: &str, price: f64, volume: f64, lock: Option<bool>) -> String {
        tracing::info!(
            "[{}] BUY {} @ {} x{}",
            self.strategy_name, vt_symbol, price, volume
        );
        format!("BUY_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis())
    }

    /// Sell order (spot or futures close long)
    #[pyo3(signature = (vt_symbol, price, volume, lock=false))]
    fn sell(&self, vt_symbol: &str, price: f64, volume: f64, lock: Option<bool>) -> String {
        tracing::info!(
            "[{}] SELL {} @ {} x{}",
            self.strategy_name, vt_symbol, price, volume
        );
        format!("SELL_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis())
    }

    /// Short order (futures only)
    #[pyo3(signature = (vt_symbol, price, volume, lock=false))]
    fn short(&self, vt_symbol: &str, price: f64, volume: f64, lock: Option<bool>) -> String {
        if self.strategy_type == "spot" {
            tracing::warn!("[{}] Short not supported for spot trading", self.strategy_name);
            return String::new();
        }
        
        tracing::info!(
            "[{}] SHORT {} @ {} x{}",
            self.strategy_name, vt_symbol, price, volume
        );
        format!("SHORT_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis())
    }

    /// Cover order (futures only)
    #[pyo3(signature = (vt_symbol, price, volume, lock=false))]
    fn cover(&self, vt_symbol: &str, price: f64, volume: f64, lock: Option<bool>) -> String {
        if self.strategy_type == "spot" {
            tracing::warn!("[{}] Cover not supported for spot trading", self.strategy_name);
            return String::new();
        }
        
        tracing::info!(
            "[{}] COVER {} @ {} x{}",
            self.strategy_name, vt_symbol, price, volume
        );
        format!("COVER_{}_{}", vt_symbol, chrono::Utc::now().timestamp_millis())
    }

    /// Cancel order
    fn cancel_order(&self, vt_orderid: &str) {
        tracing::info!("[{}] Cancel order: {}", self.strategy_name, vt_orderid);
    }

    /// Cancel all orders
    fn cancel_all(&self) {
        tracing::info!("[{}] Cancel all orders", self.strategy_name);
    }
}

/// Python Strategy Engine wrapper
#[pyclass]
pub struct PyStrategyEngine {
    engine: Arc<StrategyEngine>,
    rt: Runtime,
}

#[pymethods]
impl PyStrategyEngine {
    #[new]
    fn new() -> PyResult<Self> {
        // Create async runtime
        let rt = Runtime::new().map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to create runtime: {}", e))
        })?;

        // Create placeholder main and event engines
        // In production, these would be passed from Python or shared
        let main_engine = Arc::new(crate::trader::MainEngine::new());
        let event_engine = Arc::new(crate::event::EventEngine::new(1)); // 1ms timer interval

        Ok(Self {
            engine: Arc::new(StrategyEngine::new(main_engine, event_engine)),
            rt,
        })
    }

    /// Add a Python strategy
    fn add_strategy(&mut self, py_strategy: &Bound<'_, PyStrategy>, settings: &Bound<'_, PyDict>) -> PyResult<()> {
        // Convert PyDict to HashMap
        let mut setting_map = HashMap::new();
        for (key, value) in settings.iter() {
            if let (Ok(k), Ok(v)) = (key.extract::<String>(), value.extract::<String>()) {
                setting_map.insert(k, serde_json::Value::String(v));
            }
        }

        tracing::info!("Adding Python strategy: {}", py_strategy.borrow().strategy_name);
        
        Ok(())
    }

    /// Initialize a strategy
    fn init_strategy(&self, strategy_name: &str) -> PyResult<()> {
        self.rt.block_on(async {
            self.engine.init_strategy(strategy_name).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Init failed: {}", e))
        })
    }

    /// Start a strategy
    fn start_strategy(&self, strategy_name: &str) -> PyResult<()> {
        self.rt.block_on(async {
            self.engine.start_strategy(strategy_name).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Start failed: {}", e))
        })
    }

    /// Stop a strategy
    fn stop_strategy(&self, strategy_name: &str) -> PyResult<()> {
        self.rt.block_on(async {
            self.engine.stop_strategy(strategy_name).await
        }).map_err(|e| {
            pyo3::exceptions::PyRuntimeError::new_err(format!("Stop failed: {}", e))
        })
    }

    /// Get all strategy names
    fn get_all_strategies(&self) -> PyResult<Vec<String>> {
        Ok(self.rt.block_on(async {
            self.engine.get_all_strategy_names().await
        }))
    }

    /// Get strategy info
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
