//! Portfolio Backtesting Module
//! 
//! Supports backtesting multiple symbols simultaneously with portfolio management

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc, NaiveDate};

use crate::trader::{BarData, TickData, Exchange, Interval, Direction, Offset, TradeData};
use crate::strategy::StrategyTemplate;
use super::base::BacktestingMode;
use super::data_merge::{BarMergeIterator, TickMergeIterator};

/// Symbol configuration for portfolio
#[derive(Clone)]
pub struct SymbolConfig {
    pub vt_symbol: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub size: f64,           // Contract size
    pub pricetick: f64,      // Minimum price movement
    pub min_volume: f64,     // Minimum order volume
}

/// Portfolio backtesting engine
pub struct PortfolioBacktestingEngine {
    // Symbols
    symbols: Vec<SymbolConfig>,
    
    // Common settings
    interval: Interval,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    rate: f64,
    slippage: f64,
    capital: f64,
    mode: BacktestingMode,
    
    // Market data - organized by symbol
    bar_data: HashMap<String, Vec<BarData>>,
    tick_data: HashMap<String, Vec<TickData>>,
    current_dt: DateTime<Utc>,
    
    // Position tracking - by symbol
    positions: HashMap<String, f64>,
    
    // Trading records
    trades: Vec<TradeData>,
    
    // Daily results
    daily_results: HashMap<NaiveDate, PortfolioDailyResult>,
    
    // Strategy
    strategy: Option<Box<dyn StrategyTemplate>>,
}

/// Daily result for portfolio
#[derive(Debug, Clone)]
pub struct PortfolioDailyResult {
    pub date: NaiveDate,
    pub positions: HashMap<String, f64>,  // Symbol -> Position
    pub trades: Vec<TradeData>,
    pub trade_count: usize,
    pub total_pnl: f64,
    pub net_pnl: f64,
    pub commission: f64,
    pub slippage: f64,
}

impl Default for PortfolioBacktestingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PortfolioBacktestingEngine {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            interval: Interval::Minute,
            start: Utc::now(),
            end: Utc::now(),
            rate: 0.0,
            slippage: 0.0,
            capital: 1_000_000.0,
            mode: BacktestingMode::Bar,
            bar_data: HashMap::new(),
            tick_data: HashMap::new(),
            current_dt: Utc::now(),
            positions: HashMap::new(),
            trades: Vec::new(),
            daily_results: HashMap::new(),
            strategy: None,
        }
    }

    /// Add symbol to portfolio
    pub fn add_symbol(&mut self, config: SymbolConfig) {
        self.positions.insert(config.vt_symbol.clone(), 0.0);
        self.symbols.push(config);
    }

    /// Set common parameters
    #[allow(clippy::too_many_arguments)]
    pub fn set_parameters(
        &mut self,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        rate: f64,
        slippage: f64,
        capital: f64,
        mode: BacktestingMode,
    ) {
        self.interval = interval;
        self.start = start;
        self.end = end;
        self.rate = rate;
        self.slippage = slippage;
        self.capital = capital;
        self.mode = mode;
    }

    /// Set bar data for a symbol
    pub fn set_bar_data(&mut self, vt_symbol: &str, bars: Vec<BarData>) {
        self.bar_data.insert(vt_symbol.to_string(), bars);
    }

    /// Set tick data for a symbol
    pub fn set_tick_data(&mut self, vt_symbol: &str, ticks: Vec<TickData>) {
        self.tick_data.insert(vt_symbol.to_string(), ticks);
    }

    /// Add strategy
    pub fn add_strategy(&mut self, strategy: Box<dyn StrategyTemplate>) {
        self.strategy = Some(strategy);
    }

    /// Run portfolio backtesting
    pub async fn run_backtesting(&mut self) -> Result<(), String> {
        if self.strategy.is_none() {
            return Err("未设置策略".to_string());
        }

        match self.mode {
            BacktestingMode::Bar => self.run_bar_backtesting().await,
            BacktestingMode::Tick => self.run_tick_backtesting().await,
        }
    }

    /// Run bar-based portfolio backtesting
    async fn run_bar_backtesting(&mut self) -> Result<(), String> {
        // Use heap-based K-way merge instead of clone + sort.
        // Swap out bar_data to avoid borrow conflicts during iteration.
        let bar_data = std::mem::take(&mut self.bar_data);
        let sources: Vec<&[BarData]> = bar_data.values().map(|v| v.as_slice()).collect();
        let merge_iter = BarMergeIterator::new(sources);

        // Initialize strategy
        if let Some(strategy) = &mut self.strategy {
            let context = Arc::new(crate::strategy::StrategyContext::new());
            strategy.on_init(&context);
            strategy.on_start();
        }

        let mut current_date: Option<NaiveDate> = None;
        let mut daily_trade_start_idx = 0;

        // Process bars via K-way merge (O(N log K) instead of O(N log N))
        for bar in merge_iter {
            let bar_date = bar.datetime.date_naive();

            // When date changes, finalize the previous day's result
            if current_date.is_some_and(|d| d != bar_date) {
                if let Some(prev_date) = current_date {
                    self.finalize_daily_result(prev_date, &mut daily_trade_start_idx);
                }
            }
            current_date = Some(bar_date);

            self.current_dt = bar.datetime;
            
            if let Some(strategy) = &mut self.strategy {
                let context = Arc::new(crate::strategy::StrategyContext::new());
                strategy.on_bar(bar, &context);
            }
        }

        // Restore bar_data
        self.bar_data = bar_data;

        // Finalize the last day
        if let Some(date) = current_date {
            self.finalize_daily_result(date, &mut daily_trade_start_idx);
        }

        // Stop strategy
        if let Some(strategy) = &mut self.strategy {
            strategy.on_stop();
        }

        Ok(())
    }

    /// Run tick-based portfolio backtesting
    async fn run_tick_backtesting(&mut self) -> Result<(), String> {
        // Use heap-based K-way merge instead of clone + sort.
        // Swap out tick_data to avoid borrow conflicts during iteration.
        let tick_data = std::mem::take(&mut self.tick_data);
        let sources: Vec<&[TickData]> = tick_data.values().map(|v| v.as_slice()).collect();
        let merge_iter = TickMergeIterator::new(sources);

        // Initialize strategy
        if let Some(strategy) = &mut self.strategy {
            let context = Arc::new(crate::strategy::StrategyContext::new());
            strategy.on_init(&context);
            strategy.on_start();
        }

        let mut current_date: Option<NaiveDate> = None;
        let mut daily_trade_start_idx = 0;

        // Process ticks via K-way merge (O(N log K) instead of O(N log N))
        for tick in merge_iter {
            let tick_date = tick.datetime.date_naive();

            if current_date.is_some_and(|d| d != tick_date) {
                if let Some(prev_date) = current_date {
                    self.finalize_daily_result(prev_date, &mut daily_trade_start_idx);
                }
            }
            current_date = Some(tick_date);

            self.current_dt = tick.datetime;
            
            if let Some(strategy) = &mut self.strategy {
                let context = Arc::new(crate::strategy::StrategyContext::new());
                strategy.on_tick(tick, &context);
            }
        }

        // Restore tick_data
        self.tick_data = tick_data;

        if let Some(date) = current_date {
            self.finalize_daily_result(date, &mut daily_trade_start_idx);
        }

        // Stop strategy
        if let Some(strategy) = &mut self.strategy {
            strategy.on_stop();
        }

        Ok(())
    }

    /// Finalize daily result for a given date
    fn finalize_daily_result(&mut self, date: NaiveDate, daily_trade_start_idx: &mut usize) {
        let daily_trades: Vec<TradeData> = self.trades[*daily_trade_start_idx..].to_vec();
        *daily_trade_start_idx = self.trades.len();

        let trade_count = daily_trades.len();
        let commission: f64 = daily_trades.iter()
            .map(|t| t.price * t.volume * self.rate)
            .sum();
        let slippage: f64 = daily_trades.iter()
            .map(|t| t.volume * self.slippage)
            .sum();

        let mut net_pnl = 0.0;
        for trade in &daily_trades {
            let config = self.symbols.iter().find(|c| c.symbol == trade.symbol);
            let size = config.map(|c| c.size).unwrap_or(1.0);
            let pnl = match trade.direction {
                Some(Direction::Long) => {
                    if trade.offset == Offset::Close || trade.offset == Offset::CloseToday || trade.offset == Offset::CloseYesterday {
                        trade.price * trade.volume * size
                    } else {
                        -trade.price * trade.volume * size
                    }
                }
                Some(Direction::Short) => {
                    if trade.offset == Offset::Close || trade.offset == Offset::CloseToday || trade.offset == Offset::CloseYesterday {
                        -trade.price * trade.volume * size
                    } else {
                        trade.price * trade.volume * size
                    }
                }
                _ => 0.0,
            };
            net_pnl += pnl;
        }
        net_pnl -= commission + slippage;

        let result = PortfolioDailyResult {
            date,
            positions: self.positions.clone(),
            trades: daily_trades,
            trade_count,
            total_pnl: net_pnl + commission + slippage,
            net_pnl,
            commission,
            slippage,
        };

        self.daily_results.insert(date, result);
    }

    /// Calculate portfolio statistics
    pub fn calculate_statistics(&mut self) -> Result<PortfolioStatistics, String> {
        if self.trades.is_empty() {
            return Err("没有交易记录".to_string());
        }

        // Update positions from trades
        for trade in &self.trades {
            let key = self.symbols.iter()
                .find(|c| c.symbol == trade.symbol)
                .map(|c| &c.vt_symbol);
            if let Some(vt_sym) = key {
                let pos = self.positions.entry(vt_sym.clone()).or_insert(0.0);
                match trade.direction {
                    Some(Direction::Long) => {
                        match trade.offset {
                            Offset::Open | Offset::None => *pos += trade.volume,
                            Offset::Close | Offset::CloseToday | Offset::CloseYesterday => *pos -= trade.volume,
                        }
                    }
                    Some(Direction::Short) => {
                        match trade.offset {
                            Offset::Open | Offset::None => *pos -= trade.volume,
                            Offset::Close | Offset::CloseToday | Offset::CloseYesterday => *pos += trade.volume,
                        }
                    }
                    _ => {}
                }
            }
        }

        // Calculate per-symbol statistics
        let mut symbol_stats = HashMap::new();
        
        for config in &self.symbols {
            let symbol_trades: Vec<_> = self.trades.iter()
                .filter(|t| t.symbol == config.symbol)
                .cloned()
                .collect();

            if !symbol_trades.is_empty() {
                // Calculate individual symbol performance
                let pnl: f64 = symbol_trades.iter()
                    .map(|t| {
                        let size = config.size;
                        match t.direction {
                            Some(Direction::Long) => {
                                if t.offset == Offset::Close {
                                    t.price * t.volume * size
                                } else {
                                    -t.price * t.volume * size
                                }
                            }
                            Some(Direction::Short) => {
                                if t.offset == Offset::Close {
                                    -t.price * t.volume * size
                                } else {
                                    t.price * t.volume * size
                                }
                            }
                            _ => 0.0,
                        }
                    })
                    .sum();

                symbol_stats.insert(config.vt_symbol.clone(), SymbolStatistics {
                    trade_count: symbol_trades.len(),
                    total_pnl: pnl,
                    position: *self.positions.get(&config.vt_symbol).unwrap_or(&0.0),
                });
            }
        }

        // Calculate portfolio-level statistics
        let total_pnl: f64 = symbol_stats.values().map(|s| s.total_pnl).sum();
        let total_trades: usize = symbol_stats.values().map(|s| s.trade_count).sum();

        Ok(PortfolioStatistics {
            start_date: self.start.date_naive(),
            end_date: self.end.date_naive(),
            capital: self.capital,
            total_pnl,
            total_return: total_pnl / self.capital,
            total_trades,
            symbol_stats,
        })
    }

    /// Get current positions
    pub fn get_positions(&self) -> HashMap<String, f64> {
        self.positions.clone()
    }

    /// Get all trades
    pub fn get_trades(&self) -> Vec<TradeData> {
        self.trades.clone()
    }
}

/// Symbol-level statistics
#[derive(Debug, Clone)]
pub struct SymbolStatistics {
    pub trade_count: usize,
    pub total_pnl: f64,
    pub position: f64,
}

/// Portfolio-level statistics
#[derive(Debug, Clone)]
pub struct PortfolioStatistics {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub capital: f64,
    pub total_pnl: f64,
    pub total_return: f64,
    pub total_trades: usize,
    pub symbol_stats: HashMap<String, SymbolStatistics>,
}
