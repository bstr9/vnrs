//! Python bindings for StopOrderEngine
//!
//! Exposes stop order management to Python strategies for:
//! - Querying active/all stop orders
//! - Cancelling stop orders by ID or symbol

use pyo3::prelude::*;

use crate::trader::stop_order::{StopOrder, StopOrderEngine, StopOrderStatus, StopOrderType};
use crate::trader::utility::extract_vt_symbol;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// PyStopOrder
// ---------------------------------------------------------------------------

/// Python wrapper for StopOrder data.
///
/// Represents a stop order (stop-loss, take-profit, trailing stop) that is
/// being tracked by the StopOrderEngine.
#[pyclass(name = "StopOrder")]
#[derive(Clone)]
pub struct PyStopOrder {
    inner: StopOrder,
}

impl PyStopOrder {
    /// Create a new PyStopOrder from a Rust StopOrder
    pub fn from_rust(order: StopOrder) -> Self {
        Self { inner: order }
    }
}

#[pymethods]
impl PyStopOrder {
    /// Unique stop order ID
    #[getter]
    fn id(&self) -> u64 {
        self.inner.id
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

    /// Stop order type: "StopMarket", "StopLimit", "TrailingStopPct", "TrailingStopAbs", "TakeProfit"
    #[getter]
    fn stop_type(&self) -> String {
        format_stop_order_type(self.inner.stop_type)
    }

    /// Stop price (trigger price)
    #[getter]
    fn stop_price(&self) -> f64 {
        self.inner.stop_price
    }

    /// Limit price (for StopLimit orders)
    #[getter]
    fn limit_price(&self) -> f64 {
        self.inner.limit_price
    }

    /// Order volume
    #[getter]
    fn volume(&self) -> f64 {
        self.inner.volume
    }

    /// Offset: "NONE", "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY"
    #[getter]
    fn offset(&self) -> String {
        format_offset(self.inner.offset)
    }

    /// Status: "Pending", "Triggered", "Cancelled", "Expired"
    #[getter]
    fn status(&self) -> String {
        format_stop_order_status(self.inner.status)
    }

    /// Trailing percentage (for TrailingStopPct)
    #[getter]
    fn trail_pct(&self) -> f64 {
        self.inner.trail_pct
    }

    /// Trailing absolute distance (for TrailingStopAbs)
    #[getter]
    fn trail_abs(&self) -> f64 {
        self.inner.trail_abs
    }

    /// Highest price seen (for trailing stops)
    #[getter]
    fn highest_price(&self) -> f64 {
        self.inner.highest_price
    }

    /// Lowest price seen (for trailing stops)
    #[getter]
    fn lowest_price(&self) -> f64 {
        self.inner.lowest_price
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

    /// Creation time (ISO 8601)
    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    /// Trigger time (ISO 8601, if triggered)
    #[getter]
    fn triggered_at(&self) -> Option<String> {
        self.inner.triggered_at.map(|t| t.to_rfc3339())
    }

    /// Expiration time (ISO 8601, if set)
    #[getter]
    fn expires_at(&self) -> Option<String> {
        self.inner.expires_at.map(|t| t.to_rfc3339())
    }

    /// Tag string
    #[getter]
    fn tag(&self) -> &str {
        &self.inner.tag
    }

    /// Check if the order is still active (Pending status)
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!(
            "StopOrder(id={}, vt_symbol='{}', direction='{}', stop_type='{}', status='{}')",
            self.inner.id,
            self.inner.vt_symbol(),
            format_direction(self.inner.direction),
            format_stop_order_type(self.inner.stop_type),
            format_stop_order_status(self.inner.status)
        )
    }
}

// ---------------------------------------------------------------------------
// PyStopOrderEngine
// ---------------------------------------------------------------------------

/// Python wrapper for StopOrderEngine.
///
/// Provides read-only access to stop order state and cancellation methods.
/// Stop orders are created through strategy methods or MainEngine, not here.
///
/// Usage::
///
///     engine = create_main_engine()
///     stop_engine = engine.get_stop_order_engine()
///
///     # Get all active stop orders
///     active_orders = stop_engine.get_active_stop_orders()
///
///     # Cancel a specific stop order
///     stop_engine.cancel_stop_order(stop_orderid)
///
///     # Cancel all stop orders for a symbol
///     stop_engine.cancel_orders_for_symbol("BTCUSDT.BINANCE")
#[pyclass(name = "StopOrderEngine")]
pub struct PyStopOrderEngine {
    inner: Arc<StopOrderEngine>,
}

impl PyStopOrderEngine {
    /// Create a new PyStopOrderEngine from an Arc<StopOrderEngine>
    pub fn new(engine: Arc<StopOrderEngine>) -> Self {
        Self { inner: engine }
    }
}

#[pymethods]
impl PyStopOrderEngine {
    /// Get all active (Pending) stop orders.
    ///
    /// Returns:
    ///     List of StopOrder objects with Pending status
    fn get_active_stop_orders(&self) -> Vec<PyStopOrder> {
        self.inner
            .get_active_stop_orders()
            .into_iter()
            .map(PyStopOrder::from_rust)
            .collect()
    }

    /// Get all stop orders (including triggered, cancelled, expired).
    ///
    /// Returns:
    ///     List of all StopOrder objects
    fn get_all_stop_orders(&self) -> Vec<PyStopOrder> {
        self.inner
            .get_all_stop_orders()
            .into_iter()
            .map(PyStopOrder::from_rust)
            .collect()
    }

    /// Get a specific stop order by ID.
    ///
    /// Args:
    ///     stop_orderid: The stop order ID
    ///
    /// Returns:
    ///     StopOrder if found, None otherwise
    fn get_stop_order(&self, stop_orderid: u64) -> Option<PyStopOrder> {
        self.inner
            .get_stop_order(stop_orderid)
            .map(PyStopOrder::from_rust)
    }

    /// Cancel a stop order by ID.
    ///
    /// Args:
    ///     stop_orderid: The stop order ID to cancel
    ///
    /// Raises:
    ///     ValueError: If the order is not found or not in Pending status
    fn cancel_stop_order(&self, stop_orderid: u64) -> PyResult<()> {
        self.inner
            .cancel_stop_order(stop_orderid)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Cancel all stop orders for a specific symbol.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "BTCUSDT.BINANCE")
    ///
    /// Returns:
    ///     Number of orders cancelled
    fn cancel_orders_for_symbol(&self, vt_symbol: &str) -> PyResult<usize> {
        let (symbol, exchange) = extract_vt_symbol(vt_symbol)
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "Invalid vt_symbol format: {}",
                    vt_symbol
                ))
            })?;
        Ok(self.inner.cancel_orders_for_symbol(&symbol, exchange))
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn format_direction(d: crate::trader::Direction) -> String {
    match d {
        crate::trader::Direction::Long => "LONG".to_string(),
        crate::trader::Direction::Short => "SHORT".to_string(),
        crate::trader::Direction::Net => "NET".to_string(),
    }
}

fn format_offset(o: crate::trader::Offset) -> String {
    match o {
        crate::trader::Offset::None => "NONE".to_string(),
        crate::trader::Offset::Open => "OPEN".to_string(),
        crate::trader::Offset::Close => "CLOSE".to_string(),
        crate::trader::Offset::CloseToday => "CLOSETODAY".to_string(),
        crate::trader::Offset::CloseYesterday => "CLOSEYESTERDAY".to_string(),
    }
}

fn format_stop_order_type(t: StopOrderType) -> String {
    match t {
        StopOrderType::StopMarket => "StopMarket".to_string(),
        StopOrderType::StopLimit => "StopLimit".to_string(),
        StopOrderType::TrailingStopPct => "TrailingStopPct".to_string(),
        StopOrderType::TrailingStopAbs => "TrailingStopAbs".to_string(),
        StopOrderType::TakeProfit => "TakeProfit".to_string(),
    }
}

fn format_stop_order_status(s: StopOrderStatus) -> String {
    match s {
        StopOrderStatus::Pending => "Pending".to_string(),
        StopOrderStatus::Triggered => "Triggered".to_string(),
        StopOrderStatus::Cancelled => "Cancelled".to_string(),
        StopOrderStatus::Expired => "Expired".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register stop order engine classes with the parent module
pub fn register_stop_order_engine_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyStopOrder>()?;
    m.add_class::<PyStopOrderEngine>()?;
    Ok(())
}
