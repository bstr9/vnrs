//! OrderFactory and PyOrder — typed order creation for Python strategies
//!
//! Provides `OrderFactory` with `market()`, `limit()`, `stop()`, `stop_limit()` methods
//! that return typed `PyOrder` objects. `PyOrder` supports a builder pattern for optional
//! fields and a `submit()` method to send the order through the engine.

use crate::trader::constant::{Direction, Offset, OrderType};
use crate::trader::identifier::InstrumentId;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Side parsing
// ---------------------------------------------------------------------------

/// Parse a side string into a Rust `Direction`.
///
/// Accepted values (case-insensitive):
/// - "BUY" or "LONG" → `Direction::Long`
/// - "SELL" or "SHORT" → `Direction::Short`
fn parse_side(side: &str) -> PyResult<Direction> {
    match side.to_uppercase().as_str() {
        "BUY" | "LONG" => Ok(Direction::Long),
        "SELL" | "SHORT" => Ok(Direction::Short),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid side '{}': expected BUY, SELL, LONG, or SHORT",
            side
        ))),
    }
}

/// Convert a Rust `Direction` back to a canonical side string.
fn direction_to_side(dir: Direction) -> &'static str {
    match dir {
        Direction::Long => "BUY",
        Direction::Short => "SELL",
        Direction::Net => "BUY", // Net shouldn't appear for orders; default to BUY
    }
}

/// Convert a Rust `OrderType` to the canonical Python-side string.
fn order_type_to_str(ot: OrderType) -> &'static str {
    match ot {
        OrderType::Limit => "LIMIT",
        OrderType::Market => "MARKET",
        OrderType::Stop => "STOP",
        OrderType::StopLimit => "STOP_LIMIT",
        OrderType::Fak => "FAK",
        OrderType::Fok => "FOK",
        OrderType::Rfq => "RFQ",
        OrderType::Etf => "ETF",
    }
}

// ---------------------------------------------------------------------------
// PyOrder
// ---------------------------------------------------------------------------

/// Typed order object returned by `OrderFactory`.
///
/// Python usage:
/// ```python
/// order = self.order_factory.market("BTCUSDT.BINANCE", 0.1, "BUY")
/// order = self.order_factory.limit("BTCUSDT.BINANCE", 50000.0, 0.1, "BUY")
/// order = self.order_factory.stop("BTCUSDT.BINANCE", 48000.0, 0.1, "SELL")
/// order = self.order_factory.stop_limit("BTCUSDT.BINANCE", 48000.0, 47900.0, 0.1, "SELL")
///
/// # Builder pattern for optional fields
/// order.with_reference("my_strategy").with_client_order_id("order_1").submit()
/// ```
#[pyclass]
pub struct PyOrder {
    // Core fields
    instrument_id: String,
    quantity: f64,
    side: Direction,
    order_type: OrderType,

    // Optional price fields
    price: Option<f64>,
    trigger_price: Option<f64>,
    limit_price: Option<f64>,

    // Offset (futures: Open/Close/CloseToday/CloseYesterday, spot: None)
    offset: Offset,

    // Time-in-force
    time_in_force: Option<String>,

    // Metadata
    client_order_id: Option<String>,
    reference: String,

    // Engine reference for submit
    engine: Option<Py<PyAny>>,
}

impl PyOrder {
    /// Clone the engine reference, which requires a Python GIL.
    fn clone_engine(&self, py: Python) -> Option<Py<PyAny>> {
        self.engine.as_ref().map(|e| e.clone_ref(py))
    }
}

#[pymethods]
impl PyOrder {
    // ---- Getters ----

    /// Instrument identifier in SYMBOL.EXCHANGE format
    #[getter]
    fn instrument_id(&self) -> &str {
        &self.instrument_id
    }

    /// Order quantity
    #[getter]
    fn quantity(&self) -> f64 {
        self.quantity
    }

    /// Order side: "BUY" or "SELL"
    #[getter]
    fn side(&self) -> &str {
        direction_to_side(self.side)
    }

    /// Order type: "MARKET", "LIMIT", "STOP", "STOP_LIMIT"
    #[getter]
    fn order_type(&self) -> &str {
        order_type_to_str(self.order_type)
    }

    /// Limit price (for LIMIT and STOP_LIMIT orders)
    #[getter]
    fn price(&self) -> Option<f64> {
        self.price
    }

    /// Trigger price (for STOP and STOP_LIMIT orders)
    #[getter]
    fn trigger_price(&self) -> Option<f64> {
        self.trigger_price
    }

    /// Limit price for STOP_LIMIT orders
    #[getter]
    fn limit_price(&self) -> Option<f64> {
        self.limit_price
    }

    /// Offset: "NONE", "OPEN", "CLOSE", "CLOSE_TODAY", "CLOSE_YESTERDAY"
    #[getter]
    fn offset(&self) -> &str {
        match self.offset {
            Offset::None => "NONE",
            Offset::Open => "OPEN",
            Offset::Close => "CLOSE",
            Offset::CloseToday => "CLOSE_TODAY",
            Offset::CloseYesterday => "CLOSE_YESTERDAY",
        }
    }

    /// Time-in-force: "GTC", "IOC", "FOK", or None
    #[getter]
    fn time_in_force(&self) -> Option<&str> {
        self.time_in_force.as_deref()
    }

    /// Client-assigned order ID
    #[getter]
    fn client_order_id(&self) -> Option<&str> {
        self.client_order_id.as_deref()
    }

    /// Strategy reference tag
    #[getter]
    fn reference(&self) -> &str {
        &self.reference
    }

    // ---- Builder pattern ----

    /// Set the reference tag. Returns a new PyOrder (builder pattern).
    fn with_reference(&self, py: Python, reference: &str) -> Self {
        PyOrder {
            instrument_id: self.instrument_id.clone(),
            quantity: self.quantity,
            side: self.side,
            order_type: self.order_type,
            price: self.price,
            trigger_price: self.trigger_price,
            limit_price: self.limit_price,
            offset: self.offset,
            time_in_force: self.time_in_force.clone(),
            client_order_id: self.client_order_id.clone(),
            reference: reference.to_string(),
            engine: self.clone_engine(py),
        }
    }

    /// Set the client order ID. Returns a new PyOrder (builder pattern).
    fn with_client_order_id(&self, py: Python, id: &str) -> Self {
        PyOrder {
            instrument_id: self.instrument_id.clone(),
            quantity: self.quantity,
            side: self.side,
            order_type: self.order_type,
            price: self.price,
            trigger_price: self.trigger_price,
            limit_price: self.limit_price,
            offset: self.offset,
            time_in_force: self.time_in_force.clone(),
            client_order_id: Some(id.to_string()),
            reference: self.reference.clone(),
            engine: self.clone_engine(py),
        }
    }

    /// Set the time-in-force. Returns a new PyOrder (builder pattern).
    fn with_time_in_force(&self, py: Python, tif: &str) -> Self {
        PyOrder {
            instrument_id: self.instrument_id.clone(),
            quantity: self.quantity,
            side: self.side,
            order_type: self.order_type,
            price: self.price,
            trigger_price: self.trigger_price,
            limit_price: self.limit_price,
            offset: self.offset,
            time_in_force: Some(tif.to_string()),
            client_order_id: self.client_order_id.clone(),
            reference: self.reference.clone(),
            engine: self.clone_engine(py),
        }
    }

    /// Set the offset. Returns a new PyOrder (builder pattern).
    ///
    /// Accepts: "NONE", "OPEN", "CLOSE", "CLOSE_TODAY", "CLOSE_YESTERDAY" (case-insensitive)
    fn with_offset(&self, py: Python, offset: &str) -> PyResult<Self> {
        let parsed = match offset.to_uppercase().as_str() {
            "NONE" => Offset::None,
            "OPEN" => Offset::Open,
            "CLOSE" => Offset::Close,
            "CLOSE_TODAY" => Offset::CloseToday,
            "CLOSE_YESTERDAY" => Offset::CloseYesterday,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid offset '{}': expected NONE, OPEN, CLOSE, CLOSE_TODAY, or CLOSE_YESTERDAY",
                    offset
                )));
            }
        };
        Ok(PyOrder {
            instrument_id: self.instrument_id.clone(),
            quantity: self.quantity,
            side: self.side,
            order_type: self.order_type,
            price: self.price,
            trigger_price: self.trigger_price,
            limit_price: self.limit_price,
            offset: parsed,
            time_in_force: self.time_in_force.clone(),
            client_order_id: self.client_order_id.clone(),
            reference: self.reference.clone(),
            engine: self.clone_engine(py),
        })
    }

    // ---- Actions ----

    /// Submit this order through the engine.
    ///
    /// Returns a list of vt_orderid strings on success, or an empty list if
    /// the engine is not available.
    fn submit(&self, py: Python) -> PyResult<Vec<String>> {
        if let Some(ref engine) = self.engine {
            // Delegate to the Python engine's `send_order_typed` method
            // which accepts (vt_symbol, direction_str, offset_str, price, volume, order_type_str)
            let direction_str = direction_to_side(self.side);
            let offset_str = match self.offset {
                Offset::None => "NONE",
                Offset::Open => "OPEN",
                Offset::Close => "CLOSE",
                Offset::CloseToday => "CLOSE_TODAY",
                Offset::CloseYesterday => "CLOSE_YESTERDAY",
            };
            let order_type_str = order_type_to_str(self.order_type);
            let price = self.price.unwrap_or(0.0);
            let result = engine.call_method1(
                py,
                "send_order_typed",
                (
                    &self.instrument_id,
                    direction_str,
                    offset_str,
                    price,
                    self.quantity,
                    order_type_str,
                ),
            )?;
            Ok(result.extract(py)?)
        } else {
            Ok(vec![])
        }
    }

    /// Convert to a Python dict for serialization.
    fn to_dict(&self, py: Python) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("instrument_id", &self.instrument_id)?;
        dict.set_item("quantity", self.quantity)?;
        dict.set_item("side", self.side())?;
        dict.set_item("order_type", self.order_type())?;
        dict.set_item("price", self.price)?;
        dict.set_item("trigger_price", self.trigger_price)?;
        dict.set_item("limit_price", self.limit_price)?;
        dict.set_item("offset", self.offset())?;
        dict.set_item("time_in_force", self.time_in_force.clone())?;
        dict.set_item("client_order_id", self.client_order_id.clone())?;
        dict.set_item("reference", &self.reference)?;
        Ok(dict.unbind())
    }

    /// String representation
    fn __repr__(&self) -> String {
        match self.order_type {
            OrderType::Market => format!(
                "PyOrder({} {} @ MARKET x{})",
                self.side(),
                self.instrument_id,
                self.quantity
            ),
            OrderType::Limit => format!(
                "PyOrder({} {} @ {} x{} LIMIT)",
                self.side(),
                self.instrument_id,
                self.price
                    .map(|p| format!("{}", p))
                    .unwrap_or_else(|| "N/A".to_string()),
                self.quantity
            ),
            OrderType::Stop => format!(
                "PyOrder({} {} STOP@{} x{})",
                self.side(),
                self.instrument_id,
                self.trigger_price
                    .map(|p| format!("{}", p))
                    .unwrap_or_else(|| "N/A".to_string()),
                self.quantity
            ),
            OrderType::StopLimit => format!(
                "PyOrder({} {} STOP_LIMIT@{} limit={} x{})",
                self.side(),
                self.instrument_id,
                self.trigger_price
                    .map(|p| format!("{}", p))
                    .unwrap_or_else(|| "N/A".to_string()),
                self.limit_price
                    .map(|p| format!("{}", p))
                    .unwrap_or_else(|| "N/A".to_string()),
                self.quantity
            ),
            _ => format!(
                "PyOrder({} {} {:?} x{})",
                self.side(),
                self.instrument_id,
                self.order_type,
                self.quantity
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// OrderFactory
// ---------------------------------------------------------------------------

/// Factory for creating typed order objects.
///
/// Python usage:
/// ```python
/// factory = OrderFactory(engine, "BINANCE_SPOT")
/// order = factory.market("BTCUSDT.BINANCE", 0.1, "BUY")
/// order = factory.limit("BTCUSDT.BINANCE", 50000.0, 0.1, "SELL")
/// order = factory.stop("BTCUSDT.BINANCE", 48000.0, 0.1, "SELL")
/// order = factory.stop_limit("BTCUSDT.BINANCE", 48000.0, 47900.0, 0.1, "SELL")
/// ```
#[pyclass]
pub struct OrderFactory {
    /// Reference to the Python engine for order submission
    engine: Option<Py<PyAny>>,
    /// Default gateway name
    gateway_name: String,
}

impl OrderFactory {
    /// Create an OrderFactory from Rust code with an engine reference.
    /// This is used by Strategy.order_factory() and PythonEngineWrapper.create_order_factory().
    pub fn from_engine(engine: Py<PyAny>, gateway_name: &str) -> Self {
        OrderFactory {
            engine: Some(engine),
            gateway_name: gateway_name.to_string(),
        }
    }

    /// Create an OrderFactory with no engine (for testing).
    pub fn empty() -> Self {
        OrderFactory {
            engine: None,
            gateway_name: String::new(),
        }
    }

    /// Create a market order (Rust-callable, no Python GIL needed for engine-less factory).
    fn build_market_order(
        &self,
        instrument_id: &str,
        quantity: f64,
        side: &str,
        engine: Option<Py<PyAny>>,
    ) -> PyResult<PyOrder> {
        let _ = InstrumentId::from_str(instrument_id).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid instrument_id '{}': {}",
                instrument_id, e
            ))
        })?;
        let direction = parse_side(side)?;
        Ok(PyOrder {
            instrument_id: instrument_id.to_string(),
            quantity,
            side: direction,
            order_type: OrderType::Market,
            price: None,
            trigger_price: None,
            limit_price: None,
            offset: Offset::None,
            time_in_force: None,
            client_order_id: None,
            reference: String::new(),
            engine,
        })
    }

    /// Create a limit order (Rust-callable).
    fn build_limit_order(
        &self,
        instrument_id: &str,
        price: f64,
        quantity: f64,
        side: &str,
        engine: Option<Py<PyAny>>,
    ) -> PyResult<PyOrder> {
        let _ = InstrumentId::from_str(instrument_id).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid instrument_id '{}': {}",
                instrument_id, e
            ))
        })?;
        let direction = parse_side(side)?;
        Ok(PyOrder {
            instrument_id: instrument_id.to_string(),
            quantity,
            side: direction,
            order_type: OrderType::Limit,
            price: Some(price),
            trigger_price: None,
            limit_price: None,
            offset: Offset::None,
            time_in_force: None,
            client_order_id: None,
            reference: String::new(),
            engine,
        })
    }

    /// Create a stop order (Rust-callable).
    fn build_stop_order(
        &self,
        instrument_id: &str,
        trigger_price: f64,
        quantity: f64,
        side: &str,
        engine: Option<Py<PyAny>>,
    ) -> PyResult<PyOrder> {
        let _ = InstrumentId::from_str(instrument_id).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid instrument_id '{}': {}",
                instrument_id, e
            ))
        })?;
        let direction = parse_side(side)?;
        Ok(PyOrder {
            instrument_id: instrument_id.to_string(),
            quantity,
            side: direction,
            order_type: OrderType::Stop,
            price: None,
            trigger_price: Some(trigger_price),
            limit_price: None,
            offset: Offset::None,
            time_in_force: None,
            client_order_id: None,
            reference: String::new(),
            engine,
        })
    }

    /// Create a stop-limit order (Rust-callable).
    fn build_stop_limit_order(
        &self,
        instrument_id: &str,
        trigger_price: f64,
        limit_price: f64,
        quantity: f64,
        side: &str,
        engine: Option<Py<PyAny>>,
    ) -> PyResult<PyOrder> {
        let _ = InstrumentId::from_str(instrument_id).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid instrument_id '{}': {}",
                instrument_id, e
            ))
        })?;
        let direction = parse_side(side)?;
        Ok(PyOrder {
            instrument_id: instrument_id.to_string(),
            quantity,
            side: direction,
            order_type: OrderType::StopLimit,
            price: None,
            trigger_price: Some(trigger_price),
            limit_price: Some(limit_price),
            offset: Offset::None,
            time_in_force: None,
            client_order_id: None,
            reference: String::new(),
            engine,
        })
    }
}

#[pymethods]
impl OrderFactory {
    #[new]
    #[pyo3(signature = (engine=None, gateway_name=""))]
    fn new(engine: Option<Py<PyAny>>, gateway_name: Option<&str>) -> Self {
        OrderFactory {
            engine,
            gateway_name: gateway_name.unwrap_or("").to_string(),
        }
    }

    /// Create a market order.
    ///
    /// Args:
    ///     instrument_id: Symbol in SYMBOL.EXCHANGE format (e.g., "BTCUSDT.BINANCE")
    ///     quantity: Order volume
    ///     side: "BUY"/"SELL" or "LONG"/"SHORT" (case-insensitive)
    ///
    /// Returns:
    ///     PyOrder with order_type="MARKET"
    fn market(
        &self,
        py: Python,
        instrument_id: &str,
        quantity: f64,
        side: &str,
    ) -> PyResult<PyOrder> {
        let engine = self.engine.as_ref().map(|e| e.clone_ref(py));
        self.build_market_order(instrument_id, quantity, side, engine)
    }

    /// Create a limit order.
    ///
    /// Args:
    ///     instrument_id: Symbol in SYMBOL.EXCHANGE format
    ///     price: Limit price
    ///     quantity: Order volume
    ///     side: "BUY"/"SELL" or "LONG"/"SHORT" (case-insensitive)
    ///
    /// Returns:
    ///     PyOrder with order_type="LIMIT"
    fn limit(
        &self,
        py: Python,
        instrument_id: &str,
        price: f64,
        quantity: f64,
        side: &str,
    ) -> PyResult<PyOrder> {
        let engine = self.engine.as_ref().map(|e| e.clone_ref(py));
        self.build_limit_order(instrument_id, price, quantity, side, engine)
    }

    /// Create a stop order (market triggered at trigger_price).
    ///
    /// Args:
    ///     instrument_id: Symbol in SYMBOL.EXCHANGE format
    ///     trigger_price: Price at which the stop order is triggered
    ///     quantity: Order volume
    ///     side: "BUY"/"SELL" or "LONG"/"SHORT" (case-insensitive)
    ///
    /// Returns:
    ///     PyOrder with order_type="STOP"
    fn stop(
        &self,
        py: Python,
        instrument_id: &str,
        trigger_price: f64,
        quantity: f64,
        side: &str,
    ) -> PyResult<PyOrder> {
        let engine = self.engine.as_ref().map(|e| e.clone_ref(py));
        self.build_stop_order(instrument_id, trigger_price, quantity, side, engine)
    }

    /// Create a stop-limit order.
    ///
    /// Args:
    ///     instrument_id: Symbol in SYMBOL.EXCHANGE format
    ///     trigger_price: Price at which the stop is triggered
    ///     limit_price: Limit price after trigger
    ///     quantity: Order volume
    ///     side: "BUY"/"SELL" or "LONG"/"SHORT" (case-insensitive)
    ///
    /// Returns:
    ///     PyOrder with order_type="STOP_LIMIT"
    fn stop_limit(
        &self,
        py: Python,
        instrument_id: &str,
        trigger_price: f64,
        limit_price: f64,
        quantity: f64,
        side: &str,
    ) -> PyResult<PyOrder> {
        let engine = self.engine.as_ref().map(|e| e.clone_ref(py));
        self.build_stop_limit_order(
            instrument_id,
            trigger_price,
            limit_price,
            quantity,
            side,
            engine,
        )
    }

    /// Get the default gateway name
    #[getter]
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_side_buy() {
        assert_eq!(parse_side("BUY").unwrap(), Direction::Long);
        assert_eq!(parse_side("buy").unwrap(), Direction::Long);
        assert_eq!(parse_side("Buy").unwrap(), Direction::Long);
    }

    #[test]
    fn test_parse_side_long() {
        assert_eq!(parse_side("LONG").unwrap(), Direction::Long);
        assert_eq!(parse_side("long").unwrap(), Direction::Long);
    }

    #[test]
    fn test_parse_side_sell() {
        assert_eq!(parse_side("SELL").unwrap(), Direction::Short);
        assert_eq!(parse_side("sell").unwrap(), Direction::Short);
        assert_eq!(parse_side("Sell").unwrap(), Direction::Short);
    }

    #[test]
    fn test_parse_side_short() {
        assert_eq!(parse_side("SHORT").unwrap(), Direction::Short);
        assert_eq!(parse_side("short").unwrap(), Direction::Short);
    }

    #[test]
    fn test_parse_side_invalid() {
        assert!(parse_side("HOLD").is_err());
        assert!(parse_side("").is_err());
        assert!(parse_side("buys").is_err());
    }

    #[test]
    fn test_direction_to_side() {
        assert_eq!(direction_to_side(Direction::Long), "BUY");
        assert_eq!(direction_to_side(Direction::Short), "SELL");
    }

    #[test]
    fn test_order_type_to_str() {
        assert_eq!(order_type_to_str(OrderType::Market), "MARKET");
        assert_eq!(order_type_to_str(OrderType::Limit), "LIMIT");
        assert_eq!(order_type_to_str(OrderType::Stop), "STOP");
    }

    #[test]
    fn test_market_order_creation() {
        let factory = OrderFactory::empty();
        let order = factory
            .build_market_order("BTCUSDT.BINANCE", 0.1, "BUY", None)
            .unwrap();

        assert_eq!(order.instrument_id(), "BTCUSDT.BINANCE");
        assert_eq!(order.quantity(), 0.1);
        assert_eq!(order.side(), "BUY");
        assert_eq!(order.order_type(), "MARKET");
        assert!(order.price().is_none());
        assert!(order.trigger_price().is_none());
        assert!(order.limit_price().is_none());
        assert_eq!(order.offset(), "NONE");
    }

    #[test]
    fn test_limit_order_creation() {
        let factory = OrderFactory::empty();
        let order = factory
            .build_limit_order("BTCUSDT.BINANCE", 50000.0, 0.1, "SELL", None)
            .unwrap();

        assert_eq!(order.instrument_id(), "BTCUSDT.BINANCE");
        assert_eq!(order.quantity(), 0.1);
        assert_eq!(order.side(), "SELL");
        assert_eq!(order.order_type(), "LIMIT");
        assert_eq!(order.price(), Some(50000.0));
        assert!(order.trigger_price().is_none());
    }

    #[test]
    fn test_stop_order_creation() {
        let factory = OrderFactory::empty();
        let order = factory
            .build_stop_order("BTCUSDT.BINANCE", 48000.0, 0.1, "SELL", None)
            .unwrap();

        assert_eq!(order.side(), "SELL");
        assert_eq!(order.order_type(), "STOP");
        assert_eq!(order.trigger_price(), Some(48000.0));
        assert!(order.limit_price().is_none());
    }

    #[test]
    fn test_stop_limit_order_creation() {
        let factory = OrderFactory::empty();
        let order = factory
            .build_stop_limit_order("BTCUSDT.BINANCE", 48000.0, 47900.0, 0.1, "SELL", None)
            .unwrap();

        assert_eq!(order.order_type(), "STOP_LIMIT");
        assert_eq!(order.trigger_price(), Some(48000.0));
        assert_eq!(order.limit_price(), Some(47900.0));
    }

    #[test]
    fn test_invalid_instrument_id() {
        let factory = OrderFactory::empty();
        // Missing exchange part
        assert!(factory
            .build_market_order("BTCUSDT", 0.1, "BUY", None)
            .is_err());
        // Unknown exchange
        assert!(factory
            .build_market_order("BTCUSDT.UNKNOWN", 0.1, "BUY", None)
            .is_err());
    }

    #[test]
    fn test_side_variants() {
        let factory = OrderFactory::empty();

        // "LONG" should work same as "BUY"
        let order_long = factory
            .build_market_order("BTCUSDT.BINANCE", 1.0, "LONG", None)
            .unwrap();
        assert_eq!(order_long.side(), "BUY");

        // "SHORT" should work same as "SELL"
        let order_short = factory
            .build_market_order("BTCUSDT.BINANCE", 1.0, "SHORT", None)
            .unwrap();
        assert_eq!(order_short.side(), "SELL");
    }

    #[test]
    fn test_futures_instrument() {
        let factory = OrderFactory::empty();
        let order = factory
            .build_limit_order("rb2401.SHFE", 3800.0, 2.0, "BUY", None)
            .unwrap();

        assert_eq!(order.instrument_id(), "rb2401.SHFE");
    }

    #[test]
    fn test_binance_usdm_instrument() {
        let factory = OrderFactory::empty();
        let order = factory
            .build_limit_order("BTCUSDT.BINANCE_USDM", 50000.0, 0.5, "SELL", None)
            .unwrap();

        assert_eq!(order.instrument_id(), "BTCUSDT.BINANCE_USDM");
    }

    #[test]
    fn test_py_order_direct_construction() {
        // Test PyOrder constructed directly (simulating builder pattern results)
        let order = PyOrder {
            instrument_id: "ETHUSDT.BINANCE".to_string(),
            quantity: 1.5,
            side: Direction::Long,
            order_type: OrderType::Limit,
            price: Some(3000.0),
            trigger_price: None,
            limit_price: None,
            offset: Offset::Open,
            time_in_force: Some("GTC".to_string()),
            client_order_id: Some("test_42".to_string()),
            reference: "my_strategy".to_string(),
            engine: None,
        };

        assert_eq!(order.instrument_id(), "ETHUSDT.BINANCE");
        assert_eq!(order.quantity(), 1.5);
        assert_eq!(order.side(), "BUY");
        assert_eq!(order.order_type(), "LIMIT");
        assert_eq!(order.price(), Some(3000.0));
        assert_eq!(order.offset(), "OPEN");
        assert_eq!(order.time_in_force(), Some("GTC"));
        assert_eq!(order.client_order_id(), Some("test_42"));
        assert_eq!(order.reference(), "my_strategy");
    }

    #[test]
    fn test_py_order_repr() {
        let market_order = PyOrder {
            instrument_id: "BTCUSDT.BINANCE".to_string(),
            quantity: 0.1,
            side: Direction::Long,
            order_type: OrderType::Market,
            price: None,
            trigger_price: None,
            limit_price: None,
            offset: Offset::None,
            time_in_force: None,
            client_order_id: None,
            reference: String::new(),
            engine: None,
        };
        assert_eq!(
            market_order.__repr__(),
            "PyOrder(BUY BTCUSDT.BINANCE @ MARKET x0.1)"
        );

        let limit_order = PyOrder {
            instrument_id: "BTCUSDT.BINANCE".to_string(),
            quantity: 0.5,
            side: Direction::Short,
            order_type: OrderType::Limit,
            price: Some(50000.0),
            trigger_price: None,
            limit_price: None,
            offset: Offset::None,
            time_in_force: None,
            client_order_id: None,
            reference: String::new(),
            engine: None,
        };
        assert!(limit_order.__repr__().contains("50000"));
        assert!(limit_order.__repr__().contains("LIMIT"));
    }
}
