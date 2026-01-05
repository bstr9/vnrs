//! Portfolio Backtesting Module
//! 
//! Supports backtesting multiple symbols simultaneously with portfolio management

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc, NaiveDate, Duration};

use crate::trader::{BarData, TickData, Exchange, Interval, Direction, Offset, TradeData};
use crate::strategy::StrategyTemplate;
use super::base::{BacktestingMode, DailyResult, BacktestingStatistics};
use super::statistics::calculate_statistics;

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
        // Merge all bars from different symbols by datetime
        let mut all_bars: Vec<(&String, &BarData)> = Vec::new();
        
        for (symbol, bars) in &self.bar_data {
            for bar in bars {
                all_bars.push((symbol, bar));
            }
        }

        // Sort by datetime
        all_bars.sort_by_key(|(_, bar)| bar.datetime);

        // Initialize strategy
        if let Some(strategy) = &mut self.strategy {
            // Use a dummy context for initialization
            let context = Arc::new(crate::strategy::StrategyContext::new());
            strategy.on_init(&context);
            strategy.on_start();
        }

        // Process bars
        for (symbol, bar) in all_bars {
            self.current_dt = bar.datetime;
            
            // Update strategy with bar
            if let Some(strategy) = &mut self.strategy {
                let context = Arc::new(crate::strategy::StrategyContext::new());
                strategy.on_bar(bar, &context);
            }
        }

        // Stop strategy
        if let Some(strategy) = &mut self.strategy {
            strategy.on_stop();
        }

        Ok(())
    }

    /// Run tick-based portfolio backtesting
    async fn run_tick_backtesting(&mut self) -> Result<(), String> {
        // Similar to bar backtesting but with ticks
        let mut all_ticks: Vec<(&String, &TickData)> = Vec::new();
        
        for (symbol, ticks) in &self.tick_data {
            for tick in ticks {
                all_ticks.push((symbol, tick));
            }
        }

        all_ticks.sort_by_key(|(_, tick)| tick.datetime);

        // Initialize strategy
        if let Some(strategy) = &mut self.strategy {
            // on_init requires context
            let context = Arc::new(crate::strategy::StrategyContext::new());
            strategy.on_init(&context);
            strategy.on_start();
        }

        // Process ticks
        for (_symbol, tick) in all_ticks {
            self.current_dt = tick.datetime;
            
            if let Some(strategy) = &mut self.strategy {
                let context = Arc::new(crate::strategy::StrategyContext::new());
                strategy.on_tick(tick, &context);
            }
        }

        // Stop strategy
        if let Some(strategy) = &mut self.strategy {
            strategy.on_stop();
        }

        Ok(())
    }

    /// Calculate portfolio statistics
    pub fn calculate_statistics(&self) -> Result<PortfolioStatistics, String> {
        if self.trades.is_empty() {
            return Err("没有交易记录".to_string());
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
                        match t.direction {
                            Some(Direction::Long) => {
                                if t.offset == Offset::Close {
                                    t.price * t.volume
                                } else {
                                    -t.price * t.volume
                                }
                            }
                            Some(Direction::Short) => {
                                if t.offset == Offset::Close {
                                    -t.price * t.volume
                                } else {
                                    t.price * t.volume
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
