//! Portfolio manager for tracking positions, PnL, and portfolio-level metrics.
//!
//! Provides:
//! - Real-time position tracking across symbols and gateways
//! - Realized and unrealized PnL calculation
//! - Portfolio summary with exposure metrics
//! - Event-driven updates via BaseEngine trait

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

use super::constant::{Direction, Exchange};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{AccountData, PositionData, TickData, TradeData};

/// Summary of a single position
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionSummary {
    /// Symbol
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Direction (Long/Short/Net)
    pub direction: Direction,
    /// Total volume held
    pub volume: f64,
    /// Average entry price
    pub avg_price: f64,
    /// Current market price (last known)
    pub market_price: f64,
    /// Unrealized PnL
    pub unrealized_pnl: f64,
    /// Realized PnL for this position
    pub realized_pnl: f64,
    /// Gateway name
    pub gateway_name: String,
    /// Last updated time
    pub updated_at: Option<DateTime<Utc>>,
}

impl PositionSummary {
    /// Create a new PositionSummary from PositionData
    pub fn from_position(pos: &PositionData) -> Self {
        Self {
            symbol: pos.symbol.clone(),
            exchange: pos.exchange,
            direction: pos.direction,
            volume: pos.volume,
            avg_price: pos.price,
            market_price: pos.price,
            unrealized_pnl: pos.pnl,
            realized_pnl: 0.0,
            gateway_name: pos.gateway_name.clone(),
            updated_at: Some(Utc::now()),
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Get position key (gateway_name.vt_symbol.direction)
    pub fn position_key(&self) -> String {
        format!("{}.{}.{}", self.gateway_name, self.vt_symbol(), self.direction)
    }

    /// Calculate notional value
    pub fn notional_value(&self) -> f64 {
        self.volume * self.market_price
    }
}

/// Portfolio-level summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioSummary {
    /// Total portfolio value (sum of account balances)
    pub total_value: f64,
    /// Total unrealized PnL across all positions
    pub total_unrealized_pnl: f64,
    /// Total realized PnL today
    pub daily_realized_pnl: f64,
    /// Total realized PnL all-time
    pub total_realized_pnl: f64,
    /// Number of open positions
    pub positions_count: usize,
    /// Exposure by symbol (notional value)
    pub exposure_by_symbol: HashMap<String, f64>,
    /// Exposure by gateway (notional value)
    pub exposure_by_gateway: HashMap<String, f64>,
    /// Total exposure (sum of all position notionals)
    pub total_exposure: f64,
    /// Timestamp of last update
    pub updated_at: Option<DateTime<Utc>>,
}

/// Portfolio metrics for risk analysis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PortfolioMetrics {
    /// Number of winning trades
    pub winning_trades: usize,
    /// Number of losing trades
    pub losing_trades: usize,
    /// Total realized PnL from winning trades
    pub total_wins: f64,
    /// Total realized PnL from losing trades
    pub total_losses: f64,
    /// Win rate (0.0 to 1.0)
    pub win_rate: f64,
    /// Profit factor (total_wins / abs(total_losses))
    pub profit_factor: f64,
    /// Largest single trade loss
    pub largest_loss: f64,
    /// Largest single trade win
    pub largest_win: f64,
}

impl PortfolioMetrics {
    /// Calculate win rate
    pub fn calculate_win_rate(&mut self) {
        let total = self.winning_trades + self.losing_trades;
        self.win_rate = if total > 0 {
            self.winning_trades as f64 / total as f64
        } else {
            0.0
        };
    }

    /// Calculate profit factor
    pub fn calculate_profit_factor(&mut self) {
        self.profit_factor = if self.total_losses.abs() > 0.0 {
            self.total_wins / self.total_losses.abs()
        } else if self.total_wins > 0.0 {
            f64::INFINITY
        } else {
            0.0
        };
    }
}

/// Portfolio manager engine
///
/// Tracks positions, calculates PnL, and provides portfolio-level metrics.
/// Integrates with the event system via BaseEngine trait.
pub struct PortfolioManager {
    /// Engine name
    name: String,
    /// Position summaries by position key
    positions: RwLock<HashMap<String, PositionSummary>>,
    /// Account data by vt_accountid
    accounts: RwLock<HashMap<String, AccountData>>,
    /// Daily realized PnL
    daily_realized_pnl: RwLock<f64>,
    /// Total realized PnL (all-time)
    total_realized_pnl: RwLock<f64>,
    /// Portfolio metrics
    metrics: RwLock<PortfolioMetrics>,
    /// Running flag
    running: AtomicBool,
}

impl PortfolioManager {
    /// Create a new PortfolioManager
    pub fn new() -> Self {
        Self {
            name: "PortfolioManager".to_string(),
            positions: RwLock::new(HashMap::new()),
            accounts: RwLock::new(HashMap::new()),
            daily_realized_pnl: RwLock::new(0.0),
            total_realized_pnl: RwLock::new(0.0),
            metrics: RwLock::new(PortfolioMetrics::default()),
            running: AtomicBool::new(false),
        }
    }

    /// Get a specific position summary by position key
    pub fn get_position(&self, position_key: &str) -> Option<PositionSummary> {
        let positions = self.positions.read().unwrap_or_else(|e| e.into_inner());
        positions.get(position_key).cloned()
    }

    /// Get all position summaries
    pub fn get_all_positions(&self) -> Vec<PositionSummary> {
        let positions = self.positions.read().unwrap_or_else(|e| e.into_inner());
        positions.values().cloned().collect()
    }

    /// Get only non-empty positions (volume > 0)
    pub fn get_active_positions(&self) -> Vec<PositionSummary> {
        let positions = self.positions.read().unwrap_or_else(|e| e.into_inner());
        positions.values().filter(|p| p.volume > 0.0).cloned().collect()
    }

    /// Get account data by vt_accountid
    pub fn get_account(&self, vt_accountid: &str) -> Option<AccountData> {
        let accounts = self.accounts.read().unwrap_or_else(|e| e.into_inner());
        accounts.get(vt_accountid).cloned()
    }

    /// Get all accounts
    pub fn get_all_accounts(&self) -> Vec<AccountData> {
        let accounts = self.accounts.read().unwrap_or_else(|e| e.into_inner());
        accounts.values().cloned().collect()
    }

    /// Get daily realized PnL
    pub fn get_daily_realized_pnl(&self) -> f64 {
        *self.daily_realized_pnl.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Get total realized PnL
    pub fn get_total_realized_pnl(&self) -> f64 {
        *self.total_realized_pnl.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Reset daily PnL (call at start of new trading day)
    pub fn reset_daily_pnl(&self) {
        let mut daily = self.daily_realized_pnl.write().unwrap_or_else(|e| e.into_inner());
        info!("[PortfolioManager] Daily PnL reset (was: {:.2})", *daily);
        *daily = 0.0;
    }

    /// Calculate unrealized PnL for a specific position
    pub fn calculate_unrealized_pnl(&self, position_key: &str, market_price: f64) -> f64 {
        let positions = self.positions.read().unwrap_or_else(|e| e.into_inner());
        if let Some(pos) = positions.get(position_key) {
            match pos.direction {
                Direction::Long | Direction::Net => {
                    (market_price - pos.avg_price) * pos.volume
                }
                Direction::Short => {
                    (pos.avg_price - market_price) * pos.volume
                }
            }
        } else {
            0.0
        }
    }

    /// Update market price for a position (called on tick/bar events)
    pub fn update_position_price(&self, symbol: &str, exchange: Exchange, market_price: f64) {
        let mut positions = self.positions.write().unwrap_or_else(|e| e.into_inner());
        for pos in positions.values_mut() {
            if pos.symbol == symbol && pos.exchange == exchange && pos.volume > 0.0 {
                pos.market_price = market_price;
                pos.unrealized_pnl = match pos.direction {
                    Direction::Long | Direction::Net => {
                        (market_price - pos.avg_price) * pos.volume
                    }
                    Direction::Short => {
                        (pos.avg_price - market_price) * pos.volume
                    }
                };
                pos.updated_at = Some(Utc::now());
            }
        }
    }

    /// Get portfolio summary
    pub fn get_portfolio_summary(&self) -> PortfolioSummary {
        let positions = self.positions.read().unwrap_or_else(|e| e.into_inner());
        let accounts = self.accounts.read().unwrap_or_else(|e| e.into_inner());
        let daily_pnl = *self.daily_realized_pnl.read().unwrap_or_else(|e| e.into_inner());
        let total_pnl = *self.total_realized_pnl.read().unwrap_or_else(|e| e.into_inner());

        let total_unrealized_pnl: f64 = positions.values()
            .map(|p| p.unrealized_pnl)
            .sum();

        let total_value: f64 = accounts.values()
            .map(|a| a.balance)
            .sum();

        let mut exposure_by_symbol: HashMap<String, f64> = HashMap::new();
        let mut exposure_by_gateway: HashMap<String, f64> = HashMap::new();

        for pos in positions.values() {
            if pos.volume > 0.0 {
                let notional = pos.notional_value();
                *exposure_by_symbol.entry(pos.vt_symbol()).or_insert(0.0) += notional;
                *exposure_by_gateway.entry(pos.gateway_name.clone()).or_insert(0.0) += notional;
            }
        }

        let total_exposure: f64 = exposure_by_symbol.values().sum();
        let positions_count = positions.values().filter(|p| p.volume > 0.0).count();

        PortfolioSummary {
            total_value,
            total_unrealized_pnl,
            daily_realized_pnl: daily_pnl,
            total_realized_pnl: total_pnl,
            positions_count,
            exposure_by_symbol,
            exposure_by_gateway,
            total_exposure,
            updated_at: Some(Utc::now()),
        }
    }

    /// Get portfolio metrics
    pub fn get_metrics(&self) -> PortfolioMetrics {
        let metrics = self.metrics.read().unwrap_or_else(|e| e.into_inner());
        metrics.clone()
    }

    /// Process a position event from gateway
    fn process_position_event(&self, position: &PositionData) {
        let key = position.vt_positionid();
        let mut positions = self.positions.write().unwrap_or_else(|e| e.into_inner());

        if position.volume > 0.0 {
            // Update or create position
            let summary = positions.entry(key).or_insert_with(|| {
                PositionSummary {
                    symbol: position.symbol.clone(),
                    exchange: position.exchange,
                    direction: position.direction,
                    volume: 0.0,
                    avg_price: 0.0,
                    market_price: position.price,
                    unrealized_pnl: 0.0,
                    realized_pnl: 0.0,
                    gateway_name: position.gateway_name.clone(),
                    updated_at: None,
                }
            });

            summary.volume = position.volume;
            summary.avg_price = position.price;
            summary.unrealized_pnl = position.pnl;
            summary.updated_at = Some(Utc::now());
        } else {
            // Position closed — remove it
            if let Some(removed) = positions.remove(&key) {
                info!(
                    "[PortfolioManager] Position closed: {} {} (realized: {:.2})",
                    removed.vt_symbol(), removed.direction, removed.realized_pnl
                );
            }
        }
    }

    /// Process a trade event from gateway
    fn process_trade_event(&self, trade: &TradeData) {
        let pnl = if trade.direction == Some(Direction::Long) || trade.direction == Some(Direction::Net) {
            // For buys, we can't determine PnL directly from a single trade
            // PnL is realized when closing a position
            0.0
        } else if trade.direction == Some(Direction::Short) {
            0.0
        } else {
            0.0
        };

        // Update daily/total PnL tracking
        if pnl != 0.0 {
            let mut daily = self.daily_realized_pnl.write().unwrap_or_else(|e| e.into_inner());
            *daily += pnl;
            let mut total = self.total_realized_pnl.write().unwrap_or_else(|e| e.into_inner());
            *total += pnl;

            // Update metrics
            let mut metrics = self.metrics.write().unwrap_or_else(|e| e.into_inner());
            if pnl > 0.0 {
                metrics.winning_trades += 1;
                metrics.total_wins += pnl;
                if pnl > metrics.largest_win {
                    metrics.largest_win = pnl;
                }
            } else {
                metrics.losing_trades += 1;
                metrics.total_losses += pnl;
                if pnl < metrics.largest_loss {
                    metrics.largest_loss = pnl;
                }
            }
            metrics.calculate_win_rate();
            metrics.calculate_profit_factor();
        }
    }

    /// Process an account event from gateway
    fn process_account_event(&self, account: &AccountData) {
        let vt_accountid = account.vt_accountid();
        let mut accounts = self.accounts.write().unwrap_or_else(|e| e.into_inner());

        let prev_balance = accounts.get(&vt_accountid)
            .map(|a| a.balance)
            .unwrap_or(account.balance);

        // Balance change ≈ realized PnL (approximation)
        if accounts.contains_key(&vt_accountid) {
            let balance_change = account.balance - prev_balance;
            if balance_change.abs() > 0.001 {
                let mut daily = self.daily_realized_pnl.write().unwrap_or_else(|e| e.into_inner());
                *daily += balance_change;
                let mut total = self.total_realized_pnl.write().unwrap_or_else(|e| e.into_inner());
                *total += balance_change;
            }
        }

        accounts.insert(vt_accountid, account.clone());
    }

    /// Process a tick event to update position market prices
    fn process_tick_event(&self, tick: &TickData) {
        self.update_position_price(&tick.symbol, tick.exchange, tick.last_price);
    }
}

impl Default for PortfolioManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for PortfolioManager {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("[PortfolioManager] Closed");
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        match event {
            GatewayEvent::Position(position) => self.process_position_event(position),
            GatewayEvent::Trade(trade) => self.process_trade_event(trade),
            GatewayEvent::Account(account) => self.process_account_event(account),
            GatewayEvent::Tick(tick) => self.process_tick_event(tick),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_position(symbol: &str, exchange: Exchange, direction: Direction, volume: f64, price: f64, pnl: f64) -> PositionData {
        PositionData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: symbol.to_string(),
            exchange,
            direction,
            volume,
            frozen: 0.0,
            price,
            pnl,
            yd_volume: 0.0,
            extra: None,
        }
    }

    fn make_account(accountid: &str, balance: f64, frozen: f64) -> AccountData {
        AccountData {
            gateway_name: "BINANCE_SPOT".to_string(),
            accountid: accountid.to_string(),
            balance,
            frozen,
            extra: None,
        }
    }

    #[test]
    fn test_portfolio_manager_new() {
        let pm = PortfolioManager::new();
        assert_eq!(pm.engine_name(), "PortfolioManager");
        assert!(pm.get_all_positions().is_empty());
        assert!(pm.get_all_accounts().is_empty());
    }

    #[test]
    fn test_portfolio_process_position() {
        let pm = PortfolioManager::new();
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 1.0, 50000.0, 100.0);
        pm.process_position_event(&pos);

        let positions = pm.get_all_positions();
        assert_eq!(positions.len(), 1);
        assert!((positions[0].volume - 1.0).abs() < 0.001);
        assert!((positions[0].avg_price - 50000.0).abs() < 0.001);
    }

    #[test]
    fn test_portfolio_process_position_close() {
        let pm = PortfolioManager::new();
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 1.0, 50000.0, 100.0);
        pm.process_position_event(&pos);
        assert_eq!(pm.get_all_positions().len(), 1);

        // Close position (volume = 0)
        let pos_closed = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 0.0, 50000.0, 0.0);
        pm.process_position_event(&pos_closed);
        assert_eq!(pm.get_all_positions().len(), 0);
    }

    #[test]
    fn test_portfolio_process_account() {
        let pm = PortfolioManager::new();
        let account = make_account("USDT", 10000.0, 0.0);
        pm.process_account_event(&account);

        let accounts = pm.get_all_accounts();
        assert_eq!(accounts.len(), 1);

        // First update sets baseline, PnL should be 0
        assert!((pm.get_daily_realized_pnl()).abs() < 0.001);
    }

    #[test]
    fn test_portfolio_update_market_price() {
        let pm = PortfolioManager::new();
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 1.0, 50000.0, 0.0);
        pm.process_position_event(&pos);

        pm.update_position_price("BTCUSDT", Exchange::Binance, 52000.0);

        let positions = pm.get_all_positions();
        assert!((positions[0].market_price - 52000.0).abs() < 0.001);
        // Unrealized PnL = (52000 - 50000) * 1.0 = 2000
        assert!((positions[0].unrealized_pnl - 2000.0).abs() < 0.001);
    }

    #[test]
    fn test_portfolio_summary() {
        let pm = PortfolioManager::new();
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 1.0, 50000.0, 100.0);
        pm.process_position_event(&pos);
        pm.update_position_price("BTCUSDT", Exchange::Binance, 52000.0);

        let account = make_account("USDT", 10000.0, 0.0);
        pm.process_account_event(&account);

        let summary = pm.get_portfolio_summary();
        assert!((summary.total_value - 10000.0).abs() < 0.001);
        assert!((summary.total_unrealized_pnl - 2000.0).abs() < 0.001);
        assert_eq!(summary.positions_count, 1);
        assert!((summary.total_exposure - 52000.0).abs() < 0.001);
    }

    #[test]
    fn test_portfolio_unrealized_pnl_short() {
        let pm = PortfolioManager::new();
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Short, 1.0, 50000.0, 0.0);
        pm.process_position_event(&pos);
        pm.update_position_price("BTCUSDT", Exchange::Binance, 48000.0);

        let positions = pm.get_all_positions();
        // Short: (50000 - 48000) * 1.0 = 2000
        assert!((positions[0].unrealized_pnl - 2000.0).abs() < 0.001);
    }

    #[test]
    fn test_portfolio_calculate_unrealized_pnl() {
        let pm = PortfolioManager::new();
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 2.0, 50000.0, 0.0);
        pm.process_position_event(&pos);

        let pnl = pm.calculate_unrealized_pnl(
            &pos.vt_positionid(),
            55000.0,
        );
        // (55000 - 50000) * 2.0 = 10000
        assert!((pnl - 10000.0).abs() < 0.001);
    }

    #[test]
    fn test_portfolio_metrics() {
        let mut metrics = PortfolioMetrics::default();
        metrics.winning_trades = 6;
        metrics.losing_trades = 4;
        metrics.total_wins = 600.0;
        metrics.total_losses = -200.0;
        metrics.calculate_win_rate();
        metrics.calculate_profit_factor();

        assert!((metrics.win_rate - 0.6).abs() < 0.001);
        assert!((metrics.profit_factor - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_position_summary_from_position() {
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 1.0, 50000.0, 100.0);
        let summary = PositionSummary::from_position(&pos);
        assert_eq!(summary.symbol, "BTCUSDT");
        assert!((summary.avg_price - 50000.0).abs() < 0.001);
        assert!((summary.volume - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_position_summary_notional() {
        let pos = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 2.0, 50000.0, 0.0);
        let mut summary = PositionSummary::from_position(&pos);
        summary.market_price = 55000.0;
        assert!((summary.notional_value() - 110000.0).abs() < 0.001);
    }

    #[test]
    fn test_reset_daily_pnl() {
        let pm = PortfolioManager::new();
        // Set up account first, then update with higher balance to trigger PnL
        let account1 = make_account("USDT", 10000.0, 0.0);
        pm.process_account_event(&account1);
        let account2 = make_account("USDT", 10100.0, 0.0);
        pm.process_account_event(&account2);

        // Daily PnL should be 100.0
        assert!((pm.get_daily_realized_pnl() - 100.0).abs() < 0.001);

        pm.reset_daily_pnl();
        assert!((pm.get_daily_realized_pnl()).abs() < 0.001);
        // Total PnL should NOT be reset
        assert!((pm.get_total_realized_pnl() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_active_positions() {
        let pm = PortfolioManager::new();
        let pos1 = make_position("BTCUSDT", Exchange::Binance, Direction::Long, 1.0, 50000.0, 0.0);
        let pos2 = make_position("ETHUSDT", Exchange::Binance, Direction::Long, 0.0, 3000.0, 0.0);
        pm.process_position_event(&pos1);
        pm.process_position_event(&pos2);

        // Only position with volume > 0
        let active = pm.get_active_positions();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].symbol, "BTCUSDT");
    }
}
