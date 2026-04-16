//! Python Strategy base class
//!
//! Provides the unified `Strategy` base class that Python users subclass
//! to implement trading strategies. Method stubs are no-ops by default;
//! Python subclasses override them as needed.

use pyo3::prelude::*;
use std::collections::HashMap;

use crate::python::{MessageBus, OrderFactory, PortfolioFacade};

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
    #[pyo3(get)]
    pub portfolio: Option<Py<PortfolioFacade>>,

    /// Order factory for typed order creation
    #[pyo3(get)]
    pub order_factory: Option<Py<OrderFactory>>,

    /// Message bus for inter-strategy communication
    #[pyo3(get)]
    pub message_bus: Option<Py<MessageBus>>,
}

#[pymethods]
impl Strategy {
    #[new]
    #[pyo3(signature = (strategy_name, vt_symbols, strategy_type="spot"))]
    fn new(strategy_name: String, vt_symbols: Vec<String>, strategy_type: Option<&str>) -> Self {
        Strategy {
            strategy_name,
            vt_symbols,
            strategy_type: strategy_type.unwrap_or("spot").to_string(),
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
        }
    }

    /// Current strategy state as a string: "NotInited", "Inited", "Trading", "Stopped"
    #[getter]
    fn state(&self) -> String {
        state_to_string(self.inited, self.trading, self.stopped)
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

    /// Inject the portfolio facade (called by the engine when adding the strategy)
    pub fn set_portfolio(&mut self, portfolio: Py<PortfolioFacade>) {
        self.portfolio = Some(portfolio);
    }

    /// Inject the order factory (called by the engine when adding the strategy)
    pub fn set_order_factory(&mut self, factory: Py<OrderFactory>) {
        self.order_factory = Some(factory);
    }

    // ---- Convenience methods (delegate to engine) ----

    /// Buy (long open)
    fn buy(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "buy", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }

    /// Sell (long close)
    fn sell(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "sell", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }

    /// Short (short open, futures only)
    fn short(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if self.strategy_type == "spot" {
            tracing::warn!(
                "[{}] Short not supported for spot trading",
                self.strategy_name
            );
            return Ok(vec![]);
        }
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "short", (vt_symbol, price, volume))?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }

    /// Cover (short close, futures only)
    fn cover(&self, py: Python, vt_symbol: &str, price: f64, volume: f64) -> PyResult<Vec<String>> {
        if self.strategy_type == "spot" {
            tracing::warn!(
                "[{}] Cover not supported for spot trading",
                self.strategy_name
            );
            return Ok(vec![]);
        }
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

    /// Get position for a symbol
    fn get_pos(&self, py: Python, vt_symbol: &str) -> PyResult<f64> {
        if let Some(ref engine) = self.engine {
            let result = engine.call_method1(py, "get_pos", (vt_symbol,))?;
            Ok(result.extract(py)?)
        } else {
            Ok(0.0)
        }
    }

    /// Write log message
    fn write_log(&self, py: Python, msg: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            engine.call_method1(py, "write_log", (msg,))?;
        }
        Ok(())
    }

    /// Send email notification
    fn send_email(&self, py: Python, msg: &str) -> PyResult<()> {
        if let Some(ref engine) = self.engine {
            engine.call_method1(py, "send_email", (msg,))?;
        }
        Ok(())
    }
}
