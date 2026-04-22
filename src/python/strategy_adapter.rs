//! Python Strategy Adapter
//!
//! Adapts Python strategies to work with Rust StrategyTemplate trait

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tracing::{error, warn};

use crate::python::strategy::{PendingOrder, PendingStopOrder};
use crate::strategy::{StrategyContext, StrategyState, StrategyTemplate, StrategyType, StopOrderRequest};
use crate::trader::{
    BarData, DepthData, Direction, Exchange, Offset, OrderData, OrderRequest, OrderType, TickData, TradeData,
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

    /// Pending stop orders from Python strategy (shared with Strategy.pending_stop_orders)
    pending_stop_orders: Option<Arc<Mutex<Vec<PendingStopOrder>>>>,
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
                pending_stop_orders: None, // Will be set if the instance is a Strategy
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
        let (pending_orders, pending_stop_orders) = Python::attach(|py| {
            use crate::python::Strategy;
            py_strategy
                .cast_bound::<Strategy>(py)
                .ok()
                .map(|bound| {
                    let borrowed = bound.borrow();
                    (borrowed.pending_orders_arc(), borrowed.pending_stop_orders_arc())
                })
                .unzip()
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
            pending_stop_orders,
        }
    }

    /// Call Python method with error handling
    #[allow(dead_code)]
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
            error!("策略 {} on_init 失败: {}", self.strategy_name, e);
            self.state = StrategyState::Error;
            return;
        }
        self.state = StrategyState::Inited;
    }

    fn on_start(&mut self) {
        if let Err(e) = self.call_py_method_no_args("on_start") {
            error!("策略 {} on_start 失败，自动停止: {}", self.strategy_name, e);
            self.state = StrategyState::Stopped;
            return;
        }
        self.state = StrategyState::Trading;
    }

    fn on_stop(&mut self) {
        if let Err(e) = self.call_py_method_no_args("on_stop") {
            warn!("策略 {} on_stop 错误: {}", self.strategy_name, e);
        }
        self.state = StrategyState::Stopped;
    }

    fn on_reset(&mut self) {
        // Try to call on_reset on the Python strategy.
        // If the Python subclass doesn't define on_reset, this will call
        // the base Strategy.on_reset() which is a no-op — that's fine.
        if let Err(e) = self.call_py_method_no_args("on_reset") {
            warn!("策略 {} on_reset 错误: {}", self.strategy_name, e);
        }
        // Clear internal tracking state
        self.positions.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.variables.lock().unwrap_or_else(|e| e.into_inner()).clear();
        // Clear pending orders/stop orders
        if let Some(ref queue) = self.pending_orders {
            queue.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        if let Some(ref queue) = self.pending_stop_orders {
            queue.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
        // Reset Python strategy's pos_data and state
        Python::attach(|py| {
            let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
            // Clear positions in the Python Strategy object
            let _ = strategy.call_method1(py, "set_pos", ("", 0.0));
            // Reset internal state flags to Inited
            let _ = strategy.call_method0(py, "set_inited");
        });
        self.state = StrategyState::Inited;
    }

    fn on_tick(&mut self, tick: &TickData, _context: &StrategyContext) {
        Python::attach(|py| {
            let py_tick = crate::python::data_types::PyTickData::from_rust(tick);
            match Py::new(py, py_tick) {
                Ok(py_tick_obj) => {
                    if let Err(e) = self.call_py_method1("on_tick", py, py_tick_obj.into_any()) {
                        warn!("策略 {} on_tick 错误: {}", self.strategy_name, e);
                    }
                }
                Err(e) => error!("策略 {} 创建 PyTickData 失败: {}", self.strategy_name, e),
            }
        });
    }

    fn on_bar(&mut self, bar: &BarData, _context: &StrategyContext) {
        Python::attach(|py| {
            // Use PyBarData which supports both attribute and dict-style access
            // (vnpy BarData doesn't support bar["key"] syntax)
            let py_bar = crate::python::backtesting_bindings::PyBarData {
                gateway_name: bar.gateway_name.clone(),
                symbol: bar.symbol.clone(),
                exchange: bar.exchange.value().to_string(),
                datetime: bar.datetime.to_rfc3339(),
                interval: bar.interval.unwrap_or(crate::trader::constant::Interval::Minute).value().to_string(),
                open_price: bar.open_price,
                high_price: bar.high_price,
                low_price: bar.low_price,
                close_price: bar.close_price,
                volume: bar.volume,
                turnover: bar.turnover,
                open_interest: bar.open_interest,
            };
            match Py::new(py, py_bar) {
                Ok(py_bar_obj) => {
                    if let Err(e) = self.call_py_method1("on_bar", py, py_bar_obj.into_any()) {
                        warn!("策略 {} on_bar 错误: {}", self.strategy_name, e);
                    }
                }
                Err(e) => error!("策略 {} 创建 PyBarData 失败: {}", self.strategy_name, e),
            }
        });
    }

    fn on_order(&mut self, order: &OrderData) {
        Python::attach(|py| {
            let py_order = crate::python::data_types::PyOrderData::from_rust(order);
            match Py::new(py, py_order) {
                Ok(py_order_obj) => {
                    if let Err(e) = self.call_py_method1("on_order", py, py_order_obj.into_any()) {
                        warn!("策略 {} on_order 错误: {}", self.strategy_name, e);
                    }
                }
                Err(e) => error!("策略 {} 创建 PyOrderData 失败: {}", self.strategy_name, e),
            }
        });
    }

    fn on_trade(&mut self, trade: &TradeData) {
        Python::attach(|py| {
            let py_trade = crate::python::data_types::PyTradeData::from_rust(trade);
            match Py::new(py, py_trade) {
                Ok(py_trade_obj) => {
                    if let Err(e) = self.call_py_method1("on_trade", py, py_trade_obj.into_any()) {
                        warn!("策略 {} on_trade 错误: {}", self.strategy_name, e);
                    }
                }
                Err(e) => error!("策略 {} 创建 PyTradeData 失败: {}", self.strategy_name, e),
            }
        });
    }

    fn on_depth(&mut self, depth: &DepthData, _context: &StrategyContext) {
        Python::attach(|py| {
            let py_depth = crate::python::data_types::PyDepthData::from_rust(depth);
            match Py::new(py, py_depth) {
                Ok(py_depth_obj) => {
                    if let Err(e) = self.call_py_method1("on_depth", py, py_depth_obj.into_any()) {
                        warn!("策略 {} on_depth 错误: {}", self.strategy_name, e);
                    }
                }
                Err(e) => error!("策略 {} 创建 PyDepthData 失败: {}", self.strategy_name, e),
            }
        });
    }

    fn on_stop_order(&mut self, stop_orderid: &str) {
        Python::attach(|py| {
            if let Err(e) = self.call_py_method_with_str("on_stop_order", py, stop_orderid) {
                warn!("策略 {} on_stop_order 错误: {}", self.strategy_name, e);
            }
        });
    }

    fn on_indicator(&mut self, name: &str, value: f64) {
        Python::attach(|py| {
            let strategy = self.py_strategy.lock().unwrap_or_else(|e| e.into_inner());
            let _ = strategy.call_method1(py, "on_indicator", (name, value));
        });
    }

    fn on_timer(&mut self, timer_id: &str) {
        Python::attach(|py| {
            if let Err(e) = self.call_py_method_with_str("on_timer", py, timer_id) {
                warn!("策略 {} on_timer 错误: {}", self.strategy_name, e);
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
        // Direction/Offset mapping:
        // - buy:   Long direction, offset auto=Open (overridable)
        // - sell:  Short direction, offset auto=Close (overridable)
        // - short: Short direction, offset auto=Open (overridable)
        // - cover: Long direction, offset auto=Close (overridable)
        // When po.offset is explicitly set, it takes precedence over the default.
        pending
            .into_iter()
            .map(|po| {
                let (symbol, exchange) = crate::trader::utility::extract_vt_symbol(&po.vt_symbol)
                    .unwrap_or((po.vt_symbol.clone(), Exchange::Binance));

                let (direction, default_offset) = match po.direction.as_str() {
                    "buy" => (Direction::Long, Offset::Open),
                    "sell" => (Direction::Short, Offset::Close),
                    "short" => (Direction::Short, Offset::Open),
                    "cover" => (Direction::Long, Offset::Close),
                    _ => (Direction::Long, Offset::Open),
                };

                // Use explicit offset if provided, otherwise use the default
                let offset = match po.offset.as_deref() {
                    Some("open") => Offset::Open,
                    Some("close") => Offset::Close,
                    Some("closetoday") => Offset::CloseToday,
                    Some("closeyesterday") => Offset::CloseYesterday,
                    _ => default_offset,
                };

                OrderRequest {
                    symbol,
                    exchange,
                    direction,
                    order_type: OrderType::Limit,
                    volume: po.volume,
                    price: po.price,
                    offset,
                    reference: String::new(),
                    post_only: false,
                    reduce_only: false,
                    expire_time: None,
                    gateway_name: String::new(),
                }
            })
            .collect()
    }

    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        // Drain from the shared Arc<Mutex<Vec<PendingStopOrder>>>
        let pending: Vec<PendingStopOrder> = if let Some(ref queue) = self.pending_stop_orders {
            queue
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain(..)
                .collect()
        } else {
            Vec::new()
        };

        // Convert PendingStopOrder to StopOrderRequest
        pending
            .into_iter()
            .map(|po| {
                let direction = match po.direction.as_str() {
                    "buy" => Direction::Long,
                    "sell" => Direction::Short,
                    _ => Direction::Long,
                };

                let offset = po.offset.as_deref().map(|s| match s {
                    "open" => Offset::Open,
                    "close" => Offset::Close,
                    "closetoday" => Offset::CloseToday,
                    "closeyesterday" => Offset::CloseYesterday,
                    _ => Offset::None,
                });

                let order_type = match po.order_type.as_str() {
                    "stop" => OrderType::Stop,
                    "stop_limit" => OrderType::StopLimit,
                    _ => OrderType::Stop,
                };

                StopOrderRequest {
                    vt_symbol: po.vt_symbol,
                    direction,
                    offset,
                    price: po.stop_price,
                    volume: po.volume,
                    order_type,
                    limit_price: if order_type == OrderType::StopLimit {
                        Some(po.price)
                    } else {
                        None
                    },
                    lock: false,
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
