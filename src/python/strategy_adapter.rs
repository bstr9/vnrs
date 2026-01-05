//! Python Strategy Adapter
//!
//! Adapts Python strategies to work with Rust StrategyTemplate trait

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule, PyTuple};
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::python::backtesting_bindings::PyBarData;
use crate::strategy::{StrategyContext, StrategyState, StrategyTemplate, StrategyType};
use crate::trader::{BarData, OrderData, TickData, TradeData};

/// Python strategy adapter that implements StrategyTemplate
pub struct PythonStrategyAdapter {
    /// Python strategy instance
    py_strategy: Arc<Mutex<PyObject>>,

    /// Strategy name
    strategy_name: String,

    /// Trading symbols
    vt_symbols: Vec<String>,

    /// Strategy type
    strategy_type: StrategyType,

    /// Current state
    state: StrategyState,

    /// Position tracking
    positions: Arc<Mutex<HashMap<String, f64>>>,

    /// Parameters and variables
    parameters: Arc<Mutex<HashMap<String, String>>>,
    variables: Arc<Mutex<HashMap<String, String>>>,
}

impl PythonStrategyAdapter {
    /// Load strategy from Python file
    pub fn load_from_file(
        file_path: &str,
        class_name: &str,
        strategy_name: String,
        vt_symbols: Vec<String>,
        parameters: Option<pyo3::Py<PyDict>>,
    ) -> Result<Self, String> {
        Python::with_gil(|py| {
            // Read Python file
            let code = std::fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read strategy file: {}", e))?;

            // Compile and execute module
            use std::ffi::CString;
            let code_c = CString::new(code.as_str()).unwrap();
            let file_c = CString::new(file_path).unwrap();
            let name_c = CString::new("strategy_module").unwrap();
            let module =
                PyModule::from_code(py, code_c.as_c_str(), file_c.as_c_str(), name_c.as_c_str())
                    .map_err(|e| format!("Failed to load Python module: {}", e))?;

            // Get strategy class
            let strategy_class = module
                .getattr(class_name)
                .map_err(|e| format!("Strategy class '{}' not found: {}", class_name, e))?;

            // Create strategy instance
            let py_instance = if let Some(params) = parameters {
                let args_tuple = (strategy_name.clone(), vt_symbols.clone());
                let inst = strategy_class
                    .call1(args_tuple)
                    .map_err(|e| format!("Failed to create strategy instance: {}", e))?;

                // Set custom parameters if provided
                let params_dict = params.bind(py);
                for (key, value) in params_dict.iter() {
                    let key_str = key
                        .extract::<String>()
                        .map_err(|e| format!("Failed to extract parameter key: {}", e))?;
                    inst.setattr(key_str.as_str(), value)
                        .map_err(|e| format!("Failed to set parameter: {}", e))?;
                }
                inst
            } else {
                strategy_class
                    .call1((strategy_name.clone(), vt_symbols.clone()))
                    .map_err(|e| format!("Failed to create strategy instance: {}", e))?
            };

            Ok(Self {
                py_strategy: Arc::new(Mutex::new(py_instance.into())),
                strategy_name,
                vt_symbols,
                strategy_type: StrategyType::Spot,
                state: StrategyState::NotInited,
                positions: Arc::new(Mutex::new(HashMap::new())),
                parameters: Arc::new(Mutex::new(HashMap::new())),
                variables: Arc::new(Mutex::new(HashMap::new())),
            })
        })
    }

    /// Create from existing Python object
    pub fn from_py_object(
        py_strategy: PyObject,
        strategy_name: String,
        vt_symbols: Vec<String>,
    ) -> Self {
        Self {
            py_strategy: Arc::new(Mutex::new(py_strategy)),
            strategy_name,
            vt_symbols,
            strategy_type: StrategyType::Spot,
            state: StrategyState::NotInited,
            positions: Arc::new(Mutex::new(HashMap::new())),
            parameters: Arc::new(Mutex::new(HashMap::new())),
            variables: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Call Python method with error handling
    fn call_py_method_with_dict(
        &self,
        method_name: &str,
        py: Python,
        arg_dict: &Bound<PyDict>,
    ) -> Result<Py<PyAny>, String> {
        let strategy = self.py_strategy.lock().unwrap();
        strategy
            .call_method1(py, method_name, (arg_dict,))
            .map_err(|e| format!("Failed to call '{}': {}", method_name, e))
    }

    /// Call Python method with string argument
    fn call_py_method_with_str(
        &self,
        method_name: &str,
        py: Python,
        arg: &str,
    ) -> Result<Py<PyAny>, String> {
        let strategy = self.py_strategy.lock().unwrap();
        strategy
            .call_method1(py, method_name, (arg,))
            .map_err(|e| format!("Failed to call '{}': {}", method_name, e))
    }

    /// Call Python method without arguments
    fn call_py_method_no_args(&self, method_name: &str) -> Result<(), String> {
        Python::with_gil(|py| {
            let strategy = self.py_strategy.lock().unwrap();
            strategy
                .call_method0(py, method_name)
                .map_err(|e| format!("Failed to call '{}': {}", method_name, e))?;
            Ok(())
        })
    }

    /// Call Python method with generic object argument
    fn call_py_method1(
        &self,
        method_name: &str,
        py: Python,
        arg: Py<PyAny>,
    ) -> Result<Py<PyAny>, String> {
        let strategy = self.py_strategy.lock().unwrap();
        strategy
            .call_method1(py, method_name, (arg,))
            .map_err(|e| format!("Failed to call '{}': {}", method_name, e))
    }
}

impl StrategyTemplate for PythonStrategyAdapter {
    fn strategy_name(&self) -> &str {
        &self.strategy_name
    }

    fn vt_symbols(&self) -> &[String] {
        &self.vt_symbols
    }

    fn strategy_type(&self) -> StrategyType {
        self.strategy_type
    }

    fn state(&self) -> StrategyState {
        self.state
    }

    fn parameters(&self) -> HashMap<String, String> {
        self.parameters.lock().unwrap().clone()
    }

    fn variables(&self) -> HashMap<String, String> {
        self.variables.lock().unwrap().clone()
    }

    fn on_init(&mut self, _context: &StrategyContext) {
        if let Err(e) = self.call_py_method_no_args("on_init") {
            eprintln!("on_init error: {}", e);
        }
        self.state = StrategyState::Inited;
    }

    fn on_start(&mut self) {
        if let Err(e) = self.call_py_method_no_args("on_start") {
            eprintln!("on_start error: {}", e);
        }
        self.state = StrategyState::Trading;
    }

    fn on_stop(&mut self) {
        if let Err(e) = self.call_py_method_no_args("on_stop") {
            eprintln!("on_stop error: {}", e);
        }
        self.state = StrategyState::Stopped;
    }

    fn on_tick(&mut self, tick: &TickData, _context: &StrategyContext) {
        Python::with_gil(|py| {
            let tick_dict = PyDict::new(py);
            let _ = tick_dict.set_item("symbol", &tick.symbol);
            let _ = tick_dict.set_item("exchange", format!("{:?}", tick.exchange));
            let _ = tick_dict.set_item("datetime", tick.datetime.to_string());
            let _ = tick_dict.set_item("last_price", tick.last_price);
            let _ = tick_dict.set_item("volume", tick.volume);
            let _ = tick_dict.set_item("bid_price_1", tick.bid_price_1);
            let _ = tick_dict.set_item("ask_price_1", tick.ask_price_1);
            let _ = tick_dict.set_item("bid_volume_1", tick.bid_volume_1);
            let _ = tick_dict.set_item("ask_volume_1", tick.ask_volume_1);

            if let Err(e) = self.call_py_method_with_dict("on_tick", py, &tick_dict) {
                eprintln!("on_tick error: {}", e);
            }
        });
    }

    fn on_bar(&mut self, bar: &BarData, _context: &StrategyContext) {
        Python::with_gil(|py| {
            // Import required modules
            let object_module = match PyModule::import(py, "vnpy.trader.object") {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to import vnpy.trader.object: {}", e);
                    return;
                }
            };
            let constant_module = match PyModule::import(py, "vnpy.trader.constant") {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to import vnpy.trader.constant: {}", e);
                    return;
                }
            };
            let datetime_module = match PyModule::import(py, "datetime") {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Failed to import datetime: {}", e);
                    return;
                }
            };

            let bar_data_class = match object_module.getattr("BarData") {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("BarData class not found: {}", e);
                    return;
                }
            };
            let exchange_enum = constant_module.getattr("Exchange").unwrap();
            let interval_enum = constant_module.getattr("Interval").unwrap();

            // Convert Exchange
            // Assuming exchange.value() (e.g. "BINANCE") is valid for Exchange constuctor or lookup
            let exchange_val = bar.exchange.value();
            // Try Exchange(value)
            let py_exchange = match exchange_enum.call1((exchange_val,)) {
                Ok(e) => e,
                Err(_) => {
                    // Fallback to Exchange.LOCAL or similar if not found, or keeping string might fail if typed?
                    // Let's try to find a generic or use the first enum?
                    // Better to just print error?
                    // For backtesting, we mapped OKX to Global. Global value is "GLOBAL".
                    // "GLOBAL" should be in Exchange if it's defined in constant.rs
                    // But python vnpy might NOT have "GLOBAL".
                    // If python vnpy Exchange doesn't have it, we fallback to Local.
                    exchange_enum
                        .getattr("LOCAL")
                        .unwrap_or_else(|_| py.None().bind(py).clone())
                }
            };

            // Convert Interval
            let interval_val = bar
                .interval
                .unwrap_or(crate::trader::constant::Interval::Minute)
                .value();
            let py_interval = match interval_enum.call1((interval_val,)) {
                Ok(i) => i,
                Err(_) => interval_enum.getattr("MINUTE").unwrap(),
            };

            // Convert DateTime
            // bar.datetime is chrono::DateTime. format to RFC3339 string then parse?
            // Or simpler: datetime.datetime.fromisoformat(str)
            // But fromisoformat might not handle "Z" until recent python?
            // Rust to_rfc3339 uses +00:00 usually.
            let dt_str = bar.datetime.to_rfc3339();
            // datetime.datetime.fromisoformat(dt_str)
            let py_datetime = datetime_module
                .getattr("datetime")
                .unwrap()
                .call_method1("fromisoformat", (dt_str,))
                .unwrap();

            // Create kwargs
            let kwargs = PyDict::new(py);
            let _ = kwargs.set_item("gateway_name", &bar.gateway_name);
            let _ = kwargs.set_item("symbol", &bar.symbol);
            let _ = kwargs.set_item("exchange", py_exchange);
            let _ = kwargs.set_item("datetime", py_datetime);
            let _ = kwargs.set_item("interval", py_interval);
            let _ = kwargs.set_item("volume", bar.volume);
            let _ = kwargs.set_item("turnover", bar.turnover);
            let _ = kwargs.set_item("open_interest", bar.open_interest);
            let _ = kwargs.set_item("open_price", bar.open_price);
            let _ = kwargs.set_item("high_price", bar.high_price);
            let _ = kwargs.set_item("low_price", bar.low_price);
            let _ = kwargs.set_item("close_price", bar.close_price);

            // Instantiate BarData
            // BarData(**kwargs)
            match bar_data_class.call((), Some(&kwargs)) {
                Ok(py_bar) => {
                    if let Err(e) = self.call_py_method1("on_bar", py, py_bar.into()) {
                        eprintln!("NEW CODE on_bar error: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to instantiate BarData: {}", e);
                }
            }
        });
    }

    fn on_order(&mut self, order: &OrderData) {
        Python::with_gil(|py| {
            let order_dict = PyDict::new(py);
            let _ = order_dict.set_item("orderid", &order.orderid);
            let _ = order_dict.set_item("symbol", &order.symbol);
            let _ = order_dict.set_item("exchange", format!("{:?}", order.exchange));
            let _ = order_dict.set_item("direction", format!("{:?}", order.direction));
            let _ = order_dict.set_item("offset", format!("{:?}", order.offset));
            let _ = order_dict.set_item("price", order.price);
            let _ = order_dict.set_item("volume", order.volume);
            let _ = order_dict.set_item("traded", order.traded);
            let _ = order_dict.set_item("status", format!("{:?}", order.status));
            let _ = order_dict.set_item(
                "datetime",
                order.datetime.map(|dt| dt.to_string()).unwrap_or_default(),
            );

            if let Err(e) = self.call_py_method_with_dict("on_order", py, &order_dict) {
                eprintln!("on_order error: {}", e);
            }
        });
    }

    fn on_trade(&mut self, trade: &TradeData) {
        Python::with_gil(|py| {
            let trade_dict = PyDict::new(py);
            let _ = trade_dict.set_item("tradeid", &trade.tradeid);
            let _ = trade_dict.set_item("orderid", &trade.orderid);
            let _ = trade_dict.set_item("symbol", &trade.symbol);
            let _ = trade_dict.set_item("exchange", format!("{:?}", trade.exchange));
            let _ = trade_dict.set_item("direction", format!("{:?}", trade.direction));
            let _ = trade_dict.set_item("offset", format!("{:?}", trade.offset));
            let _ = trade_dict.set_item("price", trade.price);
            let _ = trade_dict.set_item("volume", trade.volume);
            let _ = trade_dict.set_item(
                "datetime",
                trade.datetime.map(|dt| dt.to_string()).unwrap_or_default(),
            );

            if let Err(e) = self.call_py_method_with_dict("on_trade", py, &trade_dict) {
                eprintln!("on_trade error: {}", e);
            }
        });
    }

    fn on_stop_order(&mut self, stop_orderid: &str) {
        Python::with_gil(|py| {
            if let Err(e) = self.call_py_method_with_str("on_stop_order", py, stop_orderid) {
                eprintln!("on_stop_order error: {}", e);
            }
        });
    }

    fn update_position(&mut self, vt_symbol: &str, position: f64) {
        self.positions
            .lock()
            .unwrap()
            .insert(vt_symbol.to_string(), position);
    }

    fn get_position(&self, vt_symbol: &str) -> f64 {
        self.positions
            .lock()
            .unwrap()
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }
}

/// Load multiple strategies from a directory and extract class names
pub fn load_strategies_from_directory(
    directory: &str,
) -> Result<Vec<(String, String, String)>, String> {
    use std::fs;
    use std::io::Read;

    let path = PathBuf::from(directory);
    if !path.exists() || !path.is_dir() {
        // Return empty if directory not found, simpler for UI
        return Ok(Vec::new());
    }

    let mut strategies = Vec::new();

    // Read all .py files
    match fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("py") {
                        let file_name = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();

                        let file_path = path.to_str().unwrap_or("").to_string();

                        // Read file to find class name
                        let mut class_name = "UnknownStrategy".to_string();
                        if let Ok(mut file) = fs::File::open(&path) {
                            let mut content = String::new();
                            if file.read_to_string(&mut content).is_ok() {
                                // Simple search for "class X(CtaTemplate):"
                                // We don't have regex crate in dependencies usually?
                                // Let's check imports. No regex.
                                // use string manipulation.
                                for line in content.lines() {
                                    let line = line.trim();
                                    if line.starts_with("class ") && line.contains("(CtaTemplate)")
                                    {
                                        // Parse class name
                                        if let Some(start) = line.find("class ") {
                                            if let Some(end) = line.find('(') {
                                                if end > start + 6 {
                                                    class_name =
                                                        line[start + 6..end].trim().to_string();
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        strategies.push((file_name, file_path, class_name));
                    }
                }
            }
        }
        Err(e) => return Err(e.to_string()),
    }

    Ok(strategies)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_strategy_file() {
        // This test requires a valid Python strategy file
        // Skip in automated tests
    }
}
