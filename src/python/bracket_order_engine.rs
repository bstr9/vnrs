//! Python bindings for BracketOrderEngine
//!
//! Exposes bracket/OCO/OTO contingent order management to Python strategies for:
//! - Creating bracket orders (entry + TP + SL)
//! - Creating OCO (one-cancels-other) order pairs
//! - Creating OTO (one-triggers-other) order pairs
//! - Cancelling order groups
//! - Querying active/all order groups

use pyo3::prelude::*;

use crate::trader::bracket_order::{
    BracketOrderEngine, BracketOrderRequest, ChildOrder, OcoOrderRequest,
    OrderGroup, OtoOrderRequest,
};
use crate::trader::constant::{Direction, Offset, OrderType};
use crate::trader::utility::extract_vt_symbol;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// PyBracketOrderGroup
// ---------------------------------------------------------------------------

/// Python wrapper for OrderGroup data.
///
/// Represents a group of contingent orders (bracket, OCO, or OTO) that is
/// being tracked by the BracketOrderEngine.
#[pyclass(name = "BracketOrderGroup")]
#[derive(Clone)]
pub struct PyBracketOrderGroup {
    inner: OrderGroup,
}

impl PyBracketOrderGroup {
    /// Create a new PyBracketOrderGroup from a Rust OrderGroup
    pub fn from_rust(group: OrderGroup) -> Self {
        Self { inner: group }
    }
}

#[pymethods]
impl PyBracketOrderGroup {
    /// Unique group ID
    #[getter]
    fn id(&self) -> u64 {
        self.inner.id
    }

    /// Contingency type: "OCO", "OTO", or "Bracket"
    #[getter]
    fn contingency_type(&self) -> String {
        self.inner.contingency_type.to_string()
    }

    /// Group state: "Pending", "EntryActive", "SecondaryActive", "Completed", "Cancelled", "Rejected"
    #[getter]
    fn state(&self) -> String {
        self.inner.state.to_string()
    }

    /// Full vt_symbol (e.g., "BTCUSDT.BINANCE")
    #[getter]
    fn vt_symbol(&self) -> &str {
        &self.inner.vt_symbol
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

    /// Tag string
    #[getter]
    fn tag(&self) -> &str {
        &self.inner.tag
    }

    /// Creation time (ISO 8601)
    #[getter]
    fn created_at(&self) -> String {
        self.inner.created_at.to_rfc3339()
    }

    /// Completion time (ISO 8601, if completed)
    #[getter]
    fn completed_at(&self) -> Option<String> {
        self.inner.completed_at.map(|t| t.to_rfc3339())
    }

    /// Whether this group is still active (may produce further fills)
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }

    /// Get the child orders in this group as a list of dicts.
    ///
    /// Each dict contains: role, vt_orderid, status, filled_volume, avg_fill_price,
    /// symbol, exchange, direction, order_type, price, volume, offset
    fn get_orders(&self) -> Vec<PyChildOrderInfo> {
        self.inner
            .orders
            .values()
            .map(|c| PyChildOrderInfo::from_rust(c.clone()))
            .collect()
    }

    /// String representation
    fn __repr__(&self) -> String {
        format!(
            "BracketOrderGroup(id={}, type='{}', vt_symbol='{}', state='{}')",
            self.inner.id,
            self.inner.contingency_type,
            self.inner.vt_symbol,
            self.inner.state
        )
    }
}

// ---------------------------------------------------------------------------
// PyChildOrderInfo
// ---------------------------------------------------------------------------

/// Python wrapper for child order info within a group.
#[pyclass(name = "ChildOrderInfo")]
#[derive(Clone)]
pub struct PyChildOrderInfo {
    role: String,
    vt_orderid: Option<String>,
    status: String,
    filled_volume: f64,
    avg_fill_price: f64,
    symbol: String,
    exchange: String,
    direction: String,
    order_type: String,
    price: f64,
    volume: f64,
    offset: String,
}

impl PyChildOrderInfo {
    fn from_rust(child: ChildOrder) -> Self {
        Self {
            role: child.role.to_string(),
            vt_orderid: child.vt_orderid,
            status: format_status(child.status),
            filled_volume: child.filled_volume,
            avg_fill_price: child.avg_fill_price,
            symbol: child.request.symbol.clone(),
            exchange: child.request.exchange.value().to_string(),
            direction: format_direction(child.request.direction),
            order_type: format_order_type(child.request.order_type),
            price: child.request.price,
            volume: child.request.volume,
            offset: format_offset(child.request.offset),
        }
    }
}

#[pymethods]
impl PyChildOrderInfo {
    /// Order role: "Entry", "TakeProfit", "StopLoss", "Primary", "Secondary", "OrderA", "OrderB"
    #[getter]
    fn role(&self) -> &str {
        &self.role
    }

    /// vt_orderid if submitted
    #[getter]
    fn vt_orderid(&self) -> Option<&str> {
        self.vt_orderid.as_deref()
    }

    /// Order status
    #[getter]
    fn status(&self) -> &str {
        &self.status
    }

    /// Filled volume
    #[getter]
    fn filled_volume(&self) -> f64 {
        self.filled_volume
    }

    /// Average fill price
    #[getter]
    fn avg_fill_price(&self) -> f64 {
        self.avg_fill_price
    }

    /// Symbol
    #[getter]
    fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Exchange
    #[getter]
    fn exchange(&self) -> &str {
        &self.exchange
    }

    /// Direction
    #[getter]
    fn direction(&self) -> &str {
        &self.direction
    }

    /// Order type
    #[getter]
    fn order_type(&self) -> &str {
        &self.order_type
    }

    /// Price
    #[getter]
    fn price(&self) -> f64 {
        self.price
    }

    /// Volume
    #[getter]
    fn volume(&self) -> f64 {
        self.volume
    }

    /// Offset
    #[getter]
    fn offset(&self) -> &str {
        &self.offset
    }

    fn __repr__(&self) -> String {
        format!(
            "ChildOrderInfo(role='{}', symbol='{}', status='{}')",
            self.role, self.symbol, self.status
        )
    }
}

// ---------------------------------------------------------------------------
// PyBracketOrderEngine
// ---------------------------------------------------------------------------

/// Python wrapper for BracketOrderEngine.
///
/// Provides bracket/OCO/OTO contingent order management.
///
/// Usage::
///
///     engine = create_main_engine()
///     boe = engine.get_bracket_order_engine()
///
///     # Add a bracket order (entry + TP + SL)
///     group_id = boe.add_bracket_order(
///         "BTCUSDT.BINANCE", "LONG",
///         entry_price=50000.0, entry_volume=0.1,
///         tp_price=55000.0, sl_price=48000.0,
///     )
///
///     # Add an OCO order pair
///     group_id = boe.add_oco_order(
///         "BTCUSDT.BINANCE", "LONG",
///         order_a_price=55000.0, order_b_price=48000.0,
///         volume=0.1,
///     )
///
///     # Add an OTO order pair
///     group_id = boe.add_oto_order(
///         "BTCUSDT.BINANCE",
///         primary_direction="LONG", primary_price=50000.0, primary_volume=0.1,
///         secondary_direction="SHORT", secondary_price=55000.0, secondary_volume=0.1,
///     )
///
///     # Get all active groups
///     active = boe.get_active_groups()
///
///     # Cancel a group
///     boe.cancel_group(group_id)
#[pyclass(name = "BracketOrderEngine")]
pub struct PyBracketOrderEngine {
    inner: Arc<BracketOrderEngine>,
}

impl PyBracketOrderEngine {
    /// Create a new PyBracketOrderEngine from an Arc<BracketOrderEngine>
    pub fn new(engine: Arc<BracketOrderEngine>) -> Self {
        Self { inner: engine }
    }
}

#[pymethods]
impl PyBracketOrderEngine {
    /// Add a bracket order (entry + take-profit + stop-loss).
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "BTCUSDT.BINANCE")
    ///     direction: "LONG", "SHORT", or "NET"
    ///     entry_price: Entry order price (0.0 for market entry)
    ///     entry_volume: Entry order volume (must be > 0)
    ///     tp_price: Take-profit price (must be > 0)
    ///     sl_price: Stop-loss price (must be > 0)
    ///     entry_type: "LIMIT" or "MARKET" (default "LIMIT")
    ///     sl_type: "STOP", "STOP_LIMIT", or "LIMIT" (default "STOP")
    ///     offset: Order offset - "NONE", "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY" (default "NONE")
    ///     gateway_name: Gateway name (default "MAIN")
    ///     reference: Optional reference string (default "")
    ///     tag: Optional tag string (default "")
    ///
    /// Returns:
    ///     The group ID on success
    ///
    /// Raises:
    ///     ValueError: If parameters are invalid
    #[pyo3(signature = (vt_symbol, direction, entry_price, entry_volume, tp_price, sl_price, entry_type="LIMIT", sl_type="STOP", offset="NONE", gateway_name="MAIN", reference="", tag=""))]
    fn add_bracket_order(
        &self,
        vt_symbol: &str,
        direction: &str,
        entry_price: f64,
        entry_volume: f64,
        tp_price: f64,
        sl_price: f64,
        entry_type: &str,
        sl_type: &str,
        offset: &str,
        gateway_name: &str,
        reference: &str,
        tag: &str,
    ) -> PyResult<u64> {
        let (symbol, exchange) = extract_vt_symbol(vt_symbol).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid vt_symbol format: {}",
                vt_symbol
            ))
        })?;

        let dir = parse_direction(direction)?;
        let et = parse_order_type(entry_type)?;
        let st = parse_order_type(sl_type)?;
        let off = parse_offset(offset)?;

        let req = BracketOrderRequest {
            symbol,
            exchange,
            direction: dir,
            entry_price,
            entry_volume,
            entry_type: et,
            tp_price,
            sl_price,
            sl_type: st,
            offset: off,
            gateway_name: gateway_name.to_string(),
            reference: reference.to_string(),
            tag: tag.to_string(),
        };

        self.inner
            .add_bracket_order(req)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add an OCO (one-cancels-other) order pair.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "BTCUSDT.BINANCE")
    ///     direction: "LONG", "SHORT", or "NET"
    ///     order_a_price: Price for order A
    ///     order_b_price: Price for order B
    ///     volume: Order volume (must be > 0)
    ///     order_a_type: Order type for A - "LIMIT", "MARKET", "STOP", "STOP_LIMIT" (default "LIMIT")
    ///     order_b_type: Order type for B - "LIMIT", "MARKET", "STOP", "STOP_LIMIT" (default "LIMIT")
    ///     offset: Order offset - "NONE", "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY" (default "NONE")
    ///     gateway_name: Gateway name (default "MAIN")
    ///     reference: Optional reference string (default "")
    ///     tag: Optional tag string (default "")
    ///
    /// Returns:
    ///     The group ID on success
    ///
    /// Raises:
    ///     ValueError: If parameters are invalid
    #[pyo3(signature = (vt_symbol, direction, order_a_price, order_b_price, volume, order_a_type="LIMIT", order_b_type="LIMIT", offset="NONE", gateway_name="MAIN", reference="", tag=""))]
    fn add_oco_order(
        &self,
        vt_symbol: &str,
        direction: &str,
        order_a_price: f64,
        order_b_price: f64,
        volume: f64,
        order_a_type: &str,
        order_b_type: &str,
        offset: &str,
        gateway_name: &str,
        reference: &str,
        tag: &str,
    ) -> PyResult<u64> {
        let (symbol, exchange) = extract_vt_symbol(vt_symbol).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid vt_symbol format: {}",
                vt_symbol
            ))
        })?;

        let dir = parse_direction(direction)?;
        let at = parse_order_type(order_a_type)?;
        let bt = parse_order_type(order_b_type)?;
        let off = parse_offset(offset)?;

        let req = OcoOrderRequest {
            symbol,
            exchange,
            direction: dir,
            volume,
            order_a_price,
            order_a_type: at,
            order_b_price,
            order_b_type: bt,
            offset: off,
            gateway_name: gateway_name.to_string(),
            reference: reference.to_string(),
            tag: tag.to_string(),
        };

        self.inner
            .add_oco_order(req)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Add an OTO (one-triggers-other) order pair.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g., "BTCUSDT.BINANCE")
    ///     primary_direction: Direction for primary order - "LONG", "SHORT", or "NET"
    ///     primary_price: Primary order price
    ///     primary_volume: Primary order volume (must be > 0)
    ///     secondary_direction: Direction for secondary order - "LONG", "SHORT", or "NET"
    ///     secondary_price: Secondary order price
    ///     secondary_volume: Secondary order volume (must be > 0)
    ///     primary_type: Primary order type - "LIMIT", "MARKET", "STOP", "STOP_LIMIT" (default "LIMIT")
    ///     secondary_type: Secondary order type - "LIMIT", "MARKET", "STOP", "STOP_LIMIT" (default "LIMIT")
    ///     offset: Order offset - "NONE", "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY" (default "NONE")
    ///     gateway_name: Gateway name (default "MAIN")
    ///     reference: Optional reference string (default "")
    ///     tag: Optional tag string (default "")
    ///
    /// Returns:
    ///     The group ID on success
    ///
    /// Raises:
    ///     ValueError: If parameters are invalid
    #[pyo3(signature = (vt_symbol, primary_direction, primary_price, primary_volume, secondary_direction, secondary_price, secondary_volume, primary_type="LIMIT", secondary_type="LIMIT", offset="NONE", gateway_name="MAIN", reference="", tag=""))]
    fn add_oto_order(
        &self,
        vt_symbol: &str,
        primary_direction: &str,
        primary_price: f64,
        primary_volume: f64,
        secondary_direction: &str,
        secondary_price: f64,
        secondary_volume: f64,
        primary_type: &str,
        secondary_type: &str,
        offset: &str,
        gateway_name: &str,
        reference: &str,
        tag: &str,
    ) -> PyResult<u64> {
        let (symbol, exchange) = extract_vt_symbol(vt_symbol).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid vt_symbol format: {}",
                vt_symbol
            ))
        })?;

        let pdir = parse_direction(primary_direction)?;
        let sdir = parse_direction(secondary_direction)?;
        let pt = parse_order_type(primary_type)?;
        let st = parse_order_type(secondary_type)?;
        let off = parse_offset(offset)?;

        let req = OtoOrderRequest {
            symbol,
            exchange,
            primary_direction: pdir,
            primary_price,
            primary_volume,
            primary_type: pt,
            secondary_direction: sdir,
            secondary_price,
            secondary_volume,
            secondary_type: st,
            offset: off,
            gateway_name: gateway_name.to_string(),
            reference: reference.to_string(),
            tag: tag.to_string(),
        };

        self.inner
            .add_oto_order(req)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Cancel an order group by ID.
    ///
    /// Args:
    ///     group_id: The group ID to cancel
    ///
    /// Raises:
    ///     ValueError: If the group is not found or not in an active state
    fn cancel_group(&self, group_id: u64) -> PyResult<()> {
        self.inner
            .cancel_group(group_id)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Get all active order groups.
    ///
    /// Returns:
    ///     List of BracketOrderGroup objects with active state
    fn get_active_groups(&self) -> Vec<PyBracketOrderGroup> {
        self.inner
            .get_active_groups()
            .into_iter()
            .map(PyBracketOrderGroup::from_rust)
            .collect()
    }

    /// Get all order groups (including completed, cancelled, rejected).
    ///
    /// Returns:
    ///     List of all BracketOrderGroup objects
    fn get_all_groups(&self) -> Vec<PyBracketOrderGroup> {
        self.inner
            .get_all_groups()
            .into_iter()
            .map(PyBracketOrderGroup::from_rust)
            .collect()
    }

    /// Get a specific order group by ID.
    ///
    /// Args:
    ///     group_id: The group ID
    ///
    /// Returns:
    ///     BracketOrderGroup if found, None otherwise
    fn get_group(&self, group_id: u64) -> Option<PyBracketOrderGroup> {
        self.inner
            .get_group(group_id)
            .map(PyBracketOrderGroup::from_rust)
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

fn parse_direction(s: &str) -> PyResult<Direction> {
    match s.to_uppercase().as_str() {
        "LONG" => Ok(Direction::Long),
        "SHORT" => Ok(Direction::Short),
        "NET" => Ok(Direction::Net),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid direction '{}': expected LONG, SHORT, or NET",
            s
        ))),
    }
}

fn parse_order_type(s: &str) -> PyResult<OrderType> {
    match s.to_uppercase().as_str() {
        "LIMIT" => Ok(OrderType::Limit),
        "MARKET" => Ok(OrderType::Market),
        "STOP" => Ok(OrderType::Stop),
        "STOP_LIMIT" => Ok(OrderType::StopLimit),
        "FAK" => Ok(OrderType::Fak),
        "FOK" => Ok(OrderType::Fok),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid order_type '{}': expected LIMIT, MARKET, STOP, STOP_LIMIT, FAK, or FOK",
            s
        ))),
    }
}

fn parse_offset(s: &str) -> PyResult<Offset> {
    match s.to_uppercase().as_str() {
        "NONE" => Ok(Offset::None),
        "OPEN" => Ok(Offset::Open),
        "CLOSE" => Ok(Offset::Close),
        "CLOSETODAY" => Ok(Offset::CloseToday),
        "CLOSEYESTERDAY" => Ok(Offset::CloseYesterday),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid offset '{}': expected NONE, OPEN, CLOSE, CLOSETODAY, or CLOSEYESTERDAY",
            s
        ))),
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

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

fn format_order_type(t: OrderType) -> String {
    match t {
        OrderType::Limit => "LIMIT".to_string(),
        OrderType::Market => "MARKET".to_string(),
        OrderType::Stop => "STOP".to_string(),
        OrderType::StopLimit => "STOP_LIMIT".to_string(),
        OrderType::Fak => "FAK".to_string(),
        OrderType::Fok => "FOK".to_string(),
        OrderType::Rfq => "RFQ".to_string(),
        OrderType::Etf => "ETF".to_string(),
        OrderType::Gtd => "GTD".to_string(),
        OrderType::PeggedBest => "PEGGED_BEST".to_string(),
    }
}

fn format_status(s: crate::trader::constant::Status) -> String {
    match s {
        crate::trader::constant::Status::Submitting => "Submitting".to_string(),
        crate::trader::constant::Status::NotTraded => "NotTraded".to_string(),
        crate::trader::constant::Status::PartTraded => "PartTraded".to_string(),
        crate::trader::constant::Status::AllTraded => "AllTraded".to_string(),
        crate::trader::constant::Status::Cancelled => "Cancelled".to_string(),
        crate::trader::constant::Status::Rejected => "Rejected".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

/// Register bracket order engine classes with the parent module
pub fn register_bracket_order_engine_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyBracketOrderGroup>()?;
    m.add_class::<PyChildOrderInfo>()?;
    m.add_class::<PyBracketOrderEngine>()?;
    Ok(())
}
