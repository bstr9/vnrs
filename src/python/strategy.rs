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

use crate::python::{MessageBus, OrderFactory, PortfolioFacade};

/// A pending order queued by the strategy during on_bar (to avoid mutex deadlock)
#[derive(Clone)]
pub struct PendingOrder {
    pub vt_symbol: String,
    pub direction: String, // "buy", "sell", "short", "cover"
    pub price: f64,
    pub volume: f64,
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

    /// Pending orders queued during on_bar (to avoid mutex deadlock on BacktestingEngine)
    pending_orders: Arc<Mutex<Vec<PendingOrder>>>,
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
            active_orderids: Vec::new(),
            engine: None,
            portfolio: None,
            order_factory: None,
            message_bus: None,
            pending_orders: Arc::new(Mutex::new(Vec::new())),
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

    /// Buy (long open)
    fn buy(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if volume <= 0.0 {
            return Ok(vec![]);
        }
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: "buy".to_string(),
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Sell (long close)
    fn sell(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if volume <= 0.0 {
            return Ok(vec![]);
        }
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(PendingOrder {
                vt_symbol: vt_symbol.to_string(),
                direction: "sell".to_string(),
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Short (short open, futures only)
    fn short(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
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
                price,
                volume,
            });
        Ok(vec![])
    }

    /// Cover (short close, futures only)
    fn cover(&self, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
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
                price,
                volume,
            });
        Ok(vec![])
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

    /// Write log message
    fn write_log(&self, msg: &str) -> PyResult<()> {
        println!("[Strategy Log] {}", msg);
        Ok(())
    }

    /// Send email notification
    fn send_email(&self, msg: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            Python::attach(|py| {
                let _ = engine.call_method1(py, "send_email", (msg,));
            });
        }
        Ok(())
    }
}

impl Strategy {
    /// Get the pending orders queue (for PythonStrategyAdapter to drain)
    pub fn pending_orders_arc(&self) -> Arc<Mutex<Vec<PendingOrder>>> {
        Arc::clone(&self.pending_orders)
    }
}
