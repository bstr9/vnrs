//! Pre-trade Risk Engine
//!
//! Validates orders before they are sent to the matching engine.
//! Based on nautilus_trader's risk management approach.

use crate::backtesting::position::Position;
use crate::trader::{Direction, Offset, OrderData};

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
        }
    }
}

/// Pre-trade risk engine
pub struct RiskEngine {
    config: RiskConfig,
    daily_trade_count: u64,
    daily_turnover: f64,
}

impl RiskEngine {
    pub fn new(config: RiskConfig) -> Self {
        Self {
            config,
            daily_trade_count: 0,
            daily_turnover: 0.0,
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
}
