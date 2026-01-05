//! Binance API constants and mappings.

use std::collections::HashMap;
use once_cell::sync::Lazy;

use crate::trader::{Direction, Interval, OrderType, Status};

// ============================================================================
// REST API Hosts
// ============================================================================

/// Spot REST API host (production)
pub const SPOT_REST_HOST: &str = "https://api.binance.com";

/// Spot REST API host (testnet)
pub const SPOT_TESTNET_REST_HOST: &str = "https://testnet.binance.vision";

/// USDT-M Futures REST API host (production)
pub const USDT_REST_HOST: &str = "https://fapi.binance.com";

/// USDT-M Futures REST API host (testnet)
pub const USDT_TESTNET_REST_HOST: &str = "https://testnet.binancefuture.com";

// ============================================================================
// WebSocket Hosts
// ============================================================================

/// Spot WebSocket host for trade stream (production)
pub const SPOT_WS_TRADE_HOST: &str = "wss://stream.binance.com:9443/ws/";

/// Spot WebSocket host for market data stream (production)
pub const SPOT_WS_DATA_HOST: &str = "wss://stream.binance.com:9443/stream";

/// Spot WebSocket host for trade stream (testnet)
pub const SPOT_TESTNET_WS_TRADE_HOST: &str = "wss://testnet.binance.vision/ws/";

/// Spot WebSocket host for market data stream (testnet)
pub const SPOT_TESTNET_WS_DATA_HOST: &str = "wss://testnet.binance.vision/stream";

/// USDT-M Futures WebSocket host for trade stream (production)
pub const USDT_WS_TRADE_HOST: &str = "wss://fstream.binance.com/ws/";

/// USDT-M Futures WebSocket host for market data stream (production)
pub const USDT_WS_DATA_HOST: &str = "wss://fstream.binance.com/stream";

/// USDT-M Futures WebSocket host for trade stream (testnet)
pub const USDT_TESTNET_WS_TRADE_HOST: &str = "wss://stream.binancefuture.com/ws/";

/// USDT-M Futures WebSocket host for market data stream (testnet)
pub const USDT_TESTNET_WS_DATA_HOST: &str = "wss://stream.binancefuture.com/stream";

// ============================================================================
// Security Types
// ============================================================================

/// Security type for API requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// No security required
    None,
    /// Signed request (requires timestamp and signature)
    Signed,
    /// API key required (in header)
    ApiKey,
}

// ============================================================================
// Status Mappings
// ============================================================================

/// Map Binance order status to VT status
pub static STATUS_BINANCE2VT: Lazy<HashMap<&'static str, Status>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("NEW", Status::NotTraded);
    m.insert("PARTIALLY_FILLED", Status::PartTraded);
    m.insert("FILLED", Status::AllTraded);
    m.insert("CANCELED", Status::Cancelled);
    m.insert("REJECTED", Status::Rejected);
    m.insert("EXPIRED", Status::Cancelled);
    m
});

// ============================================================================
// Order Type Mappings
// ============================================================================

/// Map VT order type to Binance order type (Spot)
pub static ORDERTYPE_VT2BINANCE: Lazy<HashMap<OrderType, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(OrderType::Limit, "LIMIT");
    m.insert(OrderType::Market, "MARKET");
    m
});

/// Map Binance order type to VT order type (Spot)
pub static ORDERTYPE_BINANCE2VT: Lazy<HashMap<&'static str, OrderType>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("LIMIT", OrderType::Limit);
    m.insert("MARKET", OrderType::Market);
    m
});

/// Map VT order type to Binance order type with time-in-force (Futures)
pub static ORDERTYPE_VT2BINANCE_FUTURES: Lazy<HashMap<OrderType, (&'static str, &'static str)>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(OrderType::Limit, ("LIMIT", "GTC"));
    m.insert(OrderType::Market, ("MARKET", "GTC"));
    m.insert(OrderType::Fak, ("LIMIT", "IOC"));
    m.insert(OrderType::Fok, ("LIMIT", "FOK"));
    m
});

/// Map Binance order type with time-in-force to VT order type (Futures)
pub static ORDERTYPE_BINANCE2VT_FUTURES: Lazy<HashMap<(&'static str, &'static str), OrderType>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(("LIMIT", "GTC"), OrderType::Limit);
    m.insert(("MARKET", "GTC"), OrderType::Market);
    m.insert(("LIMIT", "IOC"), OrderType::Fak);
    m.insert(("LIMIT", "FOK"), OrderType::Fok);
    m
});

// ============================================================================
// Direction Mappings
// ============================================================================

/// Map VT direction to Binance direction
pub static DIRECTION_VT2BINANCE: Lazy<HashMap<Direction, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(Direction::Long, "BUY");
    m.insert(Direction::Short, "SELL");
    m
});

/// Map Binance direction to VT direction
pub static DIRECTION_BINANCE2VT: Lazy<HashMap<&'static str, Direction>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("BUY", Direction::Long);
    m.insert("SELL", Direction::Short);
    m
});

// ============================================================================
// Interval Mappings
// ============================================================================

/// Map VT interval to Binance interval
pub static INTERVAL_VT2BINANCE: Lazy<HashMap<Interval, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(Interval::Minute, "1m");
    m.insert(Interval::Hour, "1h");
    m.insert(Interval::Daily, "1d");
    m
});

/// Get interval duration in seconds
pub fn get_interval_seconds(interval: Interval) -> i64 {
    match interval {
        Interval::Minute => 60,
        Interval::Hour => 3600,
        Interval::Daily => 86400,
        _ => 60,
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate datetime from Binance timestamp (milliseconds)
pub fn timestamp_to_datetime(timestamp_ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(timestamp_ms)
        .unwrap_or_else(|| chrono::Utc::now())
}

/// Generate datetime from Binance timestamp (milliseconds) with local timezone
pub fn timestamp_to_local_datetime(timestamp_ms: i64) -> chrono::DateTime<chrono::Local> {
    let utc = timestamp_to_datetime(timestamp_ms);
    utc.with_timezone(&chrono::Local)
}
