//! Python bindings for OrderEmulator
//!
//! Exposes the Order Emulator engine to Python strategies for:
//! - Adding emulated orders (trailing stop, stop-limit, iceberg, MIT, LIT)
//! - Cancelling emulated orders
//! - Querying active emulated orders

use pyo3::prelude::*;

use crate::trader::constant::{Direction, Exchange, Offset};
use crate::trader::order_emulator::{
    EmulatedOrder, EmulatedOrderRequest, EmulatedOrderStatus, EmulatedOrderType, OrderEmulator,
};
use crate::trader::utility::extract_vt_symbol;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// PyEmulatedOrder
// ---------------------------------------------------------------------------

/// Python wrapper for EmulatedOrder data.
///
/// Represents an emulated order (trailing stop, stop-limit, iceberg, MIT, LIT)
/// that is being tracked by the OrderEmulator engine.
#[pyclass(name = "EmulatedOrder")]
#[derive(Clone)]
pub struct PyEmulatedOrder {
    inner: EmulatedOrder,
}

impl PyEmulatedOrder {
    /// Create a new PyEmulatedOrder from a Rust EmulatedOrder
    pub fn from_rust(order: EmulatedOrder) -> Self {
        Self { inner: order }
    }
}

#[pymethods]
impl PyEmulatedOrder {
    /// Unique emulated order ID
    #[getter]
    fn id(&self) -> u64 {
        self.inner.id
    }

    /// Order type: "TrailingStopPct", "TrailingStopAbs", "StopLimit", "Iceberg", "MIT", "LIT"
    #[getter]
    fn order_type(&self) -> String {
        format_emulated_order_type(self.inner.order_type)
    }

    /// Status: "Pending", "Triggered", "Completed", "Cancelled", "Expired", "Rejected"
    #[getter]
    fn status(&self) -> String {
        format_emulated_order_status(self.inner.status)
    }

    /// Symbol (e.g., "BTCUSDT")
    #[getter]
    fn symbol(&self) -> &str {
        &self.inner.symbol
    }

    /// Exchange name (e.g., "BINANCE")
    #[getter]
    fn exchange(&self) -> String {
        self.inner.exchange.value().to_string()
    }

    /// Full vt_symbol (e.g., "BTCUSDT.BINANCE")
    #[getter]
    fn vt_symbol(&self) -> String {
        self.inner.vt_symbol()
    }

    /// Direction: "LONG" or "SHORT"
    #[getter]
    fn direction(&self) -> String {
        format_direction(self.inner.direction)
    }

    /// Offset: "NONE", "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY"
    #[getter]
    fn offset(&self) -> String {
        format_offset(self.inner.offset)
    }

    /// Total order volume
    #[getter]
    fn volume(&self) -> f64 {
        self.inner.volume
    }

    /// Remaining volume (for iceberg, hidden quantity)
    #[getter]
    fn remaining_volume(&self) -> f64 {
        self.inner.remaining_volume
    }

    /// Trailing percentage (for TrailingStopPct)
    #[getter]
    fn trail_pct(&self) -> Option<f64> {
        self.inner.trail_pct
    }

    /// Trailing absolute distance (for TrailingStopAbs)
    #[getter]
    fn trail_abs(&self) -> Option<f64> {
        self.inner.trail_abs
    }

    /// Current computed stop price (for trailing stops)
    #[getter]
    fn current_stop(&self) -> Option<f64> {
        self.inner.current_stop
    }

    /// Highest price seen (for trailing stops)
    #[getter]
    fn highest_price(&self) -> Option<f64> {
        self.inner.highest_price
    }

    /// Lowest price seen (for trailing stops)
    #[getter]
    fn lowest_price(&self) -> Option<f64> {
        self.inner.lowest_price
    }

    /// Trigger price (for StopLimit, MIT, LIT)
    #[getter]
    fn trigger_price(&self) -> Option<f64> {
        self.inner.trigger_price
    }

    /// Limit price (for StopLimit, LIT)
    #[getter]
    fn limit_price(&self) -> Option<f64> {
        self.inner.limit_price
    }

    /// Visible volume per slice (for Iceberg)
    #[getter]
    fn visible_volume(&self) -> Option<f64> {
        self.inner.visible_volume
    }

    /// Price for iceberg slices (for Iceberg)
    #[getter]
    fn iceberg_price(&self) -> Option<f64> {
        self.inner.iceberg_price
    }

    /// Real order ID from exchange (after trigger)
    #[getter]
    fn real_order_id(&self) -> Option<&str> {
        self.inner.real_order_id.as_deref()
    }

    /// Creation time (ISO 8601)
    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    /// Expiration time (ISO 8601, if set)
    #[getter]
    fn expires_at(&self) -> Option<String> {
        self.inner.expires_at.map(|t| t.to_rfc3339())
    }

    /// Gateway name
    #[getter]
    fn gateway_name(&self) -> &str {
        &self.inner.gateway_name
    }

    /// Reference string
    #[getter]
    fn reference(&self) -> &str {
        &self.inner.reference
    }

    /// Check if the order is still active
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!(
            "EmulatedOrder(id={}, vt_symbol='{}', order_type='{}', direction='{}', status='{}')",
            self.inner.id,
            self.inner.vt_symbol(),
            format_emulated_order_type(self.inner.order_type),
            format_direction(self.inner.direction),
            format_emulated_order_status(self.inner.status)
        )
    }
}

// ---------------------------------------------------------------------------
// PyOrderEmulator
// ---------------------------------------------------------------------------

/// Python wrapper for OrderEmulator.
///
/// Provides methods to add, cancel, and query emulated orders.
/// Emulated orders are locally simulated order types not natively supported
/// by exchanges (trailing stops, stop-limit, iceberg, MIT, LIT).
///
/// Usage::
///
///     engine = create_main_engine()
///     emulator = engine.get_order_emulator()
///
///     # Add a trailing stop with 5% distance
///     order_id = emulator.add_trailing_stop_pct(
///         vt_symbol="BTCUSDT.BINANCE",
///         direction="LONG",
///         volume=1.0,
///         rate=5.0,
///         gateway_name="BINANCE_SPOT"
///     )
///
///     # Add an iceberg order
///     order_id = emulator.add_iceberg(
///         vt_symbol="BTCUSDT.BINANCE",
///         direction="LONG",
///         volume=10.0,
///         display_volume=1.0,
///         price=50000.0,
///         gateway_name="BINANCE_SPOT"
///     )
///
///     # Get all active emulated orders
///     active = emulator.get_active_orders()
///
///     # Cancel an order
///     emulator.cancel_order(order_id)
#[pyclass(name = "OrderEmulator")]
pub struct PyOrderEmulator {
    inner: Arc<OrderEmulator>,
}

impl PyOrderEmulator {
    /// Create a new PyOrderEmulator from an Arc<OrderEmulator>
    pub fn new(engine: Arc<OrderEmulator>) -> Self {
        Self { inner: engine }
    }
}

#[pymethods]
impl PyOrderEmulator {
    /// Add a trailing stop order with percentage distance.
    ///
    /// The stop price follows the market at a percentage distance.
    /// For LONG: stop moves up as price rises, triggers when price drops.
    /// For SHORT: stop moves down as price falls, triggers when price rises.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "BTCUSDT.BINANCE")
    ///     direction: "LONG" or "SHORT"
    ///     volume: Order quantity
    ///     rate: Trailing percentage distance (e.g., 5.0 for 5%)
    ///     gateway_name: Gateway to use for order submission
    ///     offset: Order offset ("NONE", "OPEN", "CLOSE", etc.), default "NONE"
    ///
    /// Returns:
    ///     The emulated order ID as a string
    #[pyo3(signature = (vt_symbol, direction, volume, rate, gateway_name, offset="NONE"))]
    fn add_trailing_stop_pct(
        &self,
        vt_symbol: &str,
        direction: &str,
        volume: f64,
        rate: f64,
        gateway_name: &str,
        offset: &str,
    ) -> PyResult<String> {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol)?;
        let direction = parse_direction(direction)?;
        let offset = parse_offset(offset);

        let req = EmulatedOrderRequest::trailing_stop_pct(
            &symbol, exchange, direction, offset, volume, rate, gateway_name,
        );
        self.inner
            .add_order(&req)
            .map(|id| id.to_string())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add a trailing stop order with absolute price distance.
    ///
    /// The stop price follows the market at a fixed price distance.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG" or "SHORT"
    ///     volume: Order quantity
    ///     trail_amount: Trailing absolute price distance
    ///     gateway_name: Gateway to use for order submission
    ///     offset: Order offset, default "NONE"
    ///
    /// Returns:
    ///     The emulated order ID as a string
    #[pyo3(signature = (vt_symbol, direction, volume, trail_amount, gateway_name, offset="NONE"))]
    fn add_trailing_stop_abs(
        &self,
        vt_symbol: &str,
        direction: &str,
        volume: f64,
        trail_amount: f64,
        gateway_name: &str,
        offset: &str,
    ) -> PyResult<String> {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol)?;
        let direction = parse_direction(direction)?;
        let offset = parse_offset(offset);

        let req = EmulatedOrderRequest::trailing_stop_abs(
            &symbol, exchange, direction, offset, volume, trail_amount, gateway_name,
        );
        self.inner
            .add_order(&req)
            .map(|id| id.to_string())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add a stop-limit order.
    ///
    /// When the trigger price is hit, a limit order is submitted at the
    /// specified limit price.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG" or "SHORT"
    ///     volume: Order quantity
    ///     stop_price: Trigger price
    ///     limit_price: Limit price for the submitted order
    ///     gateway_name: Gateway to use for order submission
    ///     offset: Order offset, default "NONE"
    ///
    /// Returns:
    ///     The emulated order ID as a string
    #[pyo3(signature = (vt_symbol, direction, volume, stop_price, limit_price, gateway_name, offset="NONE"))]
    #[allow(clippy::too_many_arguments)]
    fn add_stop_limit(
        &self,
        vt_symbol: &str,
        direction: &str,
        volume: f64,
        stop_price: f64,
        limit_price: f64,
        gateway_name: &str,
        offset: &str,
    ) -> PyResult<String> {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol)?;
        let direction = parse_direction(direction)?;
        let offset = parse_offset(offset);

        let req = EmulatedOrderRequest::stop_limit(
            &symbol,
            exchange,
            direction,
            offset,
            volume,
            stop_price,
            limit_price,
            gateway_name,
        );
        self.inner
            .add_order(&req)
            .map(|id| id.to_string())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add an iceberg order.
    ///
    /// Only displays a portion of the total volume at a time. As each
    /// visible slice is filled, the next slice is submitted.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG" or "SHORT"
    ///     volume: Total order quantity (including hidden)
    ///     display_volume: Visible quantity per slice
    ///     price: Limit price for iceberg slices
    ///     gateway_name: Gateway to use for order submission
    ///     offset: Order offset, default "NONE"
    ///
    /// Returns:
    ///     The emulated order ID as a string
    #[pyo3(signature = (vt_symbol, direction, volume, display_volume, price, gateway_name, offset="NONE"))]
    #[allow(clippy::too_many_arguments)]
    fn add_iceberg(
        &self,
        vt_symbol: &str,
        direction: &str,
        volume: f64,
        display_volume: f64,
        price: f64,
        gateway_name: &str,
        offset: &str,
    ) -> PyResult<String> {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol)?;
        let direction = parse_direction(direction)?;
        let offset = parse_offset(offset);

        let req = EmulatedOrderRequest::iceberg(
            &symbol,
            exchange,
            direction,
            offset,
            volume,
            display_volume,
            price,
            gateway_name,
        );
        self.inner
            .add_order(&req)
            .map(|id| id.to_string())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add a Market-If-Touched (MIT) order.
    ///
    /// When the market price touches the specified price, a market order
    /// is submitted.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG" or "SHORT"
    ///     volume: Order quantity
    ///     touch_price: Trigger price
    ///     gateway_name: Gateway to use for order submission
    ///     offset: Order offset, default "NONE"
    ///
    /// Returns:
    ///     The emulated order ID as a string
    #[pyo3(signature = (vt_symbol, direction, volume, touch_price, gateway_name, offset="NONE"))]
    fn add_market_if_touched(
        &self,
        vt_symbol: &str,
        direction: &str,
        volume: f64,
        touch_price: f64,
        gateway_name: &str,
        offset: &str,
    ) -> PyResult<String> {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol)?;
        let direction = parse_direction(direction)?;
        let offset = parse_offset(offset);

        let req = EmulatedOrderRequest::market_if_touched(
            &symbol, exchange, direction, offset, volume, touch_price, gateway_name,
        );
        self.inner
            .add_order(&req)
            .map(|id| id.to_string())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add a Limit-If-Touched (LIT) order.
    ///
    /// When the market price touches the specified price, a limit order
    /// is submitted at the limit price.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///     direction: "LONG" or "SHORT"
    ///     volume: Order quantity
    ///     touch_price: Trigger price
    ///     limit_price: Limit price for the submitted order
    ///     gateway_name: Gateway to use for order submission
    ///     offset: Order offset, default "NONE"
    ///
    /// Returns:
    ///     The emulated order ID as a string
    #[pyo3(signature = (vt_symbol, direction, volume, touch_price, limit_price, gateway_name, offset="NONE"))]
    #[allow(clippy::too_many_arguments)]
    fn add_limit_if_touched(
        &self,
        vt_symbol: &str,
        direction: &str,
        volume: f64,
        touch_price: f64,
        limit_price: f64,
        gateway_name: &str,
        offset: &str,
    ) -> PyResult<String> {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol)?;
        let direction = parse_direction(direction)?;
        let offset = parse_offset(offset);

        let req = EmulatedOrderRequest::limit_if_touched(
            &symbol,
            exchange,
            direction,
            offset,
            volume,
            touch_price,
            limit_price,
            gateway_name,
        );
        self.inner
            .add_order(&req)
            .map(|id| id.to_string())
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Cancel an emulated order by ID.
    ///
    /// Args:
    ///     emulated_orderid: The emulated order ID (as string or int)
    ///
    /// Raises:
    ///     ValueError: If the order is not found or not in active status
    fn cancel_order(&self, emulated_orderid: &str) -> PyResult<()> {
        let id: u64 = emulated_orderid
            .parse()
            .map_err(|_| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid emulated order ID: {}",
                    emulated_orderid
                ))
            })?;
        self.inner
            .cancel_order(id)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Cancel all emulated orders for a specific symbol.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    fn cancel_orders_for_symbol(&self, vt_symbol: &str) -> PyResult<()> {
        self.inner.cancel_orders_for_symbol(vt_symbol);
        Ok(())
    }

    /// Get all active emulated orders.
    ///
    /// Returns:
    ///     List of EmulatedOrder objects with Pending or Triggered status
    fn get_active_orders(&self) -> Vec<PyEmulatedOrder> {
        self.inner
            .get_active_orders()
            .into_iter()
            .map(PyEmulatedOrder::from_rust)
            .collect()
    }

    /// Get all emulated orders (including completed, cancelled, etc.).
    ///
    /// Returns:
    ///     List of all EmulatedOrder objects
    fn get_all_orders(&self) -> Vec<PyEmulatedOrder> {
        self.inner
            .get_all_orders()
            .into_iter()
            .map(PyEmulatedOrder::from_rust)
            .collect()
    }

    /// Get a specific emulated order by ID.
    ///
    /// Args:
    ///     emulated_orderid: The emulated order ID
    ///
    /// Returns:
    ///     EmulatedOrder if found, None otherwise
    fn get_order(&self, emulated_orderid: u64) -> Option<PyEmulatedOrder> {
        self.inner
            .get_order(emulated_orderid)
            .map(PyEmulatedOrder::from_rust)
    }

    /// Get emulated orders for a specific symbol.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format
    ///
    /// Returns:
    ///     List of EmulatedOrder objects for this symbol
    fn get_orders_for_symbol(&self, vt_symbol: &str) -> Vec<PyEmulatedOrder> {
        self.inner
            .get_orders_for_symbol(vt_symbol)
            .into_iter()
            .map(PyEmulatedOrder::from_rust)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Parsing / formatting helpers
// ---------------------------------------------------------------------------

fn parse_vt_symbol(vt_symbol: &str) -> PyResult<(String, Exchange)> {
    extract_vt_symbol(vt_symbol).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid vt_symbol format: '{}'. Expected SYMBOL.EXCHANGE (e.g., BTCUSDT.BINANCE)",
            vt_symbol
        ))
    })
}

fn parse_direction(s: &str) -> PyResult<Direction> {
    match s.to_uppercase().as_str() {
        "LONG" | "BUY" => Ok(Direction::Long),
        "SHORT" | "SELL" => Ok(Direction::Short),
        "NET" => Ok(Direction::Net),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid direction '{}'. Must be LONG or SHORT",
            s
        ))),
    }
}

fn parse_offset(s: &str) -> Offset {
    match s.to_uppercase().as_str() {
        "OPEN" => Offset::Open,
        "CLOSE" => Offset::Close,
        "CLOSETODAY" | "CLOSE_TODAY" => Offset::CloseToday,
        "CLOSEYESTERDAY" | "CLOSE_YESTERDAY" => Offset::CloseYesterday,
        _ => Offset::None,
    }
}

fn format_direction(d: Direction) -> String {
    match d {
        Direction::Long => "LONG".to_string(),
        Direction::Short => "SHORT".to_string(),
        Direction::Net => "NET".to_string(),
    }
}

fn format_offset(o: Offset) -> String {
    match o {
        Offset::None => "NONE".to_string(),
        Offset::Open => "OPEN".to_string(),
        Offset::Close => "CLOSE".to_string(),
        Offset::CloseToday => "CLOSETODAY".to_string(),
        Offset::CloseYesterday => "CLOSEYESTERDAY".to_string(),
    }
}

fn format_emulated_order_type(t: EmulatedOrderType) -> String {
    match t {
        EmulatedOrderType::TrailingStopPct => "TrailingStopPct".to_string(),
        EmulatedOrderType::TrailingStopAbs => "TrailingStopAbs".to_string(),
        EmulatedOrderType::StopLimit => "StopLimit".to_string(),
        EmulatedOrderType::Iceberg => "Iceberg".to_string(),
        EmulatedOrderType::Mit => "MIT".to_string(),
        EmulatedOrderType::Lit => "LIT".to_string(),
        EmulatedOrderType::PeggedBest => "PeggedBest".to_string(),
    }
}

fn format_emulated_order_status(s: EmulatedOrderStatus) -> String {
    match s {
        EmulatedOrderStatus::Pending => "Pending".to_string(),
        EmulatedOrderStatus::Triggered => "Triggered".to_string(),
        EmulatedOrderStatus::Completed => "Completed".to_string(),
        EmulatedOrderStatus::Cancelled => "Cancelled".to_string(),
        EmulatedOrderStatus::Expired => "Expired".to_string(),
        EmulatedOrderStatus::Rejected => "Rejected".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register order emulator classes with the parent module
pub fn register_order_emulator_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEmulatedOrder>()?;
    m.add_class::<PyOrderEmulator>()?;
    Ok(())
}
