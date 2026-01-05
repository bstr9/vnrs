//! Strategy Base Types
//! 
//! Fundamental types and constants for the strategy framework

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::trader::{Direction, Offset, OrderType};

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
    /// Price to trigger
    pub price: f64,
    /// Order volume
    pub volume: f64,
    /// Order type after trigger
    pub order_type: OrderType,
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

/// APP name constant
pub const APP_NAME: &str = "StrategyTrading";

/// Stop order prefix
pub const STOPORDER_PREFIX: &str = "STOP";
