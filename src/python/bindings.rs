use pyo3::prelude::*;
use pyo3::types::PyDict;
use tokio::runtime::Runtime;
use crate::trader::MainEngine;
use crate::python::{PythonStrategy, PythonEngine};

/// Python module for the trading engine
#[pymodule]
fn trade_engine(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PythonStrategy>()?;
    m.add_class::<PythonEngineWrapper>()?;
    m.add_function(wrap_pyfunction!(create_main_engine, m)?)?;
    m.add_function(wrap_pyfunction!(run_event_loop, m)?)?;
    
    // Register backtesting module
    crate::python::backtesting_bindings::register_backtesting_module(m)?;
    
    Ok(())
}

/// Wrapper for PythonEngine to make it compatible with PyO3
#[pyclass]
pub struct PythonEngineWrapper {
    inner: std::sync::Mutex<PythonEngine>,
    rt: Runtime,
}

#[pymethods]
impl PythonEngineWrapper {
    #[new]
    fn new() -> PyResult<Self> {
        let rt = Runtime::new().map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string())
        })?;
        
        Ok(PythonEngineWrapper {
            inner: std::sync::Mutex::new(PythonEngine::new(MainEngine::new())),
            rt,
        })
    }
    
    fn add_strategy(&self, py: Python, strategy: Bound<'_, PythonStrategy>) -> PyResult<()> {
        self.inner.lock().unwrap().add_strategy_py(py, strategy)
    }
    
    fn init_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner.lock().unwrap().init_strategy_py(py, &strategy_name)
    }
    
    fn start_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner.lock().unwrap().start_strategy_py(py, &strategy_name)
    }
    
    fn stop_strategy(&self, py: Python, strategy_name: String) -> PyResult<()> {
        self.inner.lock().unwrap().stop_strategy_py(py, &strategy_name)
    }
    
    fn on_tick(&self, _py: Python, tick_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        // Convert the Python dict to a Rust TickData
        // This would involve more complex conversion in a real implementation
        let _symbol: String = tick_dict.get_item("symbol")?.unwrap().extract()?;
        
        // For now, just call the internal method
        // In a real implementation, we'd convert the dict to Rust struct first
        Ok(())
    }
    
    fn on_bar(&self, _py: Python, bar_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = bar_dict.get_item("symbol")?.unwrap().extract()?;
        Ok(())
    }
    
    fn on_trade(&self, _py: Python, trade_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = trade_dict.get_item("symbol")?.unwrap().extract()?;
        Ok(())
    }
    
    fn on_order(&self, _py: Python, order_dict: &Bound<'_, PyDict>) -> PyResult<()> {
        let _symbol: String = order_dict.get_item("symbol")?.unwrap().extract()?;
        Ok(())
    }
    
    // Order management methods
    fn buy(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self.inner.lock().unwrap().buy(&vt_symbol, price, volume);
        Ok(result)
    }
    
    fn sell(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self.inner.lock().unwrap().sell(&vt_symbol, price, volume);
        Ok(result)
    }
    
    fn short(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self.inner.lock().unwrap().short(&vt_symbol, price, volume);
        Ok(result)
    }
    
    fn cover(&self, vt_symbol: String, price: f64, volume: f64) -> PyResult<Vec<String>> {
        let result = self.inner.lock().unwrap().cover(&vt_symbol, price, volume);
        Ok(result)
    }
    
    fn cancel_order(&self, vt_orderid: String) {
        self.inner.lock().unwrap().cancel_order(&vt_orderid);
    }
    
    fn get_pos(&self, vt_symbol: String) -> PyResult<f64> {
        Ok(self.inner.lock().unwrap().get_pos(&vt_symbol))
    }
    
    fn write_log(&self, msg: String) {
        self.inner.lock().unwrap().write_log(&msg);
    }
    
    fn send_email(&self, msg: String) {
        self.inner.lock().unwrap().send_email(&msg);
    }
}

/// Create main engine from Python
#[pyfunction]
fn create_main_engine(py: Python) -> PyResult<Py<PyAny>> {
    // In a real implementation, we would create and return a Python-wrapped Rust MainEngine
    // For now, we'll just return None
    Ok(py.None())
}

/// Run the event loop
#[pyfunction]
fn run_event_loop() -> PyResult<()> {
    // In a real implementation, we would run the main trading event loop
    println!("Event loop running...");
    Ok(())
}