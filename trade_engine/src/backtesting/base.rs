//! Backtesting Base Types
//! 
//! Core types and enums for backtesting

use chrono::{Date, NaiveDate, DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

use crate::trader::{TradeData, OrderData};

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
        let pos_change = self.end_pos - self.start_pos;
        
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
        }
    }
}
