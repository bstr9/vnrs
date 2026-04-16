//! PyRiskManager — PyO3 wrapper exposing RiskEngine to Python strategies.
//!
//! Wraps `RiskEngine` behind `Arc<Mutex<...>>` for thread-safe shared access.
//! Python strategies use `PyRiskManager` to check orders before submission,
//! record completed trades, and reset daily counters.

use std::sync::{Arc, Mutex};

use pyo3::prelude::*;

use crate::backtesting::position::Position;
use crate::backtesting::risk_engine::{RiskCheckResult, RiskConfig, RiskEngine};
use crate::trader::{Direction, Exchange, Offset, OrderData, OrderType, Status};

// ---------------------------------------------------------------------------
// PyRiskConfig — Python-facing risk configuration
// ---------------------------------------------------------------------------

/// Risk engine configuration exposed to Python.
///
/// ```python
/// config = RiskConfig()
/// config.max_order_size = 500.0
/// config.check_notional = False
/// manager = RiskManager(config)
/// ```
#[pyclass(name = "RiskConfig")]
#[derive(Clone)]
pub struct PyRiskConfig {
    inner: RiskConfig,
}

#[pymethods]
impl PyRiskConfig {
    #[new]
    fn new() -> Self {
        Self {
            inner: RiskConfig::default(),
        }
    }

    /// Create a RiskConfig with all checks disabled.
    #[staticmethod]
    fn unrestricted() -> Self {
        Self {
            inner: RiskConfig {
                check_order_size: false,
                check_position_size: false,
                check_notional: false,
                check_open_orders: false,
                check_daily_trades: false,
                check_daily_turnover: false,
                ..RiskConfig::default()
            },
        }
    }

    // --- Getters & Setters ---

    #[getter]
    fn max_order_size(&self) -> f64 {
        self.inner.max_order_size
    }

    #[setter]
    fn set_max_order_size(&mut self, value: f64) {
        self.inner.max_order_size = value;
    }

    #[getter]
    fn max_position_size(&self) -> f64 {
        self.inner.max_position_size
    }

    #[setter]
    fn set_max_position_size(&mut self, value: f64) {
        self.inner.max_position_size = value;
    }

    #[getter]
    fn max_notional_per_order(&self) -> f64 {
        self.inner.max_notional_per_order
    }

    #[setter]
    fn set_max_notional_per_order(&mut self, value: f64) {
        self.inner.max_notional_per_order = value;
    }

    #[getter]
    fn max_open_orders(&self) -> usize {
        self.inner.max_open_orders
    }

    #[setter]
    fn set_max_open_orders(&mut self, value: usize) {
        self.inner.max_open_orders = value;
    }

    #[getter]
    fn max_daily_trades(&self) -> u64 {
        self.inner.max_daily_trades
    }

    #[setter]
    fn set_max_daily_trades(&mut self, value: u64) {
        self.inner.max_daily_trades = value;
    }

    #[getter]
    fn max_daily_turnover(&self) -> f64 {
        self.inner.max_daily_turnover
    }

    #[setter]
    fn set_max_daily_turnover(&mut self, value: f64) {
        self.inner.max_daily_turnover = value;
    }

    #[getter]
    fn check_order_size(&self) -> bool {
        self.inner.check_order_size
    }

    #[setter]
    fn set_check_order_size(&mut self, value: bool) {
        self.inner.check_order_size = value;
    }

    #[getter]
    fn check_position_size(&self) -> bool {
        self.inner.check_position_size
    }

    #[setter]
    fn set_check_position_size(&mut self, value: bool) {
        self.inner.check_position_size = value;
    }

    #[getter]
    fn check_notional(&self) -> bool {
        self.inner.check_notional
    }

    #[setter]
    fn set_check_notional(&mut self, value: bool) {
        self.inner.check_notional = value;
    }

    #[getter]
    fn check_open_orders(&self) -> bool {
        self.inner.check_open_orders
    }

    #[setter]
    fn set_check_open_orders(&mut self, value: bool) {
        self.inner.check_open_orders = value;
    }

    #[getter]
    fn check_daily_trades(&self) -> bool {
        self.inner.check_daily_trades
    }

    #[setter]
    fn set_check_daily_trades(&mut self, value: bool) {
        self.inner.check_daily_trades = value;
    }

    #[getter]
    fn check_daily_turnover(&self) -> bool {
        self.inner.check_daily_turnover
    }

    #[setter]
    fn set_check_daily_turnover(&mut self, value: bool) {
        self.inner.check_daily_turnover = value;
    }

    fn __repr__(&self) -> String {
        format!(
            "RiskConfig(max_order_size={}, max_position_size={}, max_notional_per_order={}, \
             max_open_orders={}, max_daily_trades={}, max_daily_turnover={}, \
             check_order_size={}, check_position_size={}, check_notional={}, \
             check_open_orders={}, check_daily_trades={}, check_daily_turnover={})",
            self.inner.max_order_size,
            self.inner.max_position_size,
            self.inner.max_notional_per_order,
            self.inner.max_open_orders,
            self.inner.max_daily_trades,
            self.inner.max_daily_turnover,
            self.inner.check_order_size,
            self.inner.check_position_size,
            self.inner.check_notional,
            self.inner.check_open_orders,
            self.inner.check_daily_trades,
            self.inner.check_daily_turnover,
        )
    }
}

impl PyRiskConfig {
    pub fn from_inner(config: RiskConfig) -> Self {
        Self { inner: config }
    }

    pub fn into_inner(self) -> RiskConfig {
        self.inner
    }
}

impl Default for PyRiskConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PyRiskCheckResult — Python-facing risk check result
// ---------------------------------------------------------------------------

/// Result of a risk check, indicating whether an order is approved.
///
/// ```python
/// result = manager.check_order(...)
/// if result.is_approved:
///     print("Order approved")
/// else:
///     print(f"Rejected: {result.reason}")
/// ```
#[pyclass(name = "RiskCheckResult")]
#[derive(Clone)]
pub struct PyRiskCheckResult {
    is_approved: bool,
    reason: Option<String>,
}

#[pymethods]
impl PyRiskCheckResult {
    /// Whether the order passed all risk checks.
    #[getter]
    pub fn is_approved(&self) -> bool {
        self.is_approved
    }

    /// Rejection reason, or None if approved.
    #[getter]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    fn __repr__(&self) -> String {
        match &self.reason {
            Some(r) => format!("RiskCheckResult(is_approved=False, reason='{}')", r),
            None => "RiskCheckResult(is_approved=True)".to_string(),
        }
    }

    fn __bool__(&self) -> bool {
        self.is_approved
    }
}

impl PyRiskCheckResult {
    pub fn from_inner(result: RiskCheckResult) -> Self {
        Self {
            is_approved: result.is_approved,
            reason: result.reason,
        }
    }
}

// ---------------------------------------------------------------------------
// Inner state held behind Arc<Mutex<...>>
// ---------------------------------------------------------------------------

/// Internal state combining the RiskEngine with tracked daily counters
/// that are mirrored for Python access (since RiskEngine's fields are private).
struct RiskManagerState {
    engine: RiskEngine,
    daily_trade_count: u64,
    daily_turnover: f64,
}

impl RiskManagerState {
    fn new(config: RiskConfig) -> Self {
        Self {
            engine: RiskEngine::new(config),
            daily_trade_count: 0,
            daily_turnover: 0.0,
        }
    }

    fn new_unrestricted() -> Self {
        Self {
            engine: RiskEngine::new_unrestricted(),
            daily_trade_count: 0,
            daily_turnover: 0.0,
        }
    }

    fn record_trade(&mut self, trade_value: f64) {
        self.engine.record_trade(trade_value);
        self.daily_trade_count += 1;
        self.daily_turnover += trade_value;
    }

    fn reset_daily(&mut self) {
        self.engine.reset_daily();
        self.daily_trade_count = 0;
        self.daily_turnover = 0.0;
    }
}

// ---------------------------------------------------------------------------
// PyRiskManager — main PyO3 class wrapping RiskEngine
// ---------------------------------------------------------------------------

/// Thread-safe risk manager exposed to Python strategies.
///
/// ```python
/// config = RiskConfig()
/// config.max_order_size = 100.0
/// manager = RiskManager(config)
///
/// result = manager.check_order(
///     vt_symbol="BTCUSDT.BINANCE",
///     direction="LONG",
///     offset="OPEN",
///     price=50000.0,
///     volume=1.0,
///     order_type="LIMIT",
///     position_qty=0.0,
///     active_orders=0,
/// )
///
/// if result:
///     manager.record_trade(50000.0)
/// ```
#[pyclass(name = "RiskManager")]
pub struct PyRiskManager {
    inner: Arc<Mutex<RiskManagerState>>,
}

#[pymethods]
impl PyRiskManager {
    /// Create a new RiskManager with the given configuration.
    #[new]
    fn new(config: &PyRiskConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RiskManagerState::new(config.inner.clone()))),
        }
    }

    /// Create a RiskManager with all risk checks disabled.
    #[staticmethod]
    fn unrestricted() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RiskManagerState::new_unrestricted())),
        }
    }

    /// Check whether an order passes all risk checks.
    ///
    /// Args:
    ///     vt_symbol: Symbol in SYMBOL.EXCHANGE format (e.g. "BTCUSDT.BINANCE")
    ///     direction: "LONG", "SHORT", "BUY", or "SELL"
    ///     offset: "NONE", "OPEN", "CLOSE", "CLOSE_TODAY", or "CLOSE_YESTERDAY"
    ///     price: Order price
    ///     volume: Order quantity
    ///     order_type: "MARKET", "LIMIT", or "STOP"
    ///     position_qty: Current signed position quantity (positive=long, negative=short)
    ///     active_orders: Number of currently active/pending orders
    ///
    /// Returns:
    ///     RiskCheckResult indicating approval or rejection with reason.
    #[pyo3(signature = (vt_symbol, direction, offset, price, volume, order_type, position_qty=0.0, active_orders=0))]
    pub fn check_order(
        &self,
        vt_symbol: &str,
        direction: &str,
        offset: &str,
        price: f64,
        volume: f64,
        order_type: &str,
        position_qty: f64,
        active_orders: usize,
    ) -> PyResult<PyRiskCheckResult> {
        let dir = parse_direction(direction)?;
        let off = parse_offset(offset)?;
        let ot = parse_order_type(order_type)?;

        // Parse vt_symbol into symbol + exchange parts
        let (symbol, exchange) = parse_vt_symbol(vt_symbol);

        let order = OrderData {
            gateway_name: String::new(),
            symbol,
            exchange,
            orderid: String::new(),
            order_type: ot,
            direction: Some(dir),
            offset: off,
            price,
            volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: None,
            reference: String::new(),
            extra: None,
        };

        // Build a minimal Position with the given signed quantity
        let position = build_position(&order.symbol, order.exchange, position_qty);

        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let result = state
            .engine
            .check_order(&order, &position, active_orders, 1.0);
        Ok(PyRiskCheckResult::from_inner(result))
    }

    /// Record a completed trade for daily tracking.
    ///
    /// Args:
    ///     trade_value: Notional value of the completed trade.
    fn record_trade(&self, trade_value: f64) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_trade(trade_value);
    }

    /// Reset daily counters (call at the start of a new trading day).
    fn reset_daily(&self) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reset_daily();
    }

    /// Get a copy of the current risk configuration.
    fn get_config(&self) -> PyRiskConfig {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        PyRiskConfig::from_inner(state.engine.config().clone())
    }

    /// Update the risk configuration.
    fn set_config(&self, config: &PyRiskConfig) {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .engine
            .set_config(config.inner.clone());
    }

    /// Current daily trade count.
    #[getter]
    fn daily_trade_count(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .daily_trade_count
    }

    /// Current daily turnover.
    #[getter]
    fn daily_turnover(&self) -> f64 {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .daily_turnover
    }

    fn __repr__(&self) -> String {
        let state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "RiskManager(daily_trades={}, daily_turnover={:.2})",
            state.daily_trade_count, state.daily_turnover,
        )
    }
}

// ---------------------------------------------------------------------------
// Position builder helper
// ---------------------------------------------------------------------------

/// Build a minimal `Position` with a given signed quantity by applying a simulated fill.
fn build_position(symbol: &str, exchange: Exchange, signed_qty: f64) -> Position {
    let mut pos = Position::new(String::new(), symbol.to_string(), exchange);
    if signed_qty != 0.0 {
        let direction = if signed_qty > 0.0 {
            Direction::Long
        } else {
            Direction::Short
        };
        let fill = crate::trader::TradeData {
            gateway_name: String::new(),
            symbol: symbol.to_string(),
            exchange,
            orderid: String::new(),
            tradeid: String::new(),
            direction: Some(direction),
            offset: Offset::Open,
            price: 1.0,
            volume: signed_qty.abs(),
            datetime: None,
            extra: None,
        };
        let _ = pos.apply_fill(&fill);
    }
    pos
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

fn parse_direction(s: &str) -> PyResult<Direction> {
    match s.to_uppercase().as_str() {
        "LONG" | "BUY" => Ok(Direction::Long),
        "SHORT" | "SELL" => Ok(Direction::Short),
        "NET" => Ok(Direction::Net),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid direction '{}'. Expected LONG/BUY, SHORT/SELL, or NET.",
            s
        ))),
    }
}

fn parse_offset(s: &str) -> PyResult<Offset> {
    match s.to_uppercase().as_str() {
        "NONE" => Ok(Offset::None),
        "OPEN" => Ok(Offset::Open),
        "CLOSE" => Ok(Offset::Close),
        "CLOSE_TODAY" => Ok(Offset::CloseToday),
        "CLOSE_YESTERDAY" => Ok(Offset::CloseYesterday),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid offset '{}'. Expected NONE, OPEN, CLOSE, CLOSE_TODAY, or CLOSE_YESTERDAY.",
            s
        ))),
    }
}

fn parse_order_type(s: &str) -> PyResult<OrderType> {
    match s.to_uppercase().as_str() {
        "MARKET" => Ok(OrderType::Market),
        "LIMIT" => Ok(OrderType::Limit),
        "STOP" | "STOP_LIMIT" => Ok(OrderType::Stop),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Invalid order_type '{}'. Expected MARKET, LIMIT, or STOP.",
            s
        ))),
    }
}

/// Parse "SYMBOL.EXCHANGE" into (symbol, Exchange).
fn parse_vt_symbol(vt_symbol: &str) -> (String, Exchange) {
    match vt_symbol.rsplit_once('.') {
        Some((sym, exch)) => {
            let exchange = match exch.to_uppercase().as_str() {
                "BINANCE" => Exchange::Binance,
                "BINANCE_USDM" => Exchange::BinanceUsdm,
                "BINANCE_COINM" => Exchange::BinanceCoinm,
                _ => Exchange::Local,
            };
            (sym.to_string(), exchange)
        }
        None => (vt_symbol.to_string(), Exchange::Local),
    }
}

// ---------------------------------------------------------------------------
// Registration helper (called from bindings.rs)
// ---------------------------------------------------------------------------

/// Register PyRiskManager, PyRiskConfig, and PyRiskCheckResult with the PyO3 module.
pub fn register_risk_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRiskManager>()?;
    m.add_class::<PyRiskConfig>()?;
    m.add_class::<PyRiskCheckResult>()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- PyRiskConfig tests ----

    #[test]
    fn test_risk_config_default() {
        let config = PyRiskConfig::new();
        assert_eq!(config.max_order_size(), 1000.0);
        assert_eq!(config.max_position_size(), 5000.0);
        assert_eq!(config.max_notional_per_order(), 1_000_000.0);
        assert_eq!(config.max_open_orders(), 50);
        assert_eq!(config.max_daily_trades(), 500);
        assert_eq!(config.max_daily_turnover(), 50_000_000.0);
        assert!(config.check_order_size());
        assert!(config.check_position_size());
        assert!(config.check_notional());
        assert!(config.check_open_orders());
        assert!(config.check_daily_trades());
        assert!(config.check_daily_turnover());
    }

    #[test]
    fn test_risk_config_unrestricted() {
        let config = PyRiskConfig::unrestricted();
        assert!(!config.check_order_size());
        assert!(!config.check_position_size());
        assert!(!config.check_notional());
        assert!(!config.check_open_orders());
        assert!(!config.check_daily_trades());
        assert!(!config.check_daily_turnover());
        // Numeric limits still have defaults
        assert_eq!(config.max_order_size(), 1000.0);
    }

    #[test]
    fn test_risk_config_setters() {
        let mut config = PyRiskConfig::new();
        config.set_max_order_size(500.0);
        assert_eq!(config.max_order_size(), 500.0);

        config.set_max_position_size(2000.0);
        assert_eq!(config.max_position_size(), 2000.0);

        config.set_max_notional_per_order(500_000.0);
        assert_eq!(config.max_notional_per_order(), 500_000.0);

        config.set_max_open_orders(10);
        assert_eq!(config.max_open_orders(), 10);

        config.set_max_daily_trades(100);
        assert_eq!(config.max_daily_trades(), 100);

        config.set_max_daily_turnover(10_000_000.0);
        assert_eq!(config.max_daily_turnover(), 10_000_000.0);

        config.set_check_order_size(false);
        assert!(!config.check_order_size());

        config.set_check_position_size(false);
        assert!(!config.check_position_size());

        config.set_check_notional(false);
        assert!(!config.check_notional());

        config.set_check_open_orders(false);
        assert!(!config.check_open_orders());

        config.set_check_daily_trades(false);
        assert!(!config.check_daily_trades());

        config.set_check_daily_turnover(false);
        assert!(!config.check_daily_turnover());
    }

    #[test]
    fn test_risk_config_into_inner() {
        let mut config = PyRiskConfig::new();
        config.set_max_order_size(42.0);
        let inner = config.into_inner();
        assert_eq!(inner.max_order_size, 42.0);
    }

    #[test]
    fn test_risk_config_from_inner() {
        let inner = RiskConfig {
            max_order_size: 99.0,
            ..RiskConfig::default()
        };
        let config = PyRiskConfig::from_inner(inner);
        assert_eq!(config.max_order_size(), 99.0);
    }

    // ---- PyRiskCheckResult tests ----

    #[test]
    fn test_check_result_approved() {
        let result = PyRiskCheckResult::from_inner(RiskCheckResult::approved());
        assert!(result.is_approved());
        assert!(result.reason().is_none());
    }

    #[test]
    fn test_check_result_rejected() {
        let result = PyRiskCheckResult::from_inner(RiskCheckResult::rejected("Too large"));
        assert!(!result.is_approved());
        assert_eq!(result.reason(), Some("Too large"));
    }

    // ---- PyRiskManager tests ----

    #[test]
    fn test_risk_manager_new() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);
        assert_eq!(manager.daily_trade_count(), 0);
        assert_eq!(manager.daily_turnover(), 0.0);
    }

    #[test]
    fn test_risk_manager_unrestricted() {
        let manager = PyRiskManager::unrestricted();
        let config = manager.get_config();
        assert!(!config.check_order_size());
        assert!(!config.check_position_size());
    }

    #[test]
    fn test_risk_manager_check_order_approved() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(result.is_approved());
        assert!(result.reason().is_none());
    }

    #[test]
    fn test_risk_manager_check_order_rejected_size() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);
        // max_order_size is 1000 by default
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                50000.0,
                2000.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("Order size"));
    }

    #[test]
    fn test_risk_manager_check_order_rejected_position() {
        let mut config = PyRiskConfig::new();
        config.set_max_order_size(10000.0); // Allow large order so position check triggers
        let manager = PyRiskManager::new(&config);
        // max_position_size is 5000 by default, order volume 6000 would exceed it
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                1.0,
                6000.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("Projected position"));
    }

    #[test]
    fn test_risk_manager_check_order_rejected_notional() {
        let mut config = PyRiskConfig::new();
        config.set_max_notional_per_order(1000.0);
        let manager = PyRiskManager::new(&config);
        // notional = 50000 * 1 = 50000 > 1000
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("notional"));
    }

    #[test]
    fn test_risk_manager_check_order_rejected_open_orders() {
        let mut config = PyRiskConfig::new();
        config.set_max_open_orders(2);
        let manager = PyRiskManager::new(&config);
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                5,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("Open orders"));
    }

    #[test]
    fn test_risk_manager_record_trade() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);
        manager.record_trade(50000.0);
        manager.record_trade(30000.0);
        assert_eq!(manager.daily_trade_count(), 2);
        assert!((manager.daily_turnover() - 80000.0).abs() < 1e-10);
    }

    #[test]
    fn test_risk_manager_reset_daily() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);
        manager.record_trade(50000.0);
        assert_eq!(manager.daily_trade_count(), 1);
        manager.reset_daily();
        assert_eq!(manager.daily_trade_count(), 0);
        assert_eq!(manager.daily_turnover(), 0.0);
    }

    #[test]
    fn test_risk_manager_get_set_config() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);

        let retrieved = manager.get_config();
        assert_eq!(retrieved.max_order_size(), 1000.0);

        let mut new_config = PyRiskConfig::new();
        new_config.set_max_order_size(42.0);
        manager.set_config(&new_config);

        let updated = manager.get_config();
        assert_eq!(updated.max_order_size(), 42.0);
    }

    #[test]
    fn test_risk_manager_check_daily_trades_limit() {
        let mut config = PyRiskConfig::new();
        config.set_max_daily_trades(2);
        let manager = PyRiskManager::new(&config);

        // First two trades should be fine
        manager.record_trade(1000.0);
        manager.record_trade(1000.0);

        // Third order should be rejected due to daily trade count
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("Daily trade count"));
    }

    #[test]
    fn test_risk_manager_check_daily_turnover_limit() {
        let mut config = PyRiskConfig::new();
        config.set_max_daily_turnover(1000.0);
        let manager = PyRiskManager::new(&config);

        manager.record_trade(800.0);

        // Turnover is 800, still under limit, order should be fine
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                100.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(result.is_approved());

        // Record another trade pushing turnover to 1000
        manager.record_trade(200.0);

        // Now at limit, should be rejected
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                100.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("Daily turnover"));
    }

    #[test]
    fn test_risk_manager_short_direction() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);

        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "SHORT",
                "OPEN",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(result.is_approved());
    }

    #[test]
    fn test_risk_manager_buy_sell_aliases() {
        let config = PyRiskConfig::new();
        let manager = PyRiskManager::new(&config);

        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "BUY",
                "OPEN",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(result.is_approved());

        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "SELL",
                "CLOSE",
                50000.0,
                1.0,
                "LIMIT",
                0.0,
                0,
            )
            .unwrap();
        assert!(result.is_approved());
    }

    #[test]
    fn test_risk_manager_with_existing_position() {
        let mut config = PyRiskConfig::new();
        config.set_max_order_size(10000.0); // Allow large order so position check triggers
        let manager = PyRiskManager::new(&config);

        // With existing long position of 4000, adding 2000 would exceed max 5000
        let result = manager
            .check_order(
                "BTCUSDT.BINANCE",
                "LONG",
                "OPEN",
                50000.0,
                2000.0,
                "LIMIT",
                4000.0,
                0,
            )
            .unwrap();
        assert!(!result.is_approved());
        assert!(result.reason().unwrap().contains("Projected position"));
    }

    // ---- Parsing helper tests ----

    #[test]
    fn test_parse_direction_valid() {
        assert!(matches!(parse_direction("LONG").unwrap(), Direction::Long));
        assert!(matches!(parse_direction("BUY").unwrap(), Direction::Long));
        assert!(matches!(
            parse_direction("SHORT").unwrap(),
            Direction::Short
        ));
        assert!(matches!(parse_direction("SELL").unwrap(), Direction::Short));
        assert!(matches!(parse_direction("NET").unwrap(), Direction::Net));
    }

    #[test]
    fn test_parse_direction_invalid() {
        assert!(parse_direction("INVALID").is_err());
    }

    #[test]
    fn test_parse_offset_valid() {
        assert!(matches!(parse_offset("NONE").unwrap(), Offset::None));
        assert!(matches!(parse_offset("OPEN").unwrap(), Offset::Open));
        assert!(matches!(parse_offset("CLOSE").unwrap(), Offset::Close));
        assert!(matches!(
            parse_offset("CLOSE_TODAY").unwrap(),
            Offset::CloseToday
        ));
        assert!(matches!(
            parse_offset("CLOSE_YESTERDAY").unwrap(),
            Offset::CloseYesterday
        ));
    }

    #[test]
    fn test_parse_offset_invalid() {
        assert!(parse_offset("INVALID").is_err());
    }

    #[test]
    fn test_parse_order_type_valid() {
        assert!(matches!(
            parse_order_type("MARKET").unwrap(),
            OrderType::Market
        ));
        assert!(matches!(
            parse_order_type("LIMIT").unwrap(),
            OrderType::Limit
        ));
        assert!(matches!(parse_order_type("STOP").unwrap(), OrderType::Stop));
    }

    #[test]
    fn test_parse_order_type_invalid() {
        assert!(parse_order_type("INVALID").is_err());
    }

    #[test]
    fn test_parse_vt_symbol() {
        let (sym, exch) = parse_vt_symbol("BTCUSDT.BINANCE");
        assert_eq!(sym, "BTCUSDT");
        assert_eq!(exch, Exchange::Binance);

        let (sym, exch) = parse_vt_symbol("ETHUSDT.BINANCE_USDM");
        assert_eq!(sym, "ETHUSDT");
        assert_eq!(exch, Exchange::BinanceUsdm);

        let (sym, exch) = parse_vt_symbol("RAW_SYMBOL");
        assert_eq!(sym, "RAW_SYMBOL");
        assert_eq!(exch, Exchange::Local);
    }

    // ---- Repr tests ----

    #[test]
    fn test_risk_config_repr() {
        let config = PyRiskConfig::new();
        let repr = config.__repr__();
        assert!(repr.starts_with("RiskConfig("));
        assert!(repr.contains("max_order_size=1000"));
    }

    #[test]
    fn test_check_result_repr_approved() {
        let result = PyRiskCheckResult::from_inner(RiskCheckResult::approved());
        let repr = result.__repr__();
        assert_eq!(repr, "RiskCheckResult(is_approved=True)");
    }

    #[test]
    fn test_check_result_repr_rejected() {
        let result = PyRiskCheckResult::from_inner(RiskCheckResult::rejected("test reason"));
        let repr = result.__repr__();
        assert!(repr.contains("is_approved=False"));
        assert!(repr.contains("test reason"));
    }

    // ---- build_position tests ----

    #[test]
    fn test_build_position_zero() {
        let pos = build_position("BTCUSDT", Exchange::Binance, 0.0);
        assert!(pos.is_flat());
    }

    #[test]
    fn test_build_position_long() {
        let pos = build_position("BTCUSDT", Exchange::Binance, 100.0);
        assert!(pos.is_long());
        assert!((pos.signed_qty() - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_build_position_short() {
        let pos = build_position("BTCUSDT", Exchange::Binance, -50.0);
        assert!(pos.is_short());
        assert!((pos.signed_qty() - (-50.0)).abs() < 1e-10);
    }
}
