//! Strategy Base Types
//!
//! Fundamental types and constants for the strategy framework

use crate::trader::{Direction, Offset, OrderType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Strategy type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyType {
    /// Spot trading strategy
    Spot,
    /// Futures CTA strategy
    Futures,
    /// Grid trading strategy
    Grid,
    /// Market making strategy
    MarketMaking,
    /// Arbitrage strategy
    Arbitrage,
}

/// Strategy state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyState {
    /// Not initialized
    NotInited,
    /// Initialized but not trading
    Inited,
    /// Currently trading
    Trading,
    /// Stopped
    Stopped,
    /// Error state (strategy encountered an error)
    Error,
}

/// Stop order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopOrderStatus {
    /// Waiting to be triggered
    Waiting,
    /// Triggered and submitted
    Triggered,
    /// Cancelled
    Cancelled,
}

/// Stop order structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopOrder {
    /// Unique stop order ID
    pub stop_orderid: String,
    /// Original order request symbol
    pub vt_symbol: String,
    /// Order direction
    pub direction: Direction,
    /// Offset type (for futures)
    pub offset: Option<Offset>,
    /// Price to trigger (stop price)
    pub price: f64,
    /// Order volume
    pub volume: f64,
    /// Order type after trigger
    pub order_type: OrderType,
    /// Limit price for StopLimit orders (None for Stop/Market)
    pub limit_price: Option<f64>,
    /// Strategy name that created this order
    pub strategy_name: String,
    /// Lock flag (for position management)
    pub lock: bool,
    /// Actual VT orderid after submission (if triggered)
    pub vt_orderid: Option<String>,
    /// Current status
    pub status: StopOrderStatus,
    /// Creation time
    pub datetime: DateTime<Utc>,
}

impl StopOrder {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        stop_orderid: String,
        vt_symbol: String,
        direction: Direction,
        offset: Option<Offset>,
        price: f64,
        volume: f64,
        order_type: OrderType,
        strategy_name: String,
    ) -> Self {
        Self {
            stop_orderid,
            vt_symbol,
            direction,
            offset,
            price,
            volume,
            order_type,
            limit_price: None,
            strategy_name,
            lock: false,
            vt_orderid: None,
            status: StopOrderStatus::Waiting,
            datetime: Utc::now(),
        }
    }
}

/// Strategy parameter type
pub type StrategyParam = serde_json::Value;

/// Strategy setting (parameters configuration)
pub type StrategySetting = std::collections::HashMap<String, StrategyParam>;

/// Per-strategy risk configuration for live trading.
///
/// These limits are enforced at the `StrategyEngine` level *before*
/// orders reach `MainEngine`, providing strategy-level guard rails
/// that complement the global `RiskEngine` checks.
///
/// By default all limits are disabled (set to `f64::MAX`) so that
/// existing strategies are not affected. Enable specific checks by
/// setting the corresponding `check_*` flag to `true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyRiskConfig {
    /// Maximum order volume per order (0 = unlimited)
    pub max_order_volume: f64,
    /// Maximum position volume (absolute value) per symbol (0 = unlimited)
    pub max_position_volume: f64,
    /// Maximum notional value per order: price * volume * size (0 = unlimited)
    pub max_order_notional: f64,
    /// Maximum active (pending) orders for this strategy (0 = unlimited)
    pub max_active_orders: usize,
    /// Whether to enforce max_order_volume
    pub check_order_volume: bool,
    /// Whether to enforce max_position_volume
    pub check_position_volume: bool,
    /// Whether to enforce max_order_notional
    pub check_order_notional: bool,
    /// Whether to enforce max_active_orders
    pub check_active_orders: bool,
}

impl Default for StrategyRiskConfig {
    fn default() -> Self {
        Self {
            max_order_volume: f64::MAX,
            max_position_volume: f64::MAX,
            max_order_notional: f64::MAX,
            max_active_orders: usize::MAX,
            check_order_volume: false,
            check_position_volume: false,
            check_order_notional: false,
            check_active_orders: false,
        }
    }
}

impl StrategyRiskConfig {
    /// Create an unrestricted config (all checks disabled)
    pub fn unrestricted() -> Self {
        Self::default()
    }

    /// Create a conservative config suitable for spot strategies
    pub fn conservative_spot() -> Self {
        Self {
            max_order_volume: 1.0,
            max_position_volume: 5.0,
            max_order_notional: 500_000.0,
            max_active_orders: 10,
            check_order_volume: true,
            check_position_volume: true,
            check_order_notional: true,
            check_active_orders: true,
        }
    }

    /// Create a conservative config suitable for futures strategies
    pub fn conservative_futures() -> Self {
        Self {
            max_order_volume: 10.0,
            max_position_volume: 50.0,
            max_order_notional: 1_000_000.0,
            max_active_orders: 20,
            check_order_volume: true,
            check_position_volume: true,
            check_order_notional: true,
            check_active_orders: true,
        }
    }
}

/// APP name constant
pub const APP_NAME: &str = "StrategyTrading";

/// Stop order prefix
pub const STOPORDER_PREFIX: &str = "STOP";

/// Request to create a stop order (used for routing from strategy to engine)
#[derive(Debug, Clone)]
pub struct StopOrderRequest {
    /// Symbol in vt_symbol format (e.g., "BTCUSDT.BINANCE")
    pub vt_symbol: String,
    /// Order direction
    pub direction: Direction,
    /// Offset type (for futures)
    pub offset: Option<Offset>,
    /// Price to trigger the stop order
    pub price: f64,
    /// Order volume
    pub volume: f64,
    /// Order type after trigger
    pub order_type: OrderType,
    /// Limit price for StopLimit orders (None for Stop/Market)
    pub limit_price: Option<f64>,
    /// Lock flag (for position management)
    pub lock: bool,
}

impl StopOrderRequest {
    pub fn new(
        vt_symbol: String,
        direction: Direction,
        offset: Option<Offset>,
        price: f64,
        volume: f64,
        order_type: OrderType,
        lock: bool,
    ) -> Self {
        Self {
            vt_symbol,
            direction,
            offset,
            price,
            volume,
            order_type,
            limit_price: None,
            lock,
        }
    }
}

/// Cancellation request (used for routing from strategy to engine)
#[derive(Debug, Clone)]
pub enum CancelRequestType {
    /// Cancel a regular order by vt_orderid
    Order(String),
    /// Cancel a stop order by stop_orderid
    StopOrder(String),
}
