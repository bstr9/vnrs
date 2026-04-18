//! Pre-trade Risk Engine
//!
//! Validates orders before they are sent to the matching engine.
//! Based on nautilus_trader's risk management approach.

use crate::backtesting::position::Position;
use crate::trader::{Direction, Offset, OrderData};
use std::collections::HashMap;

/// Result of a risk check
#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    pub is_approved: bool,
    pub reason: Option<String>,
}

impl RiskCheckResult {
    pub fn approved() -> Self {
        Self {
            is_approved: true,
            reason: None,
        }
    }

    pub fn rejected(reason: &str) -> Self {
        Self {
            is_approved: false,
            reason: Some(reason.to_string()),
        }
    }
}

/// Risk engine configuration
#[derive(Debug, Clone)]
pub struct RiskConfig {
    /// Maximum order quantity per order
    pub max_order_size: f64,
    /// Maximum position size (absolute value)
    pub max_position_size: f64,
    /// Maximum notional value per order (price * volume * size)
    pub max_notional_per_order: f64,
    /// Maximum number of active orders
    pub max_open_orders: usize,
    /// Maximum daily trades
    pub max_daily_trades: u64,
    /// Maximum daily turnover
    pub max_daily_turnover: f64,
    /// Enable/disable specific checks
    pub check_order_size: bool,
    pub check_position_size: bool,
    pub check_notional: bool,
    pub check_open_orders: bool,
    pub check_daily_trades: bool,
    pub check_daily_turnover: bool,
    /// Portfolio-level risk limits
    /// Max total exposure / capital (e.g., 2.0 = 200%)
    pub max_portfolio_exposure: f64,
    /// Max leverage ratio
    pub max_leverage: f64,
    /// Max drawdown percentage before circuit breaker (0.0..1.0)
    pub max_drawdown_pct: f64,
    /// Max single position notional / total portfolio notional (0.0..1.0)
    pub max_position_concentration: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_order_size: 1000.0,
            max_position_size: 5000.0,
            max_notional_per_order: 1_000_000.0,
            max_open_orders: 50,
            max_daily_trades: 500,
            max_daily_turnover: 50_000_000.0,
            check_order_size: true,
            check_position_size: true,
            check_notional: true,
            check_open_orders: true,
            check_daily_trades: true,
            check_daily_turnover: true,
            max_portfolio_exposure: f64::MAX,
            max_leverage: f64::MAX,
            max_drawdown_pct: 1.0,
            max_position_concentration: 1.0,
        }
    }
}

/// Pre-trade risk engine
pub struct RiskEngine {
    config: RiskConfig,
    daily_trade_count: u64,
    daily_turnover: f64,
    // Portfolio-level tracking
    peak_equity: f64,
    current_equity: f64,
    is_halted: bool,
    position_notional: HashMap<String, f64>,
    total_notional: f64,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            daily_trade_count: 0,
            daily_turnover: 0.0,
            peak_equity: 0.0,
            current_equity: 0.0,
            is_halted: false,
            position_notional: HashMap::new(),
            total_notional: 0.0,
        }
    }

    /// Create a risk engine with all checks disabled (for backward compat)
    pub fn new_unrestricted() -> Self {
        Self {
            config: RiskConfig {
                check_order_size: false,
                check_position_size: false,
                check_notional: false,
                check_open_orders: false,
                check_daily_trades: false,
                check_daily_turnover: false,
                ..Default::default()
            },
            daily_trade_count: 0,
            daily_turnover: 0.0,
            peak_equity: 0.0,
            current_equity: 0.0,
            is_halted: false,
            position_notional: HashMap::new(),
            total_notional: 0.0,
        }
    }

    /// Check if an order passes all risk checks
    pub fn check_order(
        &self,
        order: &OrderData,
        position: &Position,
        active_order_count: usize,
        size_multiplier: f64,
    ) -> RiskCheckResult {
        // Check order size
        if self.config.check_order_size && order.volume > self.config.max_order_size {
            return RiskCheckResult::rejected(&format!(
                "Order size {} exceeds max {}",
                order.volume, self.config.max_order_size
            ));
        }

        // Check position size (projected after fill)
        if self.config.check_position_size {
            let projected = self.projected_position(order, position);
            if projected.abs() > self.config.max_position_size {
                return RiskCheckResult::rejected(&format!(
                    "Projected position size {} would exceed max {}",
                    projected.abs(),
                    self.config.max_position_size
                ));
            }
        }

        // Check notional
        if self.config.check_notional {
            let notional = order.price * order.volume * size_multiplier;
            if notional > self.config.max_notional_per_order {
                return RiskCheckResult::rejected(&format!(
                    "Order notional {} exceeds max {}",
                    notional, self.config.max_notional_per_order
                ));
            }
        }

        // Check open orders
        if self.config.check_open_orders && active_order_count >= self.config.max_open_orders {
            return RiskCheckResult::rejected(&format!(
                "Open orders {} exceeds max {}",
                active_order_count, self.config.max_open_orders
            ));
        }

        // Check daily trades
        if self.config.check_daily_trades && self.daily_trade_count >= self.config.max_daily_trades
        {
            return RiskCheckResult::rejected(&format!(
                "Daily trade count {} exceeds max {}",
                self.daily_trade_count, self.config.max_daily_trades
            ));
        }

        // Check daily turnover
        if self.config.check_daily_turnover && self.daily_turnover >= self.config.max_daily_turnover
        {
            return RiskCheckResult::rejected(&format!(
                "Daily turnover {} exceeds max {}",
                self.daily_turnover, self.config.max_daily_turnover
            ));
        }

        RiskCheckResult::approved()
    }

    /// Record a completed trade for daily tracking
    pub fn record_trade(&mut self, trade_value: f64) {
        self.daily_trade_count += 1;
        self.daily_turnover += trade_value;
    }

    /// Reset daily counters (call at start of new trading day)
    pub fn reset_daily(&mut self) {
        self.daily_trade_count = 0;
        self.daily_turnover = 0.0;
    }

    /// Calculate projected position after order fill
    fn projected_position(&self, order: &OrderData, position: &Position) -> f64 {
        let current = position.signed_qty();
        let delta = match order.direction {
            Some(Direction::Long) => match order.offset {
                Offset::Open => order.volume,
                Offset::Close | Offset::CloseToday | Offset::CloseYesterday => -order.volume,
                Offset::None => order.volume,
            },
            Some(Direction::Short) => match order.offset {
                Offset::Open => -order.volume,
                Offset::Close | Offset::CloseToday | Offset::CloseYesterday => order.volume,
                Offset::None => -order.volume,
            },
            Some(Direction::Net) => 0.0,
            None => 0.0,
        };
        current + delta
    }

    /// Get risk config reference
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    /// Update risk config
    pub fn set_config(&mut self, config: RiskConfig) {
        self.config = config;
    }

    // ── Portfolio-level risk checks ──────────────────────────────────

    /// Check if adding additional notional exposure would exceed portfolio limits
    pub fn check_portfolio_exposure(
        &self,
        additional_notional: f64,
        capital: f64,
    ) -> RiskCheckResult {
        if capital <= 0.0 {
            return RiskCheckResult::approved();
        }
        let projected_total = self.total_notional + additional_notional;
        let exposure_ratio = projected_total / capital;
        if exposure_ratio > self.config.max_portfolio_exposure {
            return RiskCheckResult::rejected(&format!(
                "Portfolio exposure {:.2}% would exceed max {:.2}%",
                exposure_ratio * 100.0,
                self.config.max_portfolio_exposure * 100.0,
            ));
        }
        let leverage = projected_total / capital;
        if leverage > self.config.max_leverage {
            return RiskCheckResult::rejected(&format!(
                "Leverage {:.2}x would exceed max {:.2}x",
                leverage, self.config.max_leverage,
            ));
        }
        RiskCheckResult::approved()
    }

    /// Check if a single position would exceed concentration limits
    pub fn check_position_concentration(
        &self,
        symbol: &str,
        additional_notional: f64,
        capital: f64,
    ) -> RiskCheckResult {
        if capital <= 0.0 {
            return RiskCheckResult::approved();
        }
        let current = self.position_notional.get(symbol).copied().unwrap_or(0.0);
        let projected = current + additional_notional;
        let concentration = projected / capital;
        if concentration > self.config.max_position_concentration {
            return RiskCheckResult::rejected(&format!(
                "Position concentration for {} is {:.2}% which exceeds max {:.2}%",
                symbol,
                concentration * 100.0,
                self.config.max_position_concentration * 100.0,
            ));
        }
        RiskCheckResult::approved()
    }

    /// Update equity tracking and check drawdown circuit breaker.
    /// Returns rejected if drawdown exceeds threshold (circuit breaker triggered).
    pub fn update_equity(&mut self, equity: f64) -> RiskCheckResult {
        self.current_equity = equity;
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
        if self.peak_equity <= 0.0 {
            return RiskCheckResult::approved();
        }
        let drawdown = (self.peak_equity - equity) / self.peak_equity;
        if drawdown >= self.config.max_drawdown_pct {
            self.is_halted = true;
            return RiskCheckResult::rejected(&format!(
                "Drawdown {:.2}% exceeds max {:.2}% — circuit breaker triggered",
                drawdown * 100.0,
                self.config.max_drawdown_pct * 100.0,
            ));
        }
        RiskCheckResult::approved()
    }

    /// Update the notional value tracked for a given symbol
    pub fn update_position_notional(&mut self, symbol: &str, notional: f64) {
        let old = self.position_notional.get(symbol).copied().unwrap_or(0.0);
        self.total_notional = self.total_notional - old + notional;
        if notional == 0.0 {
            self.position_notional.remove(symbol);
        } else {
            self.position_notional.insert(symbol.to_string(), notional);
        }
    }

    /// Returns true if the circuit breaker has been triggered
    pub fn is_halted(&self) -> bool {
        self.is_halted
    }
}
