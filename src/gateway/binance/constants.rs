//! Binance API constants and mappings.

use once_cell::sync::Lazy;
use std::collections::HashMap;

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
// WebSocket API Hosts (for user data stream subscription)
// ============================================================================

/// Spot WebSocket API endpoint (production) - for user data stream subscription
pub const SPOT_WS_API_HOST: &str = "wss://ws-api.binance.com:443/ws-api/v3";

/// Spot WebSocket API endpoint (testnet) - for user data stream subscription
pub const SPOT_TESTNET_WS_API_HOST: &str = "wss://testnet.binance.vision/ws-api/v3";

/// USDT-M Futures WebSocket API endpoint (production)
pub const USDT_WS_API_HOST: &str = "wss://ws-api.binance.com:443/ws-api/v3";

/// USDT-M Futures WebSocket API endpoint (testnet)
pub const USDT_TESTNET_WS_API_HOST: &str = "wss://testnet.binancefuture.com/ws-api/v3";

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
/// Fak (Fill-and-Kill) and Fok (Fill-or-Kill) map to LIMIT with timeInForce set in gateway
/// StopLimit maps to STOP (Binance Spot STOP = stop-limit with stopPrice + price)
pub static ORDERTYPE_VT2BINANCE: Lazy<HashMap<OrderType, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(OrderType::Limit, "LIMIT");
    m.insert(OrderType::Market, "MARKET");
    m.insert(OrderType::Stop, "STOP_LOSS");
    m.insert(OrderType::StopLimit, "STOP");
    m.insert(OrderType::Fak, "LIMIT");
    m.insert(OrderType::Fok, "LIMIT");
    m
});

/// Map Binance order type to VT order type (Spot)
/// STOP on Binance Spot = stop-limit (requires stopPrice + price)
/// STOP_LOSS on Binance Spot = stop market (requires stopPrice only)
/// TAKE_PROFIT on Binance Spot = take-profit limit (requires stopPrice + price)
pub static ORDERTYPE_BINANCE2VT: Lazy<HashMap<&'static str, OrderType>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("LIMIT", OrderType::Limit);
    m.insert("MARKET", OrderType::Market);
    m.insert("STOP_LOSS", OrderType::Stop);
    m.insert("STOP", OrderType::StopLimit);
    m.insert("TAKE_PROFIT", OrderType::StopLimit);
    m
});

/// Map VT order type to Binance order type with time-in-force (Futures)
/// Stop → STOP_MARKET (stop market order on futures)
/// StopLimit → STOP (stop-limit order on futures, requires stopPrice + price)
pub static ORDERTYPE_VT2BINANCE_FUTURES: Lazy<HashMap<OrderType, (&'static str, &'static str)>> =
    Lazy::new(|| {
        let mut m = HashMap::new();
        m.insert(OrderType::Limit, ("LIMIT", "GTC"));
        m.insert(OrderType::Market, ("MARKET", "GTC"));
        m.insert(OrderType::Stop, ("STOP_MARKET", "GTC"));
        m.insert(OrderType::StopLimit, ("STOP", "GTC"));
        m.insert(OrderType::Fak, ("LIMIT", "IOC"));
        m.insert(OrderType::Fok, ("LIMIT", "FOK"));
        m
    });

/// Map Binance order type with time-in-force to VT order type (Futures)
/// STOP = stop-limit, STOP_MARKET = stop market, TAKE_PROFIT = take-profit limit
pub static ORDERTYPE_BINANCE2VT_FUTURES: Lazy<HashMap<(&'static str, &'static str), OrderType>> =
    Lazy::new(|| {
        let mut m = HashMap::new();
        m.insert(("LIMIT", "GTC"), OrderType::Limit);
        m.insert(("LIMIT", "GTX"), OrderType::Limit); // Post-Only limit
        m.insert(("MARKET", "GTC"), OrderType::Market);
        m.insert(("STOP", "GTC"), OrderType::StopLimit);
        m.insert(("STOP_MARKET", "GTC"), OrderType::Stop);
        m.insert(("TAKE_PROFIT", "GTC"), OrderType::StopLimit);
        m.insert(("TAKE_PROFIT_MARKET", "GTC"), OrderType::Stop);
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
    m.insert(Interval::Second, "1s");
    m.insert(Interval::Minute, "1m");
    m.insert(Interval::Minute5, "5m");
    m.insert(Interval::Minute15, "15m");
    m.insert(Interval::Minute30, "30m");
    m.insert(Interval::Hour, "1h");
    m.insert(Interval::Hour4, "4h");
    m.insert(Interval::Daily, "1d");
    m.insert(Interval::Weekly, "1w");
    m
});

/// Get interval duration in seconds
pub fn get_interval_seconds(interval: Interval) -> i64 {
    match interval {
        Interval::Second => 1,
        Interval::Minute => 60,
        Interval::Minute5 => 300,
        Interval::Minute15 => 900,
        Interval::Minute30 => 1800,
        Interval::Hour => 3600,
        Interval::Hour4 => 14400,
        Interval::Daily => 86400,
        Interval::Weekly => 604800,
        Interval::Tick => 0,
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Generate datetime from Binance timestamp (milliseconds)
pub fn timestamp_to_datetime(timestamp_ms: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_millis(timestamp_ms).unwrap_or_else(chrono::Utc::now)
}

/// Generate datetime from Binance timestamp (milliseconds) with local timezone
pub fn timestamp_to_local_datetime(timestamp_ms: i64) -> chrono::DateTime<chrono::Local> {
    let utc = timestamp_to_datetime(timestamp_ms);
    utc.with_timezone(&chrono::Local)
}
