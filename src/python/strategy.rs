//! Python Strategy base class
//!
//! Provides the unified `Strategy` base class that Python users subclass
//! to implement trading strategies. Method stubs are no-ops by default;
//! Python subclasses override them as needed.
//!
//! ## vnpy Compatibility
//! This class provides vnpy CtaTemplate-compatible properties and methods:
//! - `self.vt_symbol` — primary trading symbol (first in vt_symbols)
//! - `self.pos` — current position for the primary symbol
//! - `self.cancel_all()` — cancel all active orders
//! - `self.put_event()` — notify UI of strategy state change
//! - `self.load_bar(days)` — request historical bar data

use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::python::{MessageBus, OrderFactory, PortfolioFacade, PyInstrument, PyStrategyContext};

/// A pending order queued by the strategy during on_bar (to avoid mutex deadlock)
#[derive(Clone)]
pub struct PendingOrder {
    pub vt_symbol: String,
    pub direction: String, // "buy", "sell", "short", "cover"
    pub offset: Option<String>, // "open", "close", "closetoday", "closeyesterday", None (= auto)
    pub price: f64,
    pub volume: f64,
}

/// A pending stop order queued by the strategy during on_bar (to avoid mutex deadlock)
#[derive(Clone)]
pub struct PendingStopOrder {
    pub vt_symbol: String,
    pub direction: String,  // "buy" or "sell"
    pub offset: Option<String>,
    pub price: f64,
    pub volume: f64,
    pub order_type: String, // "stop" or "stop_limit"
    pub stop_price: f64,    // trigger price
}

/// Strategy state as a string property for Python consumers.
/// Maps to the Rust StrategyState enum:
///   "NotInited" → "Inited" → "Trading" → "Stopped"
fn state_to_string(inited: bool, trading: bool, stopped: bool) -> String {
    if stopped {
        "Stopped".to_string()
    } else if trading {
        "Trading".to_string()
    } else if inited {
        "Inited".to_string()
    } else {
        "NotInited".to_string()
    }
}

/// Unified Python Strategy base class.
///
/// Python users subclass this to implement trading strategies:
///
/// ```python
/// class MyStrategy(Strategy):
///     def on_init(self):
///         self.write_log("Strategy initialized")
///
///     def on_bar(self, bar):
///         self.buy("BTCUSDT.BINANCE", bar["close"], 1.0)
/// ```
#[pyclass(subclass)]
pub struct Strategy {
    #[pyo3(get, set)]
    pub strategy_name: String,

    #[pyo3(get, set)]
    pub vt_symbols: Vec<String>,

    /// Strategy type: "spot" or "futures"
    #[pyo3(get, set)]
    pub strategy_type: String,

    // Internal state tracking
    inited: bool,
    trading: bool,
    stopped: bool,

    // Position tracking
    #[pyo3(get)]
    pub pos_data: HashMap<String, f64>,

    #[pyo3(get)]
    pub target_data: HashMap<String, f64>,

    // Strategy parameters and variables (vnpy CtaTemplate compatible)
    #[pyo3(get)]
    pub parameters: HashMap<String, String>,

    #[pyo3(get)]
    pub variables: HashMap<String, String>,

    #[pyo3(get)]
    pub active_orderids: Vec<String>,

    // Engine reference for order routing
    #[pyo3(get, set)]
    pub engine: Option<Py<PyAny>>,

    /// Portfolio facade for querying account/position state
    #[pyo3(get, set)]
    pub portfolio: Option<Py<PortfolioFacade>>,

    /// Order factory for typed order creation
    #[pyo3(get, set)]
    pub order_factory: Option<Py<OrderFactory>>,

    /// Message bus for inter-strategy communication
    #[pyo3(get, set)]
    pub message_bus: Option<Py<MessageBus>>,

    /// Strategy context for market data access (tick/bar caches)
    #[pyo3(get, set)]
    pub context: Option<Py<PyStrategyContext>>,

    /// Pending orders queued during on_bar (to avoid mutex deadlock on BacktestingEngine)
    pending_orders: Arc<Mutex<Vec<PendingOrder>>>,

    /// Pending stop orders queued during on_bar (to avoid mutex deadlock on BacktestingEngine)
    pending_stop_orders: Arc<Mutex<Vec<PendingStopOrder>>>,

    /// Active stop order IDs for tracking
    #[pyo3(get)]
    pub active_stop_orderids: Vec<String>,
}

#[pymethods]
impl Strategy {
    #[new]
    #[pyo3(signature = (strategy_name="UnnamedStrategy".to_string(), vt_symbols=vec!["BTCUSDT.BINANCE".to_string()], strategy_type="spot".to_string()))]
    fn new(strategy_name: String, vt_symbols: Vec<String>, strategy_type: String) -> Self {
        Strategy {
            strategy_name,
            vt_symbols,
            strategy_type,
            inited: false,
            trading: false,
            stopped: false,
            pos_data: HashMap::new(),
            target_data: HashMap::new(),
            parameters: HashMap::new(),
            variables: HashMap::new(),
            active_orderids: Vec::new(),
            engine: None,
            portfolio: None,
            order_factory: None,
            message_bus: None,
            context: None,
            pending_orders: Arc::new(Mutex::new(Vec::new())),
            pending_stop_orders: Arc::new(Mutex::new(Vec::new())),
            active_stop_orderids: Vec::new(),
        }
    }

    /// Current strategy state as a string: "NotInited", "Inited", "Trading", "Stopped"
    #[getter]
    fn state(&self) -> String {
        state_to_string(self.inited, self.trading, self.stopped)
    }

    // ---- vnpy CtaTemplate compatible properties ----

    /// Primary trading symbol (first element of vt_symbols).
    /// This is the vnpy-compatible `self.vt_symbol` property.
    /// Returns the first vt_symbol, or empty string if none set.
    #[getter]
    fn vt_symbol(&self) -> String {
        self.vt_symbols.first().cloned().unwrap_or_default()
    }

    /// Current position for the primary symbol (vnpy-compatible `self.pos`).
    /// Reads from `pos_data` using the primary vt_symbol.
    #[getter]
    fn pos(&self) -> f64 {
        let symbol = self.vt_symbols.first().cloned().unwrap_or_default();
        self.pos_data.get(&symbol).copied().unwrap_or(0.0)
    }

    // ---- Lifecycle callbacks (no-op stubs, override in Python subclass) ----

    /// Initialize the strategy. Override in subclass.
    #[pyo3(signature = ())]
    fn on_init(&self, _py: Python) -> PyResult<()> {
        Ok(())
    }

    /// Start the strategy. Override in subclass.
    #[pyo3(signature = ())]
    fn on_start(&self, _py: Python) -> PyResult<()> {
        Ok(())
    }

    /// Stop the strategy. Override in subclass.
    #[pyo3(signature = ())]
    fn on_stop(&self, _py: Python) -> PyResult<()> {
        Ok(())
    }

    /// Handle tick data update. Override in subclass.
    fn on_tick(&self, _py: Python, _tick: Py<PyAny>) -> PyResult<()> {
        Ok(())
    }

    /// Handle bar data update. Override in subclass.
    fn on_bar(&self, _py: Python, _bar: Py<PyAny>) -> PyResult<()> {
        Ok(())
    }

    /// Handle multi-symbol bars update. Override in subclass.
    fn on_bars(&self, _py: Python, _bars: Py<PyAny>) -> PyResult<()> {
        Ok(())
    }

    /// Handle order update. Override in subclass.
    fn on_order(&self, _py: Python, _order: Py<PyAny>) -> PyResult<()> {
        Ok(())
    }

    /// Handle trade update. Override in subclass.
    fn on_trade(&self, _py: Python, _trade: Py<PyAny>) -> PyResult<()> {
        Ok(())
    }

    /// Handle depth/order book update. Override in subclass.
    fn on_depth(&self, _py: Python, _depth: Py<PyAny>) -> PyResult<()> {
        Ok(())
    }

    // ---- State mutators (called by the engine) ----

    /// Mark strategy as initialized
    pub fn set_inited(&mut self) {
        self.inited = true;
        self.stopped = false;
    }

    /// Mark strategy as trading
    pub fn set_trading(&mut self) {
        self.trading = true;
        self.stopped = false;
    }

    /// Mark strategy as stopped
    pub fn set_stopped(&mut self) {
        self.trading = false;
        self.stopped = true;
    }

    // ---- Convenience methods ----
    // These queue orders on the Strategy object itself. The orders are drained
    // by PythonStrategyAdapter.drain_pending_orders() after each on_bar callback.
    // This avoids the mutex deadlock that would occur if we called back into
    // PyBacktestingEngine (which holds the engine mutex during the backtest loop).

    /// Buy (long direction). Offset defaults to "open" for spot, auto for futures.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     price: Order price
    ///     volume: Order volume
    ///     offset: Offset mode — "open", "close", "closetoday", "closeyesterday", or None (auto)
    #[pyo3(signature = (vt_symbol, price, volume, offset=None))]
    fn buy(&self, vt_symbol: &str, price: f64, volume: f64, offset: Option<&str>) -> PyResult<Vec<String>> {
        if volume <= 0.0 {
            return Ok(vec![]);
        }
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: "buy".to_string(),
                offset: offset.map(|s| s.to_string()),
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Sell (short direction). Offset defaults to "close" for spot, auto for futures.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     price: Order price
    ///     volume: Order volume
    ///     offset: Offset mode — "open", "close", "closetoday", "closeyesterday", or None (auto)
    #[pyo3(signature = (vt_symbol, price, volume, offset=None))]
    fn sell(&self, vt_symbol: &str, price: f64, volume: f64, offset: Option<&str>) -> PyResult<Vec<String>> {
        if volume <= 0.0 {
            return Ok(vec![]);
        }
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: "sell".to_string(),
                offset: offset.map(|s| s.to_string()),
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Short (short open, futures only).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     price: Order price
    ///     volume: Order volume
    ///     offset: Offset mode — "open", "close", "closetoday", "closeyesterday", or None (auto=open)
    #[pyo3(signature = (vt_symbol, price, volume, offset=None))]
    fn short(&self, vt_symbol: &str, price: f64, volume: f64, offset: Option<&str>) -> PyResult<Vec<String>> {
        if volume <= 0.0 {
            return Ok(vec![]);
        }
        if self.strategy_type == "spot" {
            tracing::warn!(
                "[{}] Short not supported for spot trading",
                self.strategy_name
            );
            return Ok(vec![]);
        }
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: "short".to_string(),
                offset: offset.map(|s| s.to_string()),
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Cover (short close, futures only).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     price: Order price
    ///     volume: Order volume
    ///     offset: Offset mode — "open", "close", "closetoday", "closeyesterday", or None (auto=close)
    #[pyo3(signature = (vt_symbol, price, volume, offset=None))]
    fn cover(&self, vt_symbol: &str, price: f64, volume: f64, offset: Option<&str>) -> PyResult<Vec<String>> {
        if volume <= 0.0 {
            return Ok(vec![]);
        }
        if self.strategy_type == "spot" {
            tracing::warn!(
                "[{}] Cover not supported for spot trading",
                self.strategy_name
            );
            return Ok(vec![]);
        }
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: "cover".to_string(),
                offset: offset.map(|s| s.to_string()),
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Send stop order (conditional order that triggers when price reaches stop_price).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "buy" or "sell"
    ///     price: Order price (limit price for stop_limit, ignored for stop)
    ///     volume: Order volume
    ///     stop_price: Trigger price
    ///     offset: Offset mode — "open", "close", "closetoday", "closeyesterday", or None
    ///     order_type: "stop" (market) or "stop_limit" (limit)
    #[pyo3(signature = (vt_symbol, direction, price, volume, stop_price, offset=None, order_type="stop"))]
    fn send_stop_order(
        &self,
        vt_symbol: &str,
        direction: &str,
        price: f64,
        volume: f64,
        stop_price: f64,
        offset: Option<&str>,
        order_type: &str,
    ) -> PyResult<String> {
        if volume <= 0.0 {
            return Ok(String::new());
        }
        let stop_orderid = format!(
            "STOP_{}_{}",
            vt_symbol,
            chrono::Utc::now().timestamp_millis()
        );
        self.pending_stop_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingStopOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: direction.to_string(),
                offset: offset.map(|s| s.to_string()),
                price,
                volume,
                order_type: order_type.to_string(),
                stop_price,
            });
        Ok(stop_orderid)
    }

    /// Handle stop order update. Override in subclass.
    fn on_stop_order(&self, _py: Python, _stop_orderid: String) -> PyResult<()> {
        Ok(())
    }

    /// Handle timer callback. Override in subclass.
    fn on_timer(&self, _py: Python, _timer_id: String) -> PyResult<()> {
        Ok(())
    }

    /// Schedule a timer. `seconds` is the delay (and interval if `repeat=True`).
    #[pyo3(signature = (timer_id, seconds, repeat=false))]
    fn schedule_timer(&self, timer_id: &str, seconds: f64, repeat: bool) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            let strategy_name = self.strategy_name.clone();
            Python::attach(|py| {
                let _ = engine.call_method1(py, "schedule_timer", (strategy_name, timer_id, seconds, repeat));
            });
        }
        Ok(())
    }

    /// Cancel a scheduled timer.
    fn cancel_timer(&self, timer_id: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            let strategy_name = self.strategy_name.clone();
            Python::attach(|py| {
                let _ = engine.call_method1(py, "cancel_timer", (strategy_name, timer_id));
            });
        }
        Ok(())
    }

    /// Cancel stop order.
    fn cancel_stop_order(&self, stop_orderid: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            Python::attach(|py| {
                let _ = engine.call_method1(py, "cancel_stop_order", (stop_orderid,));
            });
        }
        Ok(())
    }

    // ---- Futures convenience methods ----
    // These provide explicit offset semantics, matching vnpy's CtaTemplate
    // buy_open / buy_close / short_open / sell_close naming convention.

    /// Buy to open long position (futures). Equivalent to `buy(symbol, price, volume, offset="open")`.
    #[pyo3(signature = (vt_symbol, price, volume))]
    fn buy_open(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        self.buy(vt_symbol, price, volume, Some("open"))
    }

    /// Buy to close short position (futures). Equivalent to `buy(symbol, price, volume, offset="close")`.
    #[pyo3(signature = (vt_symbol, price, volume))]
    fn buy_close(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        self.buy(vt_symbol, price, volume, Some("close"))
    }

    /// Short to open short position (futures). Equivalent to `short(symbol, price, volume, offset="open")`.
    #[pyo3(signature = (vt_symbol, price, volume))]
    fn short_open(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        self.short(vt_symbol, price, volume, Some("open"))
    }

    /// Sell to close long position (futures). Equivalent to `sell(symbol, price, volume, offset="close")`.
    #[pyo3(signature = (vt_symbol, price, volume))]
    fn sell_close(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        self.sell(vt_symbol, price, volume, Some("close"))
    }

    /// Cancel order
    fn cancel_order(&self, vt_orderid: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            Python::attach(|py| {
                let _ = engine.call_method1(py, "cancel_order", (vt_orderid,));
            });
        }
        Ok(())
    }

    /// Cancel all active orders (vnpy CtaTemplate compatible).
    /// Clears the pending orders queue and requests engine to cancel all.
    fn cancel_all(&self) -> PyResult<()> {
        // Clear pending orders queue
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();

        // Request engine to cancel all orders for this strategy
        if let Some(ref engine) = self.engine {
            Python::attach(|py| {
                let _ = engine.call_method1(py, "cancel_all", ());
            });
        }
        Ok(())
    }

    /// Put strategy event (vnpy CtaTemplate compatible).
    /// Notifies the engine/UI that strategy state has changed.
    /// In backtesting mode, this is a no-op.
    fn put_event(&self) -> PyResult<()> {
        // In live trading, this would trigger a UI update event.
        // In backtesting, it's a no-op since there's no event loop.
        Ok(())
    }

    /// Load historical bar data for strategy initialization
    /// (vnpy CtaTemplate compatible).
    ///
    /// In live mode, this requests the engine to load `days` days of
    /// historical bars and replay them through on_bar().
    /// In backtesting mode, this is a no-op (data is already loaded).
    ///
    /// Args:
    ///     days: Number of days of historical data to load
    #[pyo3(signature = (days, interval="1m"))]
    fn load_bar(&self, days: i32, interval: Option<&str>) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            let interval_str = interval.unwrap_or("1m").to_string();
            let vt_symbol = self.vt_symbols.first().cloned().unwrap_or_default();
            Python::attach(|py| {
                let _ = engine.call_method1(
                    py,
                    "load_bar",
                    (vt_symbol, days, interval_str),
                );
            });
        }
        Ok(())
    }

    /// Load historical tick data for strategy initialization
    /// (vnpy CtaTemplate compatible).
    ///
    /// In live mode, this requests the engine to load `days` days of
    /// historical ticks and replay them through on_tick().
    /// In backtesting mode, this is a no-op.
    #[pyo3(signature = (days))]
    fn load_tick(&self, days: i32) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            let vt_symbol = self.vt_symbols.first().cloned().unwrap_or_default();
            Python::attach(|py| {
                let _ = engine.call_method1(py, "load_tick", (vt_symbol, days));
            });
        }
        Ok(())
    }

    /// Get position for a specific symbol.
    ///
    /// Reads from the local `pos_data` cache which is updated by `on_trade()`.
    /// This avoids calling engine.get_pos() which would deadlock during
    /// backtesting (the engine mutex is held while calling strategy callbacks).
    ///
    /// Note: For the primary symbol, use `self.pos` (property) instead.
    fn get_pos_by_symbol(&self, vt_symbol: &str) -> PyResult<f64> {
        Ok(self.pos_data.get(vt_symbol).copied().unwrap_or(0.0))
    }

    /// Set position for a symbol (called by the engine after trade fills).
    ///
    /// This is the mechanism by which `pos_data` stays in sync with the
    /// engine's position tracking during backtesting.
    fn set_pos(&mut self, vt_symbol: &str, position: f64) -> PyResult<()> {
        self.pos_data.insert(vt_symbol.to_string(), position);
        Ok(())
    }

    // ---- Runtime subscription management ----

    /// Subscribe to market data for a symbol at runtime.
    ///
    /// In live trading, this sends a WebSocket subscription request to the gateway.
    /// In backtesting mode, this is a no-op (data is preloaded).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "btcusdt.binance")
    fn subscribe(&self, vt_symbol: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            let strategy_name = self.strategy_name.clone();
            let vt_symbol = vt_symbol.to_string();
            Python::attach(|py| {
                let _ = engine.call_method1(py, "subscribe", (strategy_name, vt_symbol));
            });
        }
        Ok(())
    }

    /// Unsubscribe from market data for a symbol at runtime.
    ///
    /// In live trading, this sends a WebSocket unsubscription request to the gateway
    /// if no other strategies are subscribed to the same symbol.
    /// In backtesting mode, this is a no-op.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "btcusdt.binance")
    fn unsubscribe(&self, vt_symbol: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            let strategy_name = self.strategy_name.clone();
            let vt_symbol = vt_symbol.to_string();
            Python::attach(|py| {
                let _ = engine.call_method1(py, "unsubscribe", (strategy_name, vt_symbol));
            });
        }
        Ok(())
    }

    // ---- Parameter and variable access (vnpy CtaTemplate compatible) ----

    /// Get a strategy parameter by key.
    ///
    /// Returns the parameter value if found, or `default` if not.
    /// Args:
    ///     key: Parameter name
    ///     default: Default value if key not found (None by default)
    #[pyo3(signature = (key, default=None))]
    fn get_parameter(&self, key: &str, default: Option<&str>) -> PyResult<Option<String>> {
        Ok(self.parameters.get(key).cloned().or(default.map(|s| s.to_string())))
    }

    /// Set a strategy parameter.
    ///
    /// Args:
    ///     key: Parameter name
    ///     value: Parameter value
    fn set_parameter(&mut self, key: &str, value: &str) -> PyResult<()> {
        self.parameters.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Get a strategy variable by key.
    ///
    /// Returns the variable value if found, or `default` if not.
    /// Args:
    ///     key: Variable name
    ///     default: Default value if key not found (None by default)
    #[pyo3(signature = (key, default=None))]
    fn get_variable(&self, key: &str, default: Option<&str>) -> PyResult<Option<String>> {
        Ok(self.variables.get(key).cloned().or(default.map(|s| s.to_string())))
    }

    /// Set a strategy variable.
    ///
    /// Args:
    ///     key: Variable name
    ///     value: Variable value
    fn set_variable(&mut self, key: &str, value: &str) -> PyResult<()> {
        self.variables.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Insert a strategy parameter (called by engine when loading strategy settings).
    ///
    /// This is an alias for `set_parameter` used by the engine to populate
    /// strategy parameters from configuration.
    fn insert_parameter(&mut self, key: &str, value: &str) -> PyResult<()> {
        self.parameters.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Insert a strategy variable (called by engine to set computed variables).
    ///
    /// This is an alias for `set_variable` used by the engine to populate
    /// strategy variables.
    fn insert_variable(&mut self, key: &str, value: &str) -> PyResult<()> {
        self.variables.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Load strategy settings from a dict into parameters.
    ///
    /// Iterates over the setting dict and calls `insert_parameter` for each
    /// entry. This is how vnpy's CtaTemplate loads strategy settings.
    fn load_setting(&mut self, setting: HashMap<String, String>) -> PyResult<()> {
        for (key, value) in setting {
            self.parameters.insert(key, value);
        }
        Ok(())
    }

    /// Write log message
    fn write_log(&self, msg: &str) -> PyResult<()> {
        tracing::info!("[策略:{}] {}", self.strategy_name, msg);
        Ok(())
    }

    /// Get instrument metadata for a symbol.
    ///
    /// Delegates to the engine's `get_instrument` method.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "btcusdt.binance")
    ///
    /// Returns:
    ///     PyInstrument if found, None otherwise
    fn get_instrument(&self, py: Python, vt_symbol: String) -> PyResult<Option<Py<PyInstrument>>> {
        if let Some(engine_ref) = &self.engine {
            let result = engine_ref.call_method1(py, "get_instrument", (vt_symbol,))?;
            let is_none = result.is_none(py);
            if is_none {
                Ok(None)
            } else {
                let instr: Py<PyInstrument> = result.extract(py)?;
                Ok(Some(instr))
            }
        } else {
            Ok(None)
        }
    }

    /// Send email notification
    fn send_email(&self, msg: &str) -> PyResult<()> {
        tracing::warn!(
            "[策略:{}] send_email called but email is not configured: {}",
            self.strategy_name,
            msg
        );
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "Email is not configured. Please set up SMTP settings first.",
        ))
    }
}

impl Strategy {
    /// Get the pending orders queue (for PythonStrategyAdapter to drain)
    pub fn pending_orders_arc(&self) -> Arc<Mutex<Vec<PendingOrder>>> {
        Arc::clone(&self.pending_orders)
    }

    /// Get the pending stop orders queue (for PythonStrategyAdapter to drain)
    pub fn pending_stop_orders_arc(&self) -> Arc<Mutex<Vec<PendingStopOrder>>> {
        Arc::clone(&self.pending_stop_orders)
    }
}
