//! Risk Manager engine for position sizing, daily loss limits, and exposure control.
//!
//! Provides pre-trade risk checks before orders are sent to the exchange.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use chrono::Utc;
use tracing::{info, warn};

use super::constant::Direction;
use super::engine::BaseEngine;
use super::object::{AccountData, OrderRequest, PositionData, TradeData};

/// Risk check result
#[derive(Debug, Clone)]
pub enum RiskCheckResult {
    /// Order passes all risk checks
    Approved,
    /// Order is rejected with a reason
    Rejected(String),
}

/// Risk management configuration
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum active order count per symbol (0 = unlimited)
    pub max_order_count: usize,
    /// Maximum active order count across all symbols (0 = unlimited)
    pub max_total_order_count: usize,
    /// Maximum order volume per order (0 = unlimited)
    pub max_order_volume: f64,
    /// Maximum order notional value per order (0 = unlimited)
    pub max_order_notional: f64,
    /// Maximum daily trade count (0 = unlimited)
    pub max_daily_trades: usize,
    /// Maximum daily turnover in base currency (0 = unlimited)
    pub max_daily_turnover: f64,
    /// Maximum position per symbol (0 = unlimited)
    pub max_position_per_symbol: f64,
    /// Maximum total position across all symbols (0 = unlimited)
    pub max_total_position: f64,
    /// Maximum daily loss in base currency (0 = unlimited)
    pub max_daily_loss: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_order_count: 50,
            max_total_order_count: 500,
            max_order_volume: 0.0,
            max_order_notional: 0.0,
            max_daily_trades: 0,
            max_daily_turnover: 0.0,
            max_position_per_symbol: 0.0,
            max_total_position: 0.0,
            max_daily_loss: 0.0,
        }
    }
}

impl RiskConfig {
    /// Create an unrestricted risk config (no limits)
    pub fn unrestricted() -> Self {
        Self {
            max_order_count: 0,
            max_total_order_count: 0,
            max_order_volume: 0.0,
            max_order_notional: 0.0,
            max_daily_trades: 0,
            max_daily_turnover: 0.0,
            max_position_per_symbol: 0.0,
            max_total_position: 0.0,
            max_daily_loss: 0.0,
        }
    }
}

/// Daily risk statistics
#[derive(Debug, Clone, Default)]
pub struct DailyStats {
    /// Number of trades today
    pub trade_count: usize,
    /// Total turnover today
    pub turnover: f64,
    /// Total realized PnL today
    pub realized_pnl: f64,
    /// Date string (YYYY-MM-DD) for reset tracking
    pub date: String,
}

/// RiskManager engine
///
/// Pre-trade risk management that checks orders before they are sent to the exchange.
/// Tracks daily statistics and enforces configurable limits.
pub struct RiskManager {
    /// Engine name
    name: String,
    /// Risk configuration
    config: RwLock<RiskConfig>,
    /// Daily statistics
    daily_stats: RwLock<DailyStats>,
    /// Active order count per vt_symbol
    active_orders: RwLock<HashMap<String, usize>>,
    /// Total active order count
    total_active_orders: RwLock<usize>,
    /// Current positions per vt_symbol
    positions: RwLock<HashMap<String, f64>>,
    /// Running flag
    running: AtomicBool,
    /// Whether risk manager is enabled
    enabled: AtomicBool,
}

impl RiskManager {
    /// Create a new RiskManager with default configuration
    pub fn new() -> Self {
        Self::with_config(RiskConfig::default())
    }

    /// Create a new RiskManager with custom configuration
    pub fn with_config(config: RiskConfig) -> Self {
        Self {
            name: "RiskManager".to_string(),
            config: RwLock::new(config),
            daily_stats: RwLock::new(DailyStats::default()),
            active_orders: RwLock::new(HashMap::new()),
            total_active_orders: RwLock::new(0),
            positions: RwLock::new(HashMap::new()),
            running: AtomicBool::new(false),
            enabled: AtomicBool::new(true),
        }
    }

    /// Enable risk management
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
        info!("[RiskManager] Risk management enabled");
    }

    /// Disable risk management (all orders pass)
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        warn!("[RiskManager] Risk management DISABLED - all orders will pass");
    }

    /// Check if risk management is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Update risk configuration
    pub fn update_config(&self, config: RiskConfig) {
        let mut current = self.config.write().unwrap();
        *current = config;
        info!("[RiskManager] Configuration updated");
    }

    /// Get current risk configuration
    pub fn get_config(&self) -> RiskConfig {
        self.config.read().unwrap().clone()
    }

    /// Check an order request against all risk rules
    ///
    /// Returns Approved if the order passes all checks, or Rejected with a reason.
    pub fn check_order(&self, req: &OrderRequest) -> RiskCheckResult {
        if !self.enabled.load(Ordering::SeqCst) {
            return RiskCheckResult::Approved;
        }

        let config = self.config.read().unwrap();
        let vt_symbol = format!("{}.{}", req.symbol, req.exchange.value());

        // 1. Check order volume
        if config.max_order_volume > 0.0 && req.volume > config.max_order_volume {
            return RiskCheckResult::Rejected(format!(
                "Order volume {} exceeds max {} for {}",
                req.volume, config.max_order_volume, vt_symbol
            ));
        }

        // 2. Check order notional (volume * price)
        if config.max_order_notional > 0.0 && req.price > 0.0 {
            let notional = req.volume * req.price;
            if notional > config.max_order_notional {
                return RiskCheckResult::Rejected(format!(
                    "Order notional {:.2} exceeds max {:.2} for {}",
                    notional, config.max_order_notional, vt_symbol
                ));
            }
        }

        // 3. Check active order count per symbol
        if config.max_order_count > 0 {
            let active = self.active_orders.read().unwrap();
            let count = active.get(&vt_symbol).copied().unwrap_or(0);
            if count >= config.max_order_count {
                return RiskCheckResult::Rejected(format!(
                    "Active order count {} >= max {} for {}",
                    count, config.max_order_count, vt_symbol
                ));
            }
        }

        // 4. Check total active order count
        if config.max_total_order_count > 0 {
            let total = *self.total_active_orders.read().unwrap();
            if total >= config.max_total_order_count {
                return RiskCheckResult::Rejected(format!(
                    "Total active orders {} >= max {}",
                    total, config.max_total_order_count
                ));
            }
        }

        // 5. Check daily trade count
        self.check_daily_reset();
        if config.max_daily_trades > 0 {
            let stats = self.daily_stats.read().unwrap();
            if stats.trade_count >= config.max_daily_trades {
                return RiskCheckResult::Rejected(format!(
                    "Daily trade count {} >= max {}",
                    stats.trade_count, config.max_daily_trades
                ));
            }
        }

        // 6. Check daily turnover
        if config.max_daily_turnover > 0.0 {
            let stats = self.daily_stats.read().unwrap();
            if stats.turnover >= config.max_daily_turnover {
                return RiskCheckResult::Rejected(format!(
                    "Daily turnover {:.2} >= max {:.2}",
                    stats.turnover, config.max_daily_turnover
                ));
            }
        }

        // 7. Check daily loss
        if config.max_daily_loss > 0.0 {
            let stats = self.daily_stats.read().unwrap();
            if stats.realized_pnl < 0.0 && stats.realized_pnl.abs() >= config.max_daily_loss {
                return RiskCheckResult::Rejected(format!(
                    "Daily loss {:.2} >= max loss {:.2}",
                    stats.realized_pnl.abs(), config.max_daily_loss
                ));
            }
        }

        // 8. Check position per symbol
        if config.max_position_per_symbol > 0.0 {
            let positions = self.positions.read().unwrap();
            let current = positions.get(&vt_symbol).copied().unwrap_or(0.0);
            let new_pos = match req.direction {
                Direction::Long | Direction::Net => current + req.volume,
                Direction::Short => (current - req.volume).abs(),
            };
            if new_pos > config.max_position_per_symbol {
                return RiskCheckResult::Rejected(format!(
                    "Position {:.4} would exceed max {} for {}",
                    new_pos, config.max_position_per_symbol, vt_symbol
                ));
            }
        }

        // 9. Check total position
        if config.max_total_position > 0.0 {
            let positions = self.positions.read().unwrap();
            let total: f64 = positions.values().sum();
            if total + req.volume > config.max_total_position {
                return RiskCheckResult::Rejected(format!(
                    "Total position {:.4} would exceed max {}",
                    total + req.volume, config.max_total_position
                ));
            }
        }

        RiskCheckResult::Approved
    }

    /// Record a trade for daily statistics
    pub fn record_trade(&self, trade: &TradeData) {
        self.check_daily_reset();

        let mut stats = self.daily_stats.write().unwrap();
        stats.trade_count += 1;

        if trade.direction.is_some() {
            stats.turnover += trade.volume * trade.price;
        }
    }

    /// Update position from position data
    pub fn update_position(&self, position: &PositionData) {
        let vt_symbol = position.vt_symbol();
        let mut positions = self.positions.write().unwrap();
        positions.insert(vt_symbol, position.volume);
    }

    /// Update account data (for daily PnL tracking)
    pub fn update_account(&self, _account: &AccountData) {
        // Future: track balance changes for daily PnL
    }

    /// Increment active order count for a symbol
    pub fn order_submitted(&self, vt_symbol: &str) {
        {
            let mut active = self.active_orders.write().unwrap();
            *active.entry(vt_symbol.to_string()).or_insert(0) += 1;
        }
        {
            let mut total = self.total_active_orders.write().unwrap();
            *total += 1;
        }
    }

    /// Decrement active order count for a symbol
    pub fn order_completed(&self, vt_symbol: &str) {
        {
            let mut active = self.active_orders.write().unwrap();
            if let Some(count) = active.get_mut(vt_symbol) {
                *count = count.saturating_sub(1);
            }
        }
        {
            let mut total = self.total_active_orders.write().unwrap();
            *total = total.saturating_sub(1);
        }
    }

    /// Check if daily stats need to be reset (new day)
    fn check_daily_reset(&self) {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let mut stats = self.daily_stats.write().unwrap();
        if stats.date != today {
            if !stats.date.is_empty() {
                info!(
                    "[RiskManager] Daily reset: trades={}, turnover={:.2}, pnl={:.2}",
                    stats.trade_count, stats.turnover, stats.realized_pnl
                );
            }
            *stats = DailyStats {
                date: today,
                ..Default::default()
            };
        }
    }

    /// Get daily statistics
    pub fn get_daily_stats(&self) -> DailyStats {
        self.check_daily_reset();
        self.daily_stats.read().unwrap().clone()
    }
}

impl Default for RiskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for RiskManager {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::constant::{Exchange, Offset};

    fn make_order(symbol: &str, exchange: Exchange, direction: Direction, price: f64, volume: f64) -> OrderRequest {
        OrderRequest {
            symbol: symbol.to_string(),
            exchange,
            direction,
            order_type: crate::trader::constant::OrderType::Limit,
            offset: Offset::None,
            price,
            volume,
            reference: String::new(),
        }
    }

    #[test]
    fn test_risk_manager_approved() {
        let rm = RiskManager::new();
        let req = make_order("btcusdt", Exchange::Binance, Direction::Long, 50000.0, 0.01);
        let result = rm.check_order(&req);
        assert!(matches!(result, RiskCheckResult::Approved));
    }

    #[test]
    fn test_risk_manager_volume_limit() {
        let mut config = RiskConfig::default();
        config.max_order_volume = 0.005;
        let rm = RiskManager::with_config(config);

        let req = make_order("btcusdt", Exchange::Binance, Direction::Long, 50000.0, 0.01);
        let result = rm.check_order(&req);
        assert!(matches!(result, RiskCheckResult::Rejected(_)));
    }

    #[test]
    fn test_risk_manager_notional_limit() {
        let mut config = RiskConfig::default();
        config.max_order_notional = 100.0;
        let rm = RiskManager::with_config(config);

        let req = make_order("btcusdt", Exchange::Binance, Direction::Long, 50000.0, 0.01);
        // notional = 50000 * 0.01 = 500 > 100
        let result = rm.check_order(&req);
        assert!(matches!(result, RiskCheckResult::Rejected(_)));
    }

    #[test]
    fn test_risk_manager_disabled() {
        let mut config = RiskConfig::default();
        config.max_order_volume = 0.001;
        let rm = RiskManager::with_config(config);
        rm.disable();

        let req = make_order("btcusdt", Exchange::Binance, Direction::Long, 50000.0, 0.01);
        let result = rm.check_order(&req);
        assert!(matches!(result, RiskCheckResult::Approved));
    }

    #[test]
    fn test_risk_manager_unrestricted() {
        let rm = RiskManager::with_config(RiskConfig::unrestricted());
        let req = make_order("btcusdt", Exchange::Binance, Direction::Long, 50000.0, 100.0);
        let result = rm.check_order(&req);
        assert!(matches!(result, RiskCheckResult::Approved));
    }

    #[test]
    fn test_risk_manager_daily_trade_limit() {
        let mut config = RiskConfig::default();
        config.max_daily_trades = 2;
        let rm = RiskManager::with_config(config);

        // Record 2 trades
        let trade = TradeData {
            symbol: "btcusdt".to_string(),
            exchange: Exchange::Binance,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Long),
            offset: Offset::None,
            price: 50000.0,
            volume: 0.01,
            datetime: Some(Utc::now()),
            gateway_name: "BINANCE_SPOT".to_string(),
            extra: None,
        };
        rm.record_trade(&trade);
        rm.record_trade(&trade);

        // Next order should be rejected
        let req = make_order("btcusdt", Exchange::Binance, Direction::Long, 50000.0, 0.01);
        let result = rm.check_order(&req);
        assert!(matches!(result, RiskCheckResult::Rejected(_)));
    }
}
