//! Backtesting Engine
//! 
//! Core backtesting engine that integrates with strategy framework
//! Supports both spot and futures trading

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc, NaiveDate, Datelike, Duration};
use tokio::sync::RwLock;

use crate::trader::{
    TickData, BarData, OrderData, TradeData, ContractData,
    Direction, Offset, OrderType, Status, Interval, Exchange,
    OrderRequest, HistoryRequest,
};
use crate::strategy::{
    StrategyTemplate, StrategyContext, StrategyState,
    StopOrder, StopOrderStatus,
};
use super::base::{BacktestingMode, DailyResult, BacktestingResult, BacktestingStatistics};
use super::statistics::calculate_statistics;
use super::database::DatabaseLoader;

/// Backtesting engine for strategy testing
pub struct BacktestingEngine {
    // Basic settings
    vt_symbol: String,
    symbol: String,
    exchange: Exchange,
    interval: Interval,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    
    // Trading parameters
    rate: f64,          // Commission rate
    slippage: f64,      // Slippage per share/contract
    size: f64,          // Contract size (1 for spot, different for futures)
    pricetick: f64,     // Minimum price movement
    capital: f64,       // Initial capital
    
    // Backtesting mode
    mode: BacktestingMode,
    
    // Strategy
    strategy: Option<Box<dyn StrategyTemplate>>,
    strategy_context: Arc<StrategyContext>,
    
    // Market data
    history_data: Vec<BarData>,
    tick_data: Vec<TickData>,
    current_dt: DateTime<Utc>,
    
    // Order management
    limit_order_count: u64,
    limit_orders: HashMap<String, OrderData>,
    active_limit_orders: HashMap<String, OrderData>,
    
    // Stop order management
    stop_order_count: u64,
    stop_orders: HashMap<String, StopOrder>,
    active_stop_orders: HashMap<String, StopOrder>,
    
    // Trade tracking
    trade_count: u64,
    trades: HashMap<String, TradeData>,
    
    // Position tracking
    pos: f64,
    
    // Daily results
    daily_results: HashMap<NaiveDate, DailyResult>,
    daily_result: Option<DailyResult>,
    
    // Logging
    logs: Vec<String>,
}

impl BacktestingEngine {
    /// Create new backtesting engine
    pub fn new() -> Self {
        Self {
            vt_symbol: String::new(),
            symbol: String::new(),
            exchange: Exchange::Binance,
            interval: Interval::Minute,
            start: Utc::now(),
            end: Utc::now(),
            rate: 0.0,
            slippage: 0.0,
            size: 1.0,
            pricetick: 0.01,
            capital: 1_000_000.0,
            mode: BacktestingMode::Bar,
            strategy: None,
            strategy_context: Arc::new(StrategyContext::new()),
            history_data: Vec::new(),
            tick_data: Vec::new(),
            current_dt: Utc::now(),
            limit_order_count: 0,
            limit_orders: HashMap::new(),
            active_limit_orders: HashMap::new(),
            stop_order_count: 0,
            stop_orders: HashMap::new(),
            active_stop_orders: HashMap::new(),
            trade_count: 0,
            trades: HashMap::new(),
            pos: 0.0,
            daily_results: HashMap::new(),
            daily_result: None,
            logs: Vec::new(),
        }
    }

    /// Clear all previous backtesting data
    pub fn clear_data(&mut self) {
        self.limit_order_count = 0;
        self.limit_orders.clear();
        self.active_limit_orders.clear();
        
        self.stop_order_count = 0;
        self.stop_orders.clear();
        self.active_stop_orders.clear();
        
        self.trade_count = 0;
        self.trades.clear();
        
        self.pos = 0.0;
        self.daily_results.clear();
        self.daily_result = None;
        
        self.logs.clear();
        self.history_data.clear();
    }

    /// Set backtesting parameters
    pub fn set_parameters(
        &mut self,
        vt_symbol: String,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        rate: f64,
        slippage: f64,
        size: f64,
        pricetick: f64,
        capital: f64,
        mode: BacktestingMode,
    ) {
        self.vt_symbol = vt_symbol.clone();
        
        // Parse symbol and exchange
        let parts: Vec<&str> = vt_symbol.split('.').collect();
        if parts.len() == 2 {
            self.symbol = parts[0].to_string();
            self.exchange = match parts[1].to_uppercase().as_str() {
                "BINANCE" => Exchange::Binance,
                _ => Exchange::Binance,
            };
        }
        
        self.interval = interval;
        self.start = start;
        self.end = end;
        self.rate = rate;
        self.slippage = slippage;
        self.size = size;
        self.pricetick = pricetick;
        self.capital = capital;
        self.mode = mode;
    }

    /// Add strategy to backtesting engine
    pub fn add_strategy(&mut self, strategy: Box<dyn StrategyTemplate>) {
        self.strategy = Some(strategy);
    }

    /// Load historical data
    pub async fn load_data(&mut self) -> Result<(), String> {
        self.write_log("开始加载历史数据");

        if self.start >= self.end {
            return Err("起始日期必须小于结束日期".to_string());
        }

        // In real implementation, load data from database
        // For now, this is a placeholder
        self.write_log("历史数据加载完成");
        
        Ok(())
    }

    /// Load data from database
    pub async fn load_data_from_db(&mut self, database_url: &str) -> Result<(), String> {
        self.write_log("从数据库加载历史数据");

        if self.start >= self.end {
            return Err("起始日期必须小于结束日期".to_string());
        }

        let mut loader = DatabaseLoader::new();
        loader.connect(database_url).await?;

        match self.mode {
            BacktestingMode::Bar => {
                let bars = loader.load_bar_data(
                    &self.symbol,
                    self.exchange,
                    self.interval,
                    self.start,
                    self.end,
                ).await?;
                
                self.history_data = bars;
                self.write_log(&format!("从数据库加载{}条Bar数据", self.history_data.len()));
            }
            BacktestingMode::Tick => {
                let ticks = loader.load_tick_data(
                    &self.symbol,
                    self.exchange,
                    self.start,
                    self.end,
                ).await?;
                
                self.tick_data = ticks;
                self.write_log(&format!("从数据库加载{}条Tick数据", self.tick_data.len()));
            }
        }
        
        Ok(())
    }

    /// Load bar data from external source (to be called from Python)
    pub fn set_history_data(&mut self, bars: Vec<BarData>) {
        self.history_data = bars;
        self.write_log(&format!("加载{}条历史数据", self.history_data.len()));
    }

    /// Set tick data for backtesting
    pub fn set_tick_data(&mut self, ticks: Vec<TickData>) {
        self.tick_data = ticks;
        self.write_log(&format!("加载{}条Tick数据", self.tick_data.len()));
    }


    /// Run backtesting
    pub async fn run_backtesting(&mut self) -> Result<(), String> {
        if self.history_data.is_empty() {
            return Err("历史数据为空，无法开始回测".to_string());
        }

        if self.strategy.is_none() {
            return Err("未设置策略".to_string());
        }

        self.write_log("开始运行回测");

        // Clone context for strategy callbacks
        let context = Arc::clone(&self.strategy_context);

        // Initialize strategy
        if let Some(strategy) = &mut self.strategy {
            strategy.on_init(&context);
            strategy.on_start();
        }

        // Process historical data
        match self.mode {
            BacktestingMode::Bar => {
                self.run_bar_backtesting().await?;
            }
            BacktestingMode::Tick => {
                self.run_tick_backtesting().await?;
            }
        }

        // Stop strategy
        if let Some(strategy) = &mut self.strategy {
            strategy.on_stop();
        }

        self.write_log("回测运行结束");
        Ok(())
    }

    /// Run bar-based backtesting
    async fn run_bar_backtesting(&mut self) -> Result<(), String> {
        let context = Arc::clone(&self.strategy_context);
        
        for bar in self.history_data.clone() {
            self.current_dt = bar.datetime;
            
            // New day - create new daily result
            self.new_day(&bar);
            
            // Cross limit orders
            self.cross_limit_order(&bar);
            
            // Cross stop orders
            self.cross_stop_order(&bar);
            
            // Update bar to strategy
            if let Some(strategy) = &mut self.strategy {
                strategy.on_bar(&bar, &context);
            }
        }

        // Close last day
        if let Some(last_bar) = self.history_data.last().cloned() {
            self.close_day(&last_bar);
        }

        Ok(())
    }

    /// Run tick-based backtesting
    async fn run_tick_backtesting(&mut self) -> Result<(), String> {
        let context = Arc::clone(&self.strategy_context);
        
        for tick in self.tick_data.clone() {
            self.current_dt = tick.datetime;
            
            // Cross limit orders with tick data
            self.cross_limit_order_tick(&tick);
            
            // Cross stop orders with tick data
            self.cross_stop_order_tick(&tick);
            
            // Update tick to strategy
            if let Some(strategy) = &mut self.strategy {
                let context = Arc::clone(&self.strategy_context);
                strategy.on_tick(&tick, &context);
            }
        }

        // Calculate final result based on last tick
        if let Some(last_tick) = self.tick_data.last() {
            // Create a synthetic bar for closing
            let close_bar = BarData {
                gateway_name: "BACKTESTING".to_string(),
                symbol: last_tick.symbol.clone(),
                exchange: last_tick.exchange,
                datetime: last_tick.datetime,
                interval: Some(Interval::Tick),
                volume: last_tick.volume,
                turnover: last_tick.turnover,
                open_interest: last_tick.open_interest,
                open_price: last_tick.last_price,
                high_price: last_tick.last_price,
                low_price: last_tick.last_price,
                close_price: last_tick.last_price,
                extra: None,
            };
            self.close_day(&close_bar);
        }

        Ok(())
    }

    /// Handle new day
    fn new_day(&mut self, bar: &BarData) {
        let bar_date = bar.datetime.date_naive();
        
        // Check if it's a new day
        if let Some(ref mut result) = self.daily_result {
            let result_date = result.date;
            
            if bar_date != result_date {
                // Close previous day
                let prev_bar = BarData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: bar.symbol.clone(),
                    exchange: bar.exchange,
                    datetime: bar.datetime - Duration::days(1),
                    interval: bar.interval,
                    volume: bar.volume,
                    turnover: bar.turnover,
                    open_interest: bar.open_interest,
                    open_price: result.close_price,
                    high_price: result.close_price,
                    low_price: result.close_price,
                    close_price: result.close_price,
                    extra: None,
                };
                self.close_day(&prev_bar);
                
                // Create new day result
                self.daily_result = Some(DailyResult::new(bar_date, bar.close_price));
                if let Some(result) = &mut self.daily_result {
                    result.pre_close = prev_bar.close_price;
                    result.start_pos = self.pos;
                }
            }
        } else {
            // First day
            self.daily_result = Some(DailyResult::new(bar_date, bar.close_price));
            if let Some(result) = &mut self.daily_result {
                result.start_pos = self.pos;
            }
        }
    }

    /// Close day and save result
    fn close_day(&mut self, bar: &BarData) {
        if let Some(mut result) = self.daily_result.take() {
            result.close_price = bar.close_price;
            result.end_pos = self.pos;
            
            // Calculate PnL
            result.calculate_pnl(self.size, self.rate, self.slippage);
            
            // Save result
            self.daily_results.insert(result.date, result);
        }
    }

    /// Cross limit orders with bar
    fn cross_limit_order(&mut self, bar: &BarData) {
        let mut to_remove = Vec::new();
        
        // Collect active orders to avoid borrow issues
        let active_orders: Vec<_> = self.active_limit_orders.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        for (vt_orderid, order) in active_orders {
            let should_cross = match order.direction {
                Some(Direction::Long) => {
                    order.price >= bar.low_price
                }
                Some(Direction::Short) => {
                    order.price <= bar.high_price
                }
                _ => false,
            };

            if should_cross {
                // Trade price is order price
                let trade_price = order.price;
                
                // Create trade
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: order.symbol.clone(),
                    exchange: order.exchange,
                    orderid: order.orderid.clone(),
                    tradeid: format!("{}", self.trade_count),
                    direction: order.direction,
                    offset: order.offset,
                    price: trade_price,
                    volume: order.volume,
                    datetime: Some(bar.datetime),
                    extra: None,
                };

                // Save trade
                let vt_tradeid = trade.vt_tradeid();
                self.trades.insert(vt_tradeid.clone(), trade.clone());
                
                // Update position
                self.update_position(&trade);
                
                // Update daily result
                if let Some(result) = &mut self.daily_result {
                    result.trades.push(trade.clone());
                    result.trade_count += 1;
                }

                // Call strategy callback
                if let Some(strategy) = &mut self.strategy {
                    strategy.on_trade(&trade);
                }

                // Mark for removal
                to_remove.push(vt_orderid.clone());
            }
        }

        // Remove crossed orders
        for vt_orderid in to_remove {
            self.active_limit_orders.remove(&vt_orderid);
        }
    }

    /// Cross stop orders with bar
    fn cross_stop_order(&mut self, bar: &BarData) {
        let mut to_trigger = Vec::new();
        
        for (stop_orderid, stop_order) in self.active_stop_orders.iter() {
            let should_trigger = match stop_order.direction {
                Direction::Long => bar.high_price >= stop_order.price,
                Direction::Short => bar.low_price <= stop_order.price,
                _ => false,
            };

            if should_trigger {
                to_trigger.push((stop_orderid.clone(), stop_order.clone()));
            }
        }

        // Trigger stop orders
        for (stop_orderid, mut stop_order) in to_trigger {
            // Update stop order status
            stop_order.status = StopOrderStatus::Triggered;
            
            // Send limit order
            let order_req = OrderRequest {
                symbol: stop_order.vt_symbol.split('.').next().unwrap().to_string(),
                exchange: self.exchange,
                direction: stop_order.direction,
                order_type: stop_order.order_type,
                volume: stop_order.volume,
                price: stop_order.price,
                offset: stop_order.offset.unwrap_or(Offset::Open),
                reference: format!("STOP_{}", stop_order.strategy_name),
            };

            let vt_orderid = self.send_limit_order(order_req);
            stop_order.vt_orderid = Some(vt_orderid);
            
            // Update stop order
            self.stop_orders.insert(stop_orderid.clone(), stop_order);
            self.active_stop_orders.remove(&stop_orderid);
        }
    }

    /// Cross limit orders with tick data
    fn cross_limit_order_tick(&mut self, tick: &TickData) {
        let mut to_remove = Vec::new();
        
        // Collect active orders to avoid borrow issues
        let active_orders: Vec<_> = self.active_limit_orders.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        for (vt_orderid, order) in active_orders {
            let should_cross = match order.direction {
                Some(Direction::Long) => {
                    // Buy order: check if we can buy at or below order price
                    order.price >= tick.ask_price_1
                }
                Some(Direction::Short) => {
                    // Sell order: check if we can sell at or above order price
                    order.price <= tick.bid_price_1
                }
                _ => false,
            };

            if should_cross {
                // Use best available price
                let trade_price = match order.direction {
                    Some(Direction::Long) => tick.ask_price_1,
                    Some(Direction::Short) => tick.bid_price_1,
                    _ => order.price,
                };
                
                // Create trade
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: order.symbol.clone(),
                    exchange: order.exchange,
                    orderid: order.orderid.clone(),
                    tradeid: format!("{}", self.trade_count),
                    direction: order.direction,
                    offset: order.offset,
                    price: trade_price,
                    volume: order.volume,
                    datetime: Some(tick.datetime),
                    extra: None,
                };

                // Save trade
                let vt_tradeid = trade.vt_tradeid();
                self.trades.insert(vt_tradeid.clone(), trade.clone());
                
                // Update position
                self.update_position(&trade);
                
                // Call strategy callback
                if let Some(strategy) = &mut self.strategy {
                    strategy.on_trade(&trade);
                }

                // Mark for removal
                to_remove.push(vt_orderid.clone());
            }
        }

        // Remove crossed orders
        for vt_orderid in to_remove {
            self.active_limit_orders.remove(&vt_orderid);
        }
    }

    /// Cross stop orders with tick data
    fn cross_stop_order_tick(&mut self, tick: &TickData) {
        let mut to_trigger = Vec::new();
        
        for (stop_orderid, stop_order) in self.active_stop_orders.iter() {
            let should_trigger = match stop_order.direction {
                Direction::Long => tick.last_price >= stop_order.price,
                Direction::Short => tick.last_price <= stop_order.price,
                _ => false,
            };

            if should_trigger {
                to_trigger.push((stop_orderid.clone(), stop_order.clone()));
            }
        }

        // Trigger stop orders
        for (stop_orderid, mut stop_order) in to_trigger {
            // Update stop order status
            stop_order.status = StopOrderStatus::Triggered;
            
            // Send limit order
            let order_req = OrderRequest {
                symbol: stop_order.vt_symbol.split('.').next().unwrap().to_string(),
                exchange: self.exchange,
                direction: stop_order.direction,
                order_type: stop_order.order_type,
                volume: stop_order.volume,
                price: stop_order.price,
                offset: stop_order.offset.unwrap_or(Offset::Open),
                reference: format!("STOP_{}", stop_order.strategy_name),
            };

            let vt_orderid = self.send_limit_order(order_req);
            stop_order.vt_orderid = Some(vt_orderid);
            
            // Update stop order
            self.stop_orders.insert(stop_orderid.clone(), stop_order);
            self.active_stop_orders.remove(&stop_orderid);
        }
    }

    /// Update position from trade
    fn update_position(&mut self, trade: &TradeData) {
        match trade.direction {
            Some(Direction::Long) => {
                if trade.offset == Offset::Open {
                    self.pos += trade.volume;
                } else {
                    self.pos -= trade.volume;
                }
            }
            Some(Direction::Short) => {
                if trade.offset == Offset::Open {
                    self.pos -= trade.volume;
                } else {
                    self.pos += trade.volume;
                }
            }
            _ => {}
        }
    }

    /// Send limit order (called by strategy)
    pub fn send_limit_order(&mut self, req: OrderRequest) -> String {
        self.limit_order_count += 1;
        let vt_orderid = format!("BACKTEST_{}", self.limit_order_count);

        let order = OrderData {
            gateway_name: "BACKTESTING".to_string(),
            symbol: req.symbol,
            exchange: req.exchange,
            orderid: vt_orderid.clone(),
            order_type: req.order_type,
            direction: Some(req.direction),
            offset: req.offset,
            price: req.price,
            volume: req.volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: Some(self.current_dt),
            reference: req.reference,
            extra: None,
        };

        self.limit_orders.insert(vt_orderid.clone(), order.clone());
        self.active_limit_orders.insert(vt_orderid.clone(), order);

        vt_orderid
    }

    /// Send stop order (called by strategy)
    pub fn send_stop_order(&mut self, req: OrderRequest) -> String {
        self.stop_order_count += 1;
        let stop_orderid = format!("STOP_{}", self.stop_order_count);

        let stop_order = StopOrder {
            stop_orderid: stop_orderid.clone(),
            vt_symbol: format!("{}.{}", req.symbol, req.exchange),
            direction: req.direction,
            offset: Some(req.offset),
            price: req.price,
            volume: req.volume,
            order_type: req.order_type,
            strategy_name: req.reference.clone(),
            lock: false,
            vt_orderid: None,
            status: StopOrderStatus::Waiting,
            datetime: self.current_dt,
        };

        self.stop_orders.insert(stop_orderid.clone(), stop_order.clone());
        self.active_stop_orders.insert(stop_orderid.clone(), stop_order);

        stop_orderid
    }

    /// Cancel order (called by strategy)
    pub fn cancel_order(&mut self, vt_orderid: &str) {
        if self.active_limit_orders.contains_key(vt_orderid) {
            self.active_limit_orders.remove(vt_orderid);
        }
    }

    /// Calculate backtesting result
    pub fn calculate_result(&self) -> BacktestingResult {
        let mut result = BacktestingResult::new(self.capital);
        result.daily_results = self.daily_results.clone();
        
        // Calculate end capital
        let total_pnl: f64 = self.daily_results.values().map(|r| r.net_pnl).sum();
        result.end_capital = self.capital + total_pnl;
        
        result
    }

    /// Calculate statistics
    pub fn calculate_statistics(&self, output: bool) -> BacktestingStatistics {
        let stats = calculate_statistics(
            &self.daily_results,
            self.capital,
            0.0, // risk_free
            252, // annual_days
        );

        if output {
            self.output_statistics(&stats);
        }

        stats
    }

    /// Output statistics
    fn output_statistics(&self, stats: &BacktestingStatistics) {
        self.write_log("\n============= 回测统计 =============");
        self.write_log(&format!("起始日期: {}", stats.start_date));
        self.write_log(&format!("结束日期: {}", stats.end_date));
        self.write_log(&format!("总交易日: {}", stats.total_days));
        self.write_log(&format!("盈利天数: {}", stats.profit_days));
        self.write_log(&format!("亏损天数: {}", stats.loss_days));
        self.write_log(&format!("期末资金: {:.2}", stats.end_balance));
        self.write_log(&format!("总收益: {:.2}", stats.total_net_pnl));
        self.write_log(&format!("最大回撤: {:.2} ({:.2}%)", stats.max_drawdown, stats.max_drawdown_percent));
        self.write_log(&format!("夏普比率: {:.4}", stats.sharpe_ratio));
        self.write_log(&format!("年化收益: {:.2}%", stats.return_mean * 100.0));
        self.write_log(&format!("总成交笔数: {}", stats.total_trade_count));
        self.write_log(&format!("总手续费: {:.2}", stats.total_commission));
        self.write_log(&format!("总滑点: {:.2}", stats.total_slippage));
        self.write_log("====================================\n");
    }

    /// Write log
    fn write_log(&self, msg: &str) {
        tracing::info!("[BACKTEST] {}", msg);
        // In production, also save to logs vector
    }

    /// Get current position
    pub fn get_position(&self) -> f64 {
        self.pos
    }

    /// Get logs
    pub fn get_logs(&self) -> &[String] {
        &self.logs
    }

    /// Get vt_symbol
    pub fn get_vt_symbol(&self) -> &str {
        &self.vt_symbol
    }
}

impl Default for BacktestingEngine {
    fn default() -> Self {
        Self::new()
    }
}
