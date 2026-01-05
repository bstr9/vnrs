//! Python strategy interface
//! Defines how Python strategies interact with the Rust trading engine

use pyo3::prelude::*;
use std::collections::HashMap;

#[pyclass]
pub struct PythonStrategy {
    #[pyo3(get, set)]
    pub strategy_name: String,
    
    #[pyo3(get, set)]
    pub vt_symbols: Vec<String>,
    
    // Internal state
    #[pyo3(get)]
    pub pos_data: HashMap<String, f64>,
    
    #[pyo3(get)]
    pub target_data: HashMap<String, f64>,
    
    #[pyo3(get)]
    pub active_orderids: Vec<String>,
    
    // Python callable objects for strategy methods
    #[pyo3(get, set)]
    pub on_init_method: Option<Py<PyAny>>,
    
    #[pyo3(get, set)]
    pub on_bars_method: Option<Py<PyAny>>,
    
    #[pyo3(get, set)]
    pub on_tick_method: Option<Py<PyAny>>,
    
    #[pyo3(get, set)]
    pub on_trade_method: Option<Py<PyAny>>,
    
    #[pyo3(get, set)]
    pub on_order_method: Option<Py<PyAny>>,
    
    #[pyo3(get, set)]
    pub on_stop_method: Option<Py<PyAny>>,
    
    // Engine reference for callbacks
    #[pyo3(get, set)]
    pub engine: Option<Py<PyAny>>,
}

#[pymethods]
impl PythonStrategy {
    #[new]
    fn new(strategy_name: String, vt_symbols: Vec<String>) -> Self {
        PythonStrategy {
            strategy_name,
            vt_symbols,
            pos_data: HashMap::new(),
            target_data: HashMap::new(),
            active_orderids: Vec::new(),
            on_init_method: None,
            on_bars_method: None,
            on_tick_method: None,
            on_trade_method: None,
            on_order_method: None,
            on_stop_method: None,
            engine: None,
        }
    }
    
    /// Initialize the strategy
    pub fn on_init(&self, py: Python) -> PyResult<()> {
        if let Some(ref callback) = self.on_init_method {
            callback.call0(py)?;
        }
        Ok(())
    }

    /// Handle bar data update
    pub fn on_bars(&self, py: Python, bars: Py<PyAny>) -> PyResult<()> {
        if let Some(ref callback) = self.on_bars_method {
            callback.call1(py, (bars,))?;
        }
        Ok(())
    }

    /// Handle tick data update
    pub fn on_tick(&self, py: Python, tick: Py<PyAny>) -> PyResult<()> {
        if let Some(ref callback) = self.on_tick_method {
            callback.call1(py, (tick,))?;
        }
        Ok(())
    }

    /// Handle trade update
    pub fn on_trade(&self, py: Python, trade: Py<PyAny>) -> PyResult<()> {
        if let Some(ref callback) = self.on_trade_method {
            callback.call1(py, (trade,))?;
        }
        Ok(())
    }

    /// Handle order update
    pub fn on_order(&self, py: Python, order: Py<PyAny>) -> PyResult<()> {
        if let Some(ref callback) = self.on_order_method {
            callback.call1(py, (order,))?;
        }
        Ok(())
    }

    /// Handle stop event
    pub fn on_stop(&self, py: Python) -> PyResult<()> {
        if let Some(ref callback) = self.on_stop_method {
            callback.call0(py)?;
        }
        Ok(())
    }
    
    /// Buy order
    fn buy(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "buy", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }
    
    /// Sell order
    fn sell(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "sell", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }
    
    /// Short order
    fn short(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "short", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }
    
    /// Cover order
    fn cover(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "cover", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }
    
    /// Cancel order
    fn cancel_order(&self, py: Python, vt_orderid: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            engine.call_method1(py, "cancel_order", (vt_orderid,))?;
        }
        Ok(())
    }
    
    /// Get position
    fn get_pos(&self, py: Python, vt_symbol: &str) -> PyResult<f64> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "get_pos", (vt_symbol,))?;
            Ok(result.extract(py)?)
        } else {
            Ok(0.0)
        }
    }
    
    /// Send email
    fn send_email(&self, py: Python, msg: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            engine.call_method1(py, "send_email", (msg,))?;
        }
        Ok(())
    }
    
    /// Write log
    fn write_log(&self, py: Python, msg: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            engine.call_method1(py, "write_log", (msg,))?;
        }
        Ok(())
    }
}