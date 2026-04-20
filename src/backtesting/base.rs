//! Backtesting Base Types
//!
//! Core types and enums for backtesting

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::trader::TradeData;

/// Backtesting mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BacktestingMode {
    /// Bar-based backtesting (using OHLCV data)
    Bar,
    /// Tick-by-tick backtesting (using tick data)
    Tick,
}

/// Daily backtesting result for a single symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyResult {
    /// Date
    pub date: NaiveDate,
    /// Closing price
    pub close_price: f64,
    /// Previous close
    pub pre_close: f64,
    /// Trades executed
    pub trades: Vec<TradeData>,
    /// Number of trades
    pub trade_count: u32,
    /// Position at start of day
    pub start_pos: f64,
    /// Position at end of day
    pub end_pos: f64,
    /// Total turnover (volume * price)
    pub turnover: f64,
    /// Commission paid
    pub commission: f64,
    /// Slippage cost
    pub slippage: f64,
    /// Trading PnL (from closing positions)
    pub trading_pnl: f64,
    /// Holding PnL (from position value change)
    pub holding_pnl: f64,
    /// Total PnL
    pub total_pnl: f64,
    /// Net PnL (after commission and slippage)
    pub net_pnl: f64,
}

impl DailyResult {
    pub fn new(date: NaiveDate, close_price: f64) -> Self {
        Self {
            date,
            close_price,
            pre_close: 0.0,
            trades: Vec::new(),
            trade_count: 0,
            start_pos: 0.0,
            end_pos: 0.0,
            turnover: 0.0,
            commission: 0.0,
            slippage: 0.0,
            trading_pnl: 0.0,
            holding_pnl: 0.0,
            total_pnl: 0.0,
            net_pnl: 0.0,
        }
    }

    /// Calculate daily result from trades
    pub fn calculate_pnl(&mut self, size: f64, rate: f64, slippage: f64) {
        // Calculate position change
        let _pos_change = self.end_pos - self.start_pos;

        // Calculate holding PnL
        if self.pre_close > 0.0 {
            self.holding_pnl = self.start_pos * (self.close_price - self.pre_close) * size;
        }

        // Calculate trading PnL and costs
        for trade in &self.trades {
            let trade_value = trade.price * trade.volume * size;

            // Trading PnL
            if trade.direction == Some(crate::trader::Direction::Long) {
                self.trading_pnl -= trade_value;
            } else {
                self.trading_pnl += trade_value;
            }

            // Commission
            self.commission += trade_value * rate;

            // Slippage
            self.slippage += trade.volume * size * slippage;

            // Turnover
            self.turnover += trade_value;
        }

        // Total and net PnL
        self.total_pnl = self.trading_pnl + self.holding_pnl;
        self.net_pnl = self.total_pnl - self.commission - self.slippage;
    }
}

/// Overall backtesting result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestingResult {
    /// Initial capital
    pub start_capital: f64,
    /// Final capital
    pub end_capital: f64,
    /// Total return
    pub total_return: f64,
    /// Annual return
    pub annual_return: f64,
    /// Maximum drawdown
    pub max_drawdown: f64,
    /// Maximum drawdown percentage
    pub max_drawdown_percent: f64,
    /// Sharpe ratio
    pub sharpe_ratio: f64,
    /// Total trades
    pub total_trade_count: u32,
    /// Total days
    pub total_days: u32,
    /// Profit days
    pub profit_days: u32,
    /// Loss days
    pub loss_days: u32,
    /// Total commission
    pub total_commission: f64,
    /// Total slippage
    pub total_slippage: f64,
    /// Total turnover
    pub total_turnover: f64,
    /// Daily results
    pub daily_results: HashMap<NaiveDate, DailyResult>,
}

impl BacktestingResult {
    pub fn new(start_capital: f64) -> Self {
        Self {
            start_capital,
            end_capital: start_capital,
            total_return: 0.0,
            annual_return: 0.0,
            max_drawdown: 0.0,
            max_drawdown_percent: 0.0,
            sharpe_ratio: 0.0,
            total_trade_count: 0,
            total_days: 0,
            profit_days: 0,
            loss_days: 0,
            total_commission: 0.0,
            total_slippage: 0.0,
            total_turnover: 0.0,
            daily_results: HashMap::new(),
        }
    }
}

/// Backtesting statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestingStatistics {
    /// Start date
    pub start_date: String,
    /// End date
    pub end_date: String,
    /// Total days
    pub total_days: u32,
    /// Profit days
    pub profit_days: u32,
    /// Loss days
    pub loss_days: u32,
    /// End balance
    pub end_balance: f64,
    /// Max drawdown
    pub max_drawdown: f64,
    /// Max drawdown percent
    pub max_drawdown_percent: f64,
    /// Total net pnl
    pub total_net_pnl: f64,
    /// Total commission
    pub total_commission: f64,
    /// Total slippage
    pub total_slippage: f64,
    /// Total turnover
    pub total_turnover: f64,
    /// Total trade count
    pub total_trade_count: u32,
    /// Daily net pnl
    pub daily_net_pnl: f64,
    /// Daily commission
    pub daily_commission: f64,
    /// Daily slippage
    pub daily_slippage: f64,
    /// Daily turnover
    pub daily_turnover: f64,
    /// Daily trade count
    pub daily_trade_count: f64,
    /// Daily return
    pub daily_return: f64,
    /// Return std
    pub return_std: f64,
    /// Sharpe ratio
    pub sharpe_ratio: f64,
    /// Annual return
    pub return_mean: f64,
    // GAP 3 additions: additional performance metrics
    /// Win rate (fraction of profitable trades)
    pub win_rate: f64,
    /// Profit factor (gross profit / gross loss)
    pub profit_factor: f64,
    /// Average trade PnL
    pub avg_trade_pnl: f64,
    /// Maximum consecutive winning trades
    pub max_consecutive_wins: u32,
    /// Maximum consecutive losing trades
    pub max_consecutive_losses: u32,
    /// Sortino ratio (downside deviation based)
    pub sortino_ratio: f64,
    /// Calmar ratio (annual return / max drawdown)
    pub calmar_ratio: f64,
    /// Average winning trade PnL
    pub avg_winning_trade: f64,
    /// Average losing trade PnL
    pub avg_losing_trade: f64,
    /// Largest winning trade PnL
    pub largest_winning_trade: f64,
    /// Largest losing trade PnL
    pub largest_losing_trade: f64,
}

impl Default for BacktestingStatistics {
    fn default() -> Self {
        Self {
            start_date: String::new(),
            end_date: String::new(),
            total_days: 0,
            profit_days: 0,
            loss_days: 0,
            end_balance: 0.0,
            max_drawdown: 0.0,
            max_drawdown_percent: 0.0,
            total_net_pnl: 0.0,
            total_commission: 0.0,
            total_slippage: 0.0,
            total_turnover: 0.0,
            total_trade_count: 0,
            daily_net_pnl: 0.0,
            daily_commission: 0.0,
            daily_slippage: 0.0,
            daily_turnover: 0.0,
            daily_trade_count: 0.0,
            daily_return: 0.0,
            return_std: 0.0,
            sharpe_ratio: 0.0,
            return_mean: 0.0,
            win_rate: 0.0,
            profit_factor: 0.0,
            avg_trade_pnl: 0.0,
            max_consecutive_wins: 0,
            max_consecutive_losses: 0,
            sortino_ratio: 0.0,
            calmar_ratio: 0.0,
            avg_winning_trade: 0.0,
            avg_losing_trade: 0.0,
            largest_winning_trade: 0.0,
            largest_losing_trade: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Direction, TradeData};
    use chrono::NaiveDate;

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_backtesting_mode_serialize_deserialize() {
        let bar_mode = BacktestingMode::Bar;
        let json = serde_json::to_string(&bar_mode).unwrap();
        let deserialized: BacktestingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(bar_mode, deserialized);

        let tick_mode = BacktestingMode::Tick;
        let json = serde_json::to_string(&tick_mode).unwrap();
        let deserialized: BacktestingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(tick_mode, deserialized);
    }

    #[test]
    fn test_backtesting_mode_equality() {
        assert_eq!(BacktestingMode::Bar, BacktestingMode::Bar);
        assert_eq!(BacktestingMode::Tick, BacktestingMode::Tick);
        assert_ne!(BacktestingMode::Bar, BacktestingMode::Tick);
    }

    #[test]
    fn test_daily_result_new_defaults() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let result = DailyResult::new(date, 100.0);

        assert_eq!(result.date, date);
        assert!((result.close_price - 100.0).abs() < f64::EPSILON);
        assert!((result.pre_close - 0.0).abs() < f64::EPSILON);
        assert!(result.trades.is_empty());
        assert_eq!(result.trade_count, 0);
        assert!((result.start_pos - 0.0).abs() < f64::EPSILON);
        assert!((result.end_pos - 0.0).abs() < f64::EPSILON);
        assert!((result.turnover - 0.0).abs() < f64::EPSILON);
        assert!((result.commission - 0.0).abs() < f64::EPSILON);
        assert!((result.slippage - 0.0).abs() < f64::EPSILON);
        assert!((result.trading_pnl - 0.0).abs() < f64::EPSILON);
        assert!((result.holding_pnl - 0.0).abs() < f64::EPSILON);
        assert!((result.total_pnl - 0.0).abs() < f64::EPSILON);
        assert!((result.net_pnl - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_daily_result_calculate_pnl_no_trades() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut result = DailyResult::new(date, 105.0);
        result.pre_close = 100.0;
        result.start_pos = 10.0;
        result.end_pos = 10.0;

        result.calculate_pnl(1.0, 0.001, 0.1);

        // holding_pnl = start_pos * (close - pre_close) * size = 10 * (105 - 100) * 1 = 50
        assert!((result.holding_pnl - 50.0).abs() < 1e-10);
        assert!((result.trading_pnl - 0.0).abs() < f64::EPSILON);
        assert!((result.total_pnl - 50.0).abs() < 1e-10);
        assert!((result.net_pnl - 50.0).abs() < 1e-10);
        assert!((result.commission - 0.0).abs() < f64::EPSILON);
        assert!((result.slippage - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_daily_result_calculate_pnl_with_trades() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut result = DailyResult::new(date, 105.0);
        result.pre_close = 100.0;
        result.start_pos = 10.0;
        result.end_pos = 5.0;

        // Add a short trade: selling 5 units at price 103
        let trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: crate::trader::Exchange::Binance,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Short),
            offset: crate::trader::Offset::Open,
            price: 103.0,
            volume: 5.0,
            datetime: None,
            extra: None,
        };
        result.trades.push(trade);
        result.trade_count = 1;

        result.calculate_pnl(1.0, 0.001, 0.1);

        // trade_value = 103.0 * 5.0 * 1.0 = 515.0
        // Short direction: trading_pnl += trade_value => trading_pnl = 515.0
        let trade_value = 103.0 * 5.0 * 1.0;
        assert!((result.trading_pnl - trade_value).abs() < 1e-10);
        // holding_pnl = 10 * (105 - 100) * 1 = 50
        assert!((result.holding_pnl - 50.0).abs() < 1e-10);
        // commission = trade_value * 0.001
        assert!((result.commission - trade_value * 0.001).abs() < 1e-10);
        // slippage = 5.0 * 1.0 * 0.1 = 0.5
        assert!((result.slippage - 0.5).abs() < 1e-10);
        // total_pnl = trading_pnl + holding_pnl
        assert!((result.total_pnl - (trade_value + 50.0)).abs() < 1e-10);
        // net_pnl = total_pnl - commission - slippage
        let expected_net = trade_value + 50.0 - trade_value * 0.001 - 0.5;
        assert!((result.net_pnl - expected_net).abs() < 1e-9);
    }

    #[test]
    fn test_backtesting_result_new_defaults() {
        let result = BacktestingResult::new(100000.0);

        assert!((result.start_capital - 100000.0).abs() < f64::EPSILON);
        assert!((result.end_capital - 100000.0).abs() < f64::EPSILON);
        assert!((result.total_return - 0.0).abs() < f64::EPSILON);
        assert!((result.annual_return - 0.0).abs() < f64::EPSILON);
        assert!((result.max_drawdown - 0.0).abs() < f64::EPSILON);
        assert!((result.max_drawdown_percent - 0.0).abs() < f64::EPSILON);
        assert!((result.sharpe_ratio - 0.0).abs() < f64::EPSILON);
        assert_eq!(result.total_trade_count, 0);
        assert_eq!(result.total_days, 0);
        assert!(result.daily_results.is_empty());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_backtesting_result_with_daily_results() {
        let mut result = BacktestingResult::new(50000.0);
        let date1 = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let date2 = NaiveDate::from_ymd_opt(2024, 1, 16).unwrap();

        let daily1 = DailyResult::new(date1, 100.0);
        let daily2 = DailyResult::new(date2, 102.0);

        result.daily_results.insert(date1, daily1);
        result.daily_results.insert(date2, daily2);

        assert_eq!(result.daily_results.len(), 2);
        assert!(result.daily_results.contains_key(&date1));
        assert!(result.daily_results.contains_key(&date2));
        assert!((result.daily_results[&date2].close_price - 102.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_backtesting_statistics_default() {
        let stats = BacktestingStatistics::default();

        assert!(stats.start_date.is_empty());
        assert!(stats.end_date.is_empty());
        assert_eq!(stats.total_days, 0);
        assert_eq!(stats.profit_days, 0);
        assert_eq!(stats.loss_days, 0);
        assert!((stats.end_balance - 0.0).abs() < f64::EPSILON);
        assert!((stats.max_drawdown - 0.0).abs() < f64::EPSILON);
        assert!((stats.total_net_pnl - 0.0).abs() < f64::EPSILON);
        assert!((stats.sharpe_ratio - 0.0).abs() < f64::EPSILON);
        assert!((stats.win_rate - 0.0).abs() < f64::EPSILON);
        assert!((stats.profit_factor - 0.0).abs() < f64::EPSILON);
        assert!((stats.sortino_ratio - 0.0).abs() < f64::EPSILON);
        assert!((stats.calmar_ratio - 0.0).abs() < f64::EPSILON);
        assert_eq!(stats.max_consecutive_wins, 0);
        assert_eq!(stats.max_consecutive_losses, 0);
        assert_eq!(stats.total_trade_count, 0);
    }
}
