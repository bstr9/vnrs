//! Python Strategy Adapter
//!
//! Adapts Python strategies to work with Rust StrategyTemplate trait

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::python::strategy::PendingOrder;
use crate::strategy::{StrategyContext, StrategyState, StrategyTemplate, StrategyType};
use crate::trader::{
    BarData, Direction, Exchange, Offset, OrderData, OrderRequest, OrderType, TickData, TradeData,
};

/// Python strategy adapter that implements StrategyTemplate
pub struct PythonStrategyAdapter {
    /// Python strategy instance
    py_strategy: Arc<Mutex<Py<PyAny>>>,

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

    /// Pending orders from Python strategy (shared with Strategy.pending_orders)
    pending_orders: Option<Arc<Mutex<Vec<PendingOrder>>>>,
}

impl PythonStrategyAdapter {
    /// Load strategy from Python file
    pub fn load_from_file(
        file_path: &str,
        class_name: &str,
        strategy_name: String,
        vt_symbols: Vec<String>,
        setting: Option<pyo3::Py<PyDict>>,
    ) -> Result<Self, String> {
        Python::attach(|py| {
            // Read Python file
            let code = std::fs::read_to_string(file_path)
                .map_err(|e| format!("Failed to read strategy file: {}", e))?;

            // Compile and execute module
            use std::ffi::CString;
            let code_c = CString::new(code.as_str())
                .map_err(|e| format!("Invalid code string (contains NUL): {}", e))?;
            let file_c = CString::new(file_path)
                .map_err(|e| format!("Invalid file path (contains NUL): {}", e))?;
            let name_c = CString::new("strategy_module")
                .map_err(|e| format!("Invalid module name (contains NUL): {}", e))?;
            let module =
                PyModule::from_code(py, code_c.as_c_str(), file_c.as_c_str(), name_c.as_c_str())
                    .map_err(|e| format!("Failed to load Python module: {}", e))?;

            // Get strategy class
            let strategy_class = module
                .getattr(class_name)
                .map_err(|e| format!("Strategy class '{}' not found: {}", class_name, e))?;

            // Try to instantiate the strategy.
            // Support two constructor signatures:
            //   1. vnpy CtaTemplate: __init__(self, engine, strategy_name, vt_symbol, setting)
            //   2. Local CtaTemplate: __init__(self, strategy_name, vt_symbols, strategy_type="spot")
            //
            // We try vnpy-style first (4 args), then fall back to local-style (2 args).

            let vt_symbol = vt_symbols.first().cloned().unwrap_or_default();
            let py_instance = if let Some(ref params) = setting {
                // vnpy-style: (engine, strategy_name, vt_symbol, setting)
                match strategy_class.call1((
                    py.None(),
                    strategy_name.clone(),
                    vt_symbol.clone(),
                    params.bind(py),
                )) {
                    Ok(inst) => inst,
                    Err(_) => {
                        // Fallback: local CtaTemplate (strategy_name, vt_symbols)
                        // Then set parameters as attributes
                        let inst = strategy_class
                            .call1((strategy_name.clone(), vt_symbols.clone()))
                            .map_err(|e| {
                                format!(
                                    "Failed to create strategy instance with both vnpy and local signatures: {}",
                                    e
                                )
                            })?;
                        let params_dict = params.bind(py);
                        for (key, value) in params_dict.iter() {
                            let key_str = key
                                .extract::<String>()
                                .map_err(|e| format!("Failed to extract parameter key: {}", e))?;
                            inst.setattr(key_str.as_str(), value).map_err(|e| {
                                format!("Failed to set parameter '{}': {}", key_str, e)
                            })?;
                        }
                        inst
                    }
                }
            } else {
                // No setting dict provided - try vnpy-style with empty dict first
                let empty_setting = PyDict::new(py);
                match strategy_class.call1((
                    py.None(),
                    strategy_name.clone(),
                    vt_symbol.clone(),
                    empty_setting,
                )) {
                    Ok(inst) => inst,
                    Err(_) => {
                        // Fallback: local CtaTemplate (strategy_name, vt_symbols)
                        strategy_class
                            .call1((strategy_name.clone(), vt_symbols.clone()))
                            .map_err(|e| format!("Failed to create strategy instance: {}", e))?
                    }
                }
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
                pending_orders: None, // Will be set if the instance is a Strategy
            })
        })
    }

    /// Create from existing Python object
    pub fn from_py_object(
        py_strategy: Py<PyAny>,
        strategy_name: String,
        vt_symbols: Vec<String>,
    ) -> Self {
        // Try to get the pending_orders Arc from the Strategy instance
        // by downcasting the PyAny to the Rust Strategy type
        let pending_orders = Python::attach(|py| {
            use crate::python::Strategy;
            py_strategy
                .cast_bound::<Strategy>(py)
                .ok()
                .map(|bound| bound.borrow().pending_orders_arc())
        });

        Self {
            py_strategy: Arc::new(Mutex::new(py_strategy)),
            strategy_name,
            vt_symbols,
            strategy_type: StrategyType::Spot,
            state: StrategyState::NotInited,
            positions: Arc::new(Mutex::new(HashMap::new())),
            parameters: Arc::new(Mutex::new(HashMap::new())),
            variables: Arc::new(Mutex::new(HashMap::new())),
            pending_orders,
        }
    }

    /// Call Python method with error handling
    fn call_py_method_with_dict(
        &self,
        method_name: &str,
        py: Python,
        arg_dict: &Bound<PyDict>,
    ) -> Result<Py<PyAny>, String> {
        let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
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
        let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
        strategy
            .call_method1(py, method_name, (arg,))
            .map_err(|e| format!("Failed to call '{}': {}", method_name, e))
    }

    /// Call Python method without arguments
    fn call_py_method_no_args(&self, method_name: &str) -> Result<(), String> {
        Python::attach(|py| {
            let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
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
        let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
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
        self.parameters
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn variables(&self) -> HashMap<String, String> {
        self.variables
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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
        Python::attach(|py| {
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
        Python::attach(|py| {
            // Build a simple dict representation of the bar first (works without vnpy)
            let bar_dict = PyDict::new(py);
            let _ = bar_dict.set_item("gateway_name", &bar.gateway_name);
            let _ = bar_dict.set_item("symbol", &bar.symbol);
            let _ = bar_dict.set_item("exchange", bar.exchange.value());
            let _ = bar_dict.set_item("datetime", bar.datetime.to_rfc3339());
            let _ = bar_dict.set_item(
                "interval",
                bar.interval
                    .unwrap_or(crate::trader::constant::Interval::Minute)
                    .value(),
            );
            let _ = bar_dict.set_item("open_price", bar.open_price);
            let _ = bar_dict.set_item("high_price", bar.high_price);
            let _ = bar_dict.set_item("low_price", bar.low_price);
            let _ = bar_dict.set_item("close_price", bar.close_price);
            let _ = bar_dict.set_item("volume", bar.volume);
            let _ = bar_dict.set_item("turnover", bar.turnover);
            let _ = bar_dict.set_item("open_interest", bar.open_interest);

            // Try vnpy BarData first (for vnpy-compatible strategies)
            let vnpy_available = PyModule::import(py, "vnpy.trader.object").is_ok()
                && PyModule::import(py, "vnpy.trader.constant").is_ok();

            if vnpy_available {
                // Try to create a vnpy BarData object
                if let (Ok(object_module), Ok(constant_module), Ok(datetime_module)) = (
                    PyModule::import(py, "vnpy.trader.object"),
                    PyModule::import(py, "vnpy.trader.constant"),
                    PyModule::import(py, "datetime"),
                ) {
                    if let (Ok(bar_data_class), Ok(exchange_enum), Ok(interval_enum)) = (
                        object_module.getattr("BarData"),
                        constant_module.getattr("Exchange"),
                        constant_module.getattr("Interval"),
                    ) {
                        // Convert Exchange
                        let exchange_val = bar.exchange.value();
                        let py_exchange =
                            exchange_enum.call1((exchange_val,)).unwrap_or_else(|_| {
                                exchange_enum
                                    .getattr("LOCAL")
                                    .unwrap_or_else(|_| py.None().bind(py).clone())
                            });

                        // Convert Interval
                        let interval_val = bar
                            .interval
                            .unwrap_or(crate::trader::constant::Interval::Minute)
                            .value();
                        let py_interval =
                            interval_enum.call1((interval_val,)).unwrap_or_else(|_| {
                                interval_enum
                                    .getattr("MINUTE")
                                    .unwrap_or_else(|_| py.None().bind(py).clone())
                            });

                        // Convert DateTime
                        let dt_str = bar.datetime.to_rfc3339();
                        let py_datetime = datetime_module.getattr("datetime").and_then(|dt_cls| {
                            dt_cls.call_method1("fromisoformat", (dt_str.as_str(),))
                        });

                        if let Ok(py_dt) = py_datetime {
                            // Create vnpy BarData kwargs
                            let kwargs = PyDict::new(py);
                            let _ = kwargs.set_item("gateway_name", &bar.gateway_name);
                            let _ = kwargs.set_item("symbol", &bar.symbol);
                            let _ = kwargs.set_item("exchange", py_exchange);
                            let _ = kwargs.set_item("datetime", py_dt);
                            let _ = kwargs.set_item("interval", py_interval);
                            let _ = kwargs.set_item("volume", bar.volume);
                            let _ = kwargs.set_item("turnover", bar.turnover);
                            let _ = kwargs.set_item("open_interest", bar.open_interest);
                            let _ = kwargs.set_item("open_price", bar.open_price);
                            let _ = kwargs.set_item("high_price", bar.high_price);
                            let _ = kwargs.set_item("low_price", bar.low_price);
                            let _ = kwargs.set_item("close_price", bar.close_price);

                            if let Ok(py_bar) = bar_data_class.call((), Some(&kwargs)) {
                                if let Err(e) = self.call_py_method1("on_bar", py, py_bar.into()) {
                                    eprintln!("on_bar error: {}", e);
                                }
                                return; // Success with vnpy BarData
                            }
                        }
                    }
                }
            }

            // Fallback: send plain dict (works with our Strategy/CtaStrategy base classes)
            if let Err(e) = self.call_py_method_with_dict("on_bar", py, &bar_dict) {
                eprintln!("on_bar error: {}", e);
            }
        });
    }

    fn on_order(&mut self, order: &OrderData) {
        Python::attach(|py| {
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
        Python::attach(|py| {
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
        Python::attach(|py| {
            if let Err(e) = self.call_py_method_with_str("on_stop_order", py, stop_orderid) {
                eprintln!("on_stop_order error: {}", e);
            }
        });
    }

    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        // Drain from the shared Arc<Mutex<Vec<PendingOrder>>> (set during from_py_object)
        let pending: Vec<PendingOrder> = if let Some(ref queue) = self.pending_orders {
            queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain(..)
                .collect()
        } else {
            Vec::new()
        };

        // Convert PendingOrder to OrderRequest
        // Direction/Offset mapping (consistent with apply_fill):
        // - buy:   Long+Open  → delta = +vol (open long)
        // - sell:  Short+Close→ delta = -vol (close long)
        // - short: Short+Open → delta = -vol (open short)
        // - cover: Long+Close → delta = +vol (close short)
        pending
            .into_iter()
            .map(|po| {
                let symbol = po
                    .vt_symbol
                    .split('.')
                    .next()
                    .unwrap_or(&po.vt_symbol)
                    .to_string();
                let (direction, offset) = match po.direction.as_str() {
                    "buy" => (Direction::Long, Offset::Open),
                    "sell" => (Direction::Short, Offset::Close),
                    "short" => (Direction::Short, Offset::Open),
                    "cover" => (Direction::Long, Offset::Close),
                    _ => (Direction::Long, Offset::Open),
                };
                OrderRequest {
                    symbol,
                    exchange: Exchange::Binance,
                    direction,
                    order_type: OrderType::Limit,
                    volume: po.volume,
                    price: po.price,
                    offset,
                    reference: String::new(),
                }
            })
            .collect()
    }

    fn update_position(&mut self, vt_symbol: &str, position: f64) {
        // Update our internal tracking
        self.positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(vt_symbol.to_string(), position);

        // Propagate to the Python Strategy's pos_data so that
        // get_pos() / self.pos (CtaStrategy) return the correct value
        // without needing to call engine.get_pos() (which would deadlock).
        Python::attach(|py| {
            let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
            let _ = strategy.call_method1(py, "set_pos", (vt_symbol, position));
        });
    }

    fn get_position(&self, vt_symbol: &str) -> f64 {
        self.positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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
            for entry in entries.flatten() {
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
                            // Simple search for "class X(CtaTemplate):" or "class X(CtaStrategy):"
                            for line in content.lines() {
                                let line = line.trim();
                                if line.starts_with("class ")
                                    && (line.contains("(CtaTemplate)")
                                        || line.contains("(CtaStrategy)"))
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
        Err(e) => return Err(e.to_string()),
    }

    Ok(strategies)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_load_strategy_file() {
        // This test requires a valid Python strategy file
        // Skip in automated tests
    }
}
