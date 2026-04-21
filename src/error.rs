//! Structured error types for the trading engine.
//!
//! These error enums replace `Result<T, String>` with typed, matchable errors
//! that carry structured context. New code should prefer these over raw strings.
//!
//! ## Migration Strategy
//! - New code: use `Result<T, XxxError>` directly
//! - Existing code: migrate incrementally; `From<String>` impls allow gradual adoption

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Gateway errors
// ---------------------------------------------------------------------------

/// Errors from exchange gateway operations (connect, subscribe, send_order, etc.)
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("subscription failed for {symbol}: {reason}")]
    SubscriptionFailed { symbol: String, reason: String },

    #[error("order rejected: {0}")]
    OrderRejected(String),

    #[error("order cancel failed: {0}")]
    OrderCancelFailed(String),

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("request timeout: {0}")]
    Timeout(String),

    #[error("rate limited: {0}")]
    RateLimited(String),

    #[error("websocket error: {0}")]
    WebSocketError(String),

    #[error("rest api error: {status} {message}")]
    RestApiError { status: u16, message: String },

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for GatewayError {
    fn from(s: String) -> Self {
        GatewayError::Other(s)
    }
}

impl From<&str> for GatewayError {
    fn from(s: &str) -> Self {
        GatewayError::Other(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Database errors
// ---------------------------------------------------------------------------

/// Errors from database operations (save, load, connect, etc.)
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("query failed: {0}")]
    QueryFailed(String),

    #[error("insert failed for table {table}: {reason}")]
    InsertFailed { table: String, reason: String },

    #[error("record not found: {0}")]
    NotFound(String),

    #[error("migration failed: {0}")]
    MigrationFailed(String),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl From<String> for DatabaseError {
    fn from(s: String) -> Self {
        DatabaseError::Other(s)
    }
}

impl From<&str> for DatabaseError {
    fn from(s: &str) -> Self {
        DatabaseError::Other(s.to_string())
    }
}

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for DatabaseError {
    fn from(e: rusqlite::Error) -> Self {
        DatabaseError::QueryFailed(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Strategy errors
// ---------------------------------------------------------------------------

/// Errors from strategy operations (init, start, order placement, etc.)
#[derive(Debug, thiserror::Error)]
pub enum StrategyError {
    #[error("strategy not found: {0}")]
    NotFound(String),

    #[error("strategy not initialized: {0}")]
    NotInitialized(String),

    #[error("strategy already exists: {0}")]
    AlreadyExists(String),

    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("order failed: {0}")]
    OrderFailed(String),

    #[error("position error: {0}")]
    PositionError(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for StrategyError {
    fn from(s: String) -> Self {
        StrategyError::Other(s)
    }
}

impl From<&str> for StrategyError {
    fn from(s: &str) -> Self {
        StrategyError::Other(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Backtest errors
// ---------------------------------------------------------------------------

/// Errors from backtesting operations (data loading, simulation, etc.)
#[derive(Debug, thiserror::Error)]
pub enum BacktestError {
    #[error("no data loaded")]
    NoData,

    #[error("no strategy configured")]
    NoStrategy,

    #[error("data loading failed: {0}")]
    DataLoadingFailed(String),

    #[error("invalid date range: {0}")]
    InvalidDateRange(String),

    #[error("engine error: {0}")]
    EngineError(String),

    #[error("file error at {path}: {reason}")]
    FileError { path: PathBuf, reason: String },

    #[error("{0}")]
    Other(String),
}

impl From<String> for BacktestError {
    fn from(s: String) -> Self {
        BacktestError::Other(s)
    }
}

impl From<&str> for BacktestError {
    fn from(s: &str) -> Self {
        BacktestError::Other(s.to_string())
    }
}
