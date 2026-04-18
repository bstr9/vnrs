//! Backtesting Engine
//! 
//! Core backtesting engine that integrates with strategy framework
//! Supports both spot and futures trading
//!
//! Event loop order (following nautilus_trader to prevent look-ahead bias):
//! 1. Bar arrives → update current_dt
//! 2. Handle new day (daily result tracking)
//! 3. Cross pending limit orders from PREVIOUS bar against current bar
//! 4. Cross pending stop orders from PREVIOUS bar against current bar
//! 5. THEN call strategy's on_bar() (strategy can place new orders,
//!    but they won't be evaluated until NEXT bar)

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc, NaiveDate, Duration};

use crate::trader::{
    TickData, BarData, OrderData, TradeData,
    Direction, Offset, Status, Interval, Exchange,
    OrderRequest,
};
use crate::strategy::{
    StrategyTemplate, StrategyContext,
    StopOrder, StopOrderStatus,
};
use super::base::{BacktestingMode, DailyResult, BacktestingResult, BacktestingStatistics};
use super::statistics::calculate_statistics;
use super::database::DatabaseLoader;
use super::position::Position;
use super::fill_model::{FillModel, BestPriceFillModel};
use super::risk_engine::{RiskEngine, RiskConfig};

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
    slippage: f64,      // Slippage per share/contract (kept for DailyResult calculation)
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
    
    // Position tracking (enhanced with avg price, realized PnL, flip handling)
    position: Position,
    
    // Fill model for realistic order fill simulation
    fill_model: Box<dyn FillModel>,
    
    // Pre-trade risk engine
    risk_engine: RiskEngine,
    
    // Daily results
    daily_results: HashMap<NaiveDate, DailyResult>,
    daily_result: Option<DailyResult>,
    
    // Logging
    logs: Vec<String>,
    
    // Statistics parameters
    risk_free: f64,
    annual_days: u32,
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
            position: Position::new(
                Position::generate_position_id("", Exchange::Binance, 0),
                String::new(),
                Exchange::Binance,
            ),
            fill_model: Box::new(BestPriceFillModel::new(0.0)),
            risk_engine: RiskEngine::new_unrestricted(),
            daily_results: HashMap::new(),
            daily_result: None,
            logs: Vec::new(),
            risk_free: 0.0,
            annual_days: 252,
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
        
        self.position.reset();
        self.daily_results.clear();
        self.daily_result = None;
        
        self.logs.clear();
        self.history_data.clear();
    }

    /// Set backtesting parameters
    #[allow(clippy::too_many_arguments)]
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
                other => {
                    tracing::warn!("Unknown exchange '{}', defaulting to Binance", other);
                    Exchange::Binance
                }
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

        // Re-initialize position with correct symbol/exchange
        self.position = Position::new(
            Position::generate_position_id(&self.symbol, self.exchange, 0),
            self.symbol.clone(),
            self.exchange,
        ).with_size_multiplier(size);
    }

    /// Set fill model for order fill simulation
    pub fn set_fill_model(&mut self, model: Box<dyn FillModel>) {
        self.fill_model = model;
    }

    /// Set risk engine configuration
    pub fn set_risk_config(&mut self, config: RiskConfig) {
        self.risk_engine.set_config(config);
    }

    /// Add strategy to backtesting engine
    pub fn add_strategy(&mut self, strategy: Box<dyn StrategyTemplate>) {
        self.strategy = Some(strategy);
    }

    /// Load historical data from CSV file or database
    pub async fn load_data(&mut self) -> Result<(), String> {
        self.write_log("开始加载历史数据");

        if self.start >= self.end {
            return Err("起始日期必须小于结束日期".to_string());
        }

        let data_dir = std::path::Path::new(".data");
        let csv_path = data_dir.join(format!(
            "{}_{}_{}.csv",
            self.symbol.to_lowercase(),
            self.exchange.value().to_lowercase(),
            self.interval.value()
        ));

        if csv_path.exists() {
            self.write_log(&format!("从CSV文件加载: {:?}", csv_path));
            let content = std::fs::read_to_string(&csv_path)
                .map_err(|e| format!("读取CSV失败: {}", e))?;

            let mut bars = Vec::new();
            for (i, line) in content.lines().enumerate() {
                if i == 0 { continue; }
                let fields: Vec<&str> = line.split(',').collect();
                if fields.len() < 7 { continue; }

                let datetime = fields[0].parse::<DateTime<Utc>>()
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(fields[0], "%Y-%m-%d %H:%M:%S")
                            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
                    })
                    .or_else(|_| {
                        chrono::NaiveDate::parse_from_str(fields[0], "%Y-%m-%d")
                            .map(|d| d.and_hms_opt(0, 0, 0).unwrap_or_default())
                            .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
                    });

                let Ok(dt) = datetime else { continue };

                if dt < self.start || dt > self.end { continue; }

                bars.push(BarData {
                    gateway_name: "CSV".to_string(),
                    symbol: self.symbol.clone(),
                    exchange: self.exchange,
                    datetime: dt,
                    interval: Some(self.interval),
                    open_price: fields[1].parse().unwrap_or(0.0),
                    high_price: fields[2].parse().unwrap_or(0.0),
                    low_price: fields[3].parse().unwrap_or(0.0),
                    close_price: fields[4].parse().unwrap_or(0.0),
                    volume: fields[5].parse().unwrap_or(0.0),
                    turnover: fields.get(6).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    open_interest: fields.get(7).and_then(|v| v.parse().ok()).unwrap_or(0.0),
                    extra: None,
                });
            }

            self.write_log(&format!("从CSV加载{}条Bar数据", bars.len()));
            self.history_data = bars;
        } else {
            self.write_log("未找到CSV数据文件，使用空数据集");
        }

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
        if self.history_data.is_empty() && self.tick_data.is_empty() {
            return Err("历史数据为空，无法开始回测".to_string());
        }

        if self.strategy.is_none() {
            return Err("未设置策略".to_string());
        }

        self.write_log("开始运行回测");

        // Clone context for strategy callbacks
        let context = Arc::clone(&self.strategy_context);

        // Initialize strategy
        let pending_init = if let Some(strategy) = &mut self.strategy {
            strategy.on_init(&context);
            strategy.drain_pending_orders()
        } else {
            Vec::new()
        };
        for req in pending_init {
            self.send_limit_order(req);
        }
        
        if let Some(strategy) = &mut self.strategy {
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
    ///
    /// Event loop order per bar (prevents look-ahead bias):
    /// 1. Update current_dt
    /// 2. Handle new day
    /// 3. Cross pending limit orders (placed on PREVIOUS bar)
    /// 4. Cross pending stop orders (placed on PREVIOUS bar)
    /// 5. Call strategy on_bar() - new orders placed here are evaluated on NEXT bar
    async fn run_bar_backtesting(&mut self) -> Result<(), String> {
        let context = Arc::clone(&self.strategy_context);

        // Take ownership to avoid borrow conflicts while mutating self
        let history_data = std::mem::take(&mut self.history_data);
        
        for bar in &history_data {
            // 1. Update current time
            self.current_dt = bar.datetime;
            
            // 2. New day - create new daily result
            self.new_day(bar);
            
            // 3. Cross pending limit orders from previous bar
            self.cross_limit_order(bar);
            
            // 4. Cross pending stop orders from previous bar
            self.cross_stop_order(bar);
            
            #[cfg(feature = "gui")]
            self.strategy_context.update_indicators(&self.vt_symbol, bar);
            
            // 5. Call strategy on_bar AFTER fills are settled
            //    Orders placed here won't be evaluated until next bar's step 3-4
            let pending = if let Some(strategy) = &mut self.strategy {
                strategy.on_bar(bar, &context);
                strategy.drain_pending_orders()
            } else {
                Vec::new()
            };
            for req in pending {
                self.send_limit_order(req);
            }
        }

        // Close last day
        if let Some(last_bar) = history_data.last() {
            self.close_day(last_bar);
        }

        // Restore history data
        self.history_data = history_data;

        Ok(())
    }

    /// Run tick-based backtesting
    ///
    /// Same look-ahead bias prevention as bar mode:
    /// Orders placed by strategy are only evaluated on the next tick.
    async fn run_tick_backtesting(&mut self) -> Result<(), String> {
        // Take ownership to avoid borrow conflicts while mutating self
        let tick_data = std::mem::take(&mut self.tick_data);

        for tick in &tick_data {
            self.current_dt = tick.datetime;

            let synthetic_bar = BarData {
                gateway_name: "BACKTESTING".to_string(),
                symbol: tick.symbol.clone(),
                exchange: tick.exchange,
                datetime: tick.datetime,
                interval: Some(Interval::Tick),
                open_price: tick.last_price,
                high_price: tick.last_price,
                low_price: tick.last_price,
                close_price: tick.last_price,
                volume: tick.volume,
                turnover: tick.turnover,
                open_interest: tick.open_interest,
                extra: None,
            };
            self.new_day(&synthetic_bar);

            // Cross limit orders with tick data
            self.cross_limit_order_tick(tick);
            
            // Cross stop orders with tick data
            self.cross_stop_order_tick(tick);
            
            #[cfg(feature = "gui")]
            self.strategy_context.update_indicators(&self.vt_symbol, &synthetic_bar);
            
            // Call strategy on_tick AFTER fills are settled
            let pending = if let Some(strategy) = &mut self.strategy {
                let ctx = Arc::clone(&self.strategy_context);
                strategy.on_tick(tick, &ctx);
                strategy.drain_pending_orders()
            } else {
                Vec::new()
            };
            for req in pending {
                self.send_limit_order(req);
            }
        }

        // Calculate final result based on last tick
        if let Some(last_tick) = tick_data.last() {
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

        // Restore tick data
        self.tick_data = tick_data;

        Ok(())
    }

    /// Handle new day
    fn new_day(&mut self, bar: &BarData) {
        let bar_date = bar.datetime.date_naive();
        
        // Check if it's a new day
        if let Some(ref mut result) = self.daily_result {
            let result_date = result.date;
            
            if bar_date != result_date {
                // Reset daily risk counters for new day
                self.risk_engine.reset_daily();
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
                    result.start_pos = self.position.signed_qty();
                }
            }
        } else {
            // First day
            self.daily_result = Some(DailyResult::new(bar_date, bar.close_price));
            if let Some(result) = &mut self.daily_result {
                result.start_pos = self.position.signed_qty();
            }
        }
    }

    /// Close day and save result
    fn close_day(&mut self, bar: &BarData) {
        if let Some(mut result) = self.daily_result.take() {
            result.close_price = bar.close_price;
            result.end_pos = self.position.signed_qty();
            
            // Calculate PnL
            result.calculate_pnl(self.size, self.rate, self.slippage);
            
            // Save result
            self.daily_results.insert(result.date, result);
        }
    }

    /// Cross limit orders with bar using FillModel
    fn cross_limit_order(&mut self, bar: &BarData) {
        // Collect active orders to avoid borrow issues
        let active_orders: Vec<_> = self.active_limit_orders.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        let mut to_remove = Vec::new();

        for (vt_orderid, order) in active_orders {
            // Use FillModel to simulate the fill
            let result = self.fill_model.simulate_limit_fill(&order, bar);

            if result.filled {
                // Create trade from FillResult
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: order.symbol.clone(),
                    exchange: order.exchange,
                    orderid: order.orderid.clone(),
                    tradeid: format!("{}", self.trade_count),
                    direction: order.direction,
                    offset: order.offset,
                    price: result.fill_price,
                    volume: result.fill_qty,  // GAP 2 fix: use fill_qty, not order.volume
                    datetime: Some(bar.datetime),
                    extra: None,
                };

                // Save trade
                let vt_tradeid = trade.vt_tradeid();
                self.trades.insert(vt_tradeid.clone(), trade.clone());
                
                // Update position using apply_fill
                self.position.apply_fill(&trade)
                    .expect("Position apply_fill failed");
                
                // Record trade in risk engine
                self.risk_engine.record_trade(trade.price * trade.volume * self.size);
                
                // Update daily result
                if let Some(result) = &mut self.daily_result {
                    result.trades.push(trade.clone());
                    result.trade_count += 1;
                }

                // Call strategy callback
                if let Some(strategy) = &mut self.strategy {
                    strategy.on_trade(&trade);
                    // Sync position cache so get_pos() / self.pos works
                    // without calling engine.get_pos() (which would deadlock)
                    strategy.update_position(&self.vt_symbol, self.position.signed_qty());
                }

                // Handle partial fills: if fill_qty < order volume, keep order active
                let remaining = order.volume - order.traded - result.fill_qty;
                if remaining > 1e-10 {
                    // Partially filled - update order's traded amount and keep active
                    if let Some(active_order) = self.active_limit_orders.get_mut(&vt_orderid) {
                        active_order.traded += result.fill_qty;
                    }
                } else {
                    // Fully filled - mark for removal
                    to_remove.push(vt_orderid.clone());
                }
            }
        }

        // Remove crossed orders
        for vt_orderid in to_remove {
            self.active_limit_orders.remove(&vt_orderid);
        }
    }

    /// Cross stop orders with bar using FillModel
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

        for (stop_orderid, mut stop_order) in to_trigger {
            stop_order.status = StopOrderStatus::Triggered;

            // Create a synthetic OrderData to pass to FillModel
            let order = OrderData {
                gateway_name: "BACKTESTING".to_string(),
                symbol: crate::trader::utility::extract_vt_symbol(&stop_order.vt_symbol)
                    .map(|(s, _)| s)
                    .unwrap_or_else(|| stop_order.vt_symbol.split('.').next().unwrap_or("").to_string()),
                exchange: self.exchange,
                orderid: stop_orderid.clone(),
                order_type: stop_order.order_type,
                direction: Some(stop_order.direction),
                offset: stop_order.offset.unwrap_or(Offset::Open),
                price: stop_order.price,
                volume: stop_order.volume,
                traded: 0.0,
                status: Status::NotTraded,
                datetime: Some(bar.datetime),
                reference: String::new(),
                extra: None,
            };

            // Use FillModel to simulate the stop fill with trigger price
            let fill_result = self.fill_model.simulate_stop_fill(&order, bar, stop_order.price);

            if fill_result.filled {
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: order.symbol.clone(),
                    exchange: self.exchange,
                    orderid: stop_orderid.clone(),
                    tradeid: format!("{}", self.trade_count),
                    direction: Some(stop_order.direction),
                    offset: stop_order.offset.unwrap_or(Offset::Open),
                    price: fill_result.fill_price,
                    volume: fill_result.fill_qty,  // GAP 2 fix: use fill_qty, not stop_order.volume
                    datetime: Some(bar.datetime),
                    extra: None,
                };

                let vt_tradeid = trade.vt_tradeid();
                self.trades.insert(vt_tradeid.clone(), trade.clone());
                
                // Update position using apply_fill
                self.position.apply_fill(&trade)
                    .expect("Position apply_fill failed");

                // Record trade in risk engine
                self.risk_engine.record_trade(trade.price * trade.volume * self.size);

                if let Some(result) = &mut self.daily_result {
                    result.trades.push(trade.clone());
                    result.trade_count += 1;
                }

                if let Some(strategy) = &mut self.strategy {
                    strategy.on_trade(&trade);
                    // Sync position cache so get_pos() / self.pos works
                    strategy.update_position(&self.vt_symbol, self.position.signed_qty());
                    // Notify strategy that stop order was triggered
                    strategy.on_stop_order(&stop_orderid);
                }

                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            } else {
                // Not filled but still triggered — notify strategy anyway
                if let Some(strategy) = &mut self.strategy {
                    strategy.on_stop_order(&stop_orderid);
                }

                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            }
        }
    }

    /// Cross limit orders with tick data using FillModel
    fn cross_limit_order_tick(&mut self, tick: &TickData) {
        // Collect active orders to avoid borrow issues
        let active_orders: Vec<_> = self.active_limit_orders.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        
        let mut to_remove = Vec::new();

        for (vt_orderid, order) in active_orders {
            // Use FillModel to simulate tick fill
            let result = self.fill_model.simulate_tick_fill(&order, tick);

            if result.filled {
                // Create trade from FillResult
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: order.symbol.clone(),
                    exchange: order.exchange,
                    orderid: order.orderid.clone(),
                    tradeid: format!("{}", self.trade_count),
                    direction: order.direction,
                    offset: order.offset,
                    price: result.fill_price,
                    volume: result.fill_qty,  // GAP 2 fix: use fill_qty, not order.volume
                    datetime: Some(tick.datetime),
                    extra: None,
                };

                // Save trade
                let vt_tradeid = trade.vt_tradeid();
                self.trades.insert(vt_tradeid.clone(), trade.clone());
                
                // Update position using apply_fill
                self.position.apply_fill(&trade)
                    .expect("Position apply_fill failed");

                // Record trade in risk engine
                self.risk_engine.record_trade(trade.price * trade.volume * self.size);

                // Update daily result
                if let Some(result) = &mut self.daily_result {
                    result.trades.push(trade.clone());
                    result.trade_count += 1;
                }

                // Call strategy callback
                if let Some(strategy) = &mut self.strategy {
                    strategy.on_trade(&trade);
                    // Sync position cache so get_pos() / self.pos works
                    strategy.update_position(&self.vt_symbol, self.position.signed_qty());
                }

                // Handle partial fills: if fill_qty < order volume, keep order active
                let remaining = order.volume - order.traded - result.fill_qty;
                if remaining > 1e-10 {
                    // Partially filled - update order's traded amount and keep active
                    if let Some(active_order) = self.active_limit_orders.get_mut(&vt_orderid) {
                        active_order.traded += result.fill_qty;
                    }
                } else {
                    // Fully filled - mark for removal
                    to_remove.push(vt_orderid.clone());
                }
            }
        }

        // Remove crossed orders
        for vt_orderid in to_remove {
            self.active_limit_orders.remove(&vt_orderid);
        }
    }

    /// Cross stop orders with tick data using FillModel
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

        for (stop_orderid, mut stop_order) in to_trigger {
            stop_order.status = StopOrderStatus::Triggered;

            // Create a synthetic OrderData to pass to FillModel
            let order = OrderData {
                gateway_name: "BACKTESTING".to_string(),
                symbol: crate::trader::utility::extract_vt_symbol(&stop_order.vt_symbol)
                    .map(|(s, _)| s)
                    .unwrap_or_else(|| stop_order.vt_symbol.split('.').next().unwrap_or("").to_string()),
                exchange: self.exchange,
                orderid: stop_orderid.clone(),
                order_type: stop_order.order_type,
                direction: Some(stop_order.direction),
                offset: stop_order.offset.unwrap_or(Offset::Open),
                price: stop_order.price,
                volume: stop_order.volume,
                traded: 0.0,
                status: Status::NotTraded,
                datetime: Some(tick.datetime),
                reference: String::new(),
                extra: None,
            };

            // For tick data, use simulate_tick_fill for stop orders
            let fill_result = self.fill_model.simulate_tick_fill(&order, tick);

            if fill_result.filled {
                self.trade_count += 1;
                let trade = TradeData {
                    gateway_name: "BACKTESTING".to_string(),
                    symbol: order.symbol.clone(),
                    exchange: self.exchange,
                    orderid: stop_orderid.clone(),
                    tradeid: format!("{}", self.trade_count),
                    direction: Some(stop_order.direction),
                    offset: stop_order.offset.unwrap_or(Offset::Open),
                    price: fill_result.fill_price,
                    volume: fill_result.fill_qty,  // GAP 2 fix: use fill_qty, not stop_order.volume
                    datetime: Some(tick.datetime),
                    extra: None,
                };

                let vt_tradeid = trade.vt_tradeid();
                self.trades.insert(vt_tradeid.clone(), trade.clone());
                
                // Update position using apply_fill
                self.position.apply_fill(&trade)
                    .expect("Position apply_fill failed");

                // Record trade in risk engine
                self.risk_engine.record_trade(trade.price * trade.volume * self.size);

                if let Some(result) = &mut self.daily_result {
                    result.trades.push(trade.clone());
                    result.trade_count += 1;
                }

                if let Some(strategy) = &mut self.strategy {
                    strategy.on_trade(&trade);
                    // Sync position cache so get_pos() / self.pos works
                    strategy.update_position(&self.vt_symbol, self.position.signed_qty());
                    // Notify strategy that stop order was triggered
                    strategy.on_stop_order(&stop_orderid);
                }

                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            } else {
                // Not filled but still triggered — notify strategy anyway
                if let Some(strategy) = &mut self.strategy {
                    strategy.on_stop_order(&stop_orderid);
                }

                stop_order.vt_orderid = Some(stop_orderid.clone());
                self.stop_orders.insert(stop_orderid.clone(), stop_order);
                self.active_stop_orders.remove(&stop_orderid);
            }
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

        // Pre-trade risk check
        let active_order_count = self.active_limit_orders.len() + self.active_stop_orders.len();
        let risk_result = self.risk_engine.check_order(
            &order, &self.position, active_order_count, self.size,
        );
        if !risk_result.is_approved {
            self.write_log(&format!(
                "Order rejected by risk engine: {}",
                risk_result.reason.as_deref().unwrap_or("unknown")
            ));
            return String::new();
        }

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

        // Pre-trade risk check - create a synthetic OrderData for the check
        let order_for_check = OrderData {
            gateway_name: "BACKTESTING".to_string(),
            symbol: req.symbol,
            exchange: req.exchange,
            orderid: stop_orderid.clone(),
            order_type: req.order_type,
            direction: Some(req.direction),
            offset: req.offset,
            price: req.price,
            volume: req.volume,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: Some(self.current_dt),
            reference: String::new(),
            extra: None,
        };
        let active_order_count = self.active_limit_orders.len() + self.active_stop_orders.len();
        let risk_result = self.risk_engine.check_order(
            &order_for_check, &self.position, active_order_count, self.size,
        );
        if !risk_result.is_approved {
            self.write_log(&format!(
                "Stop order rejected by risk engine: {}",
                risk_result.reason.as_deref().unwrap_or("unknown")
            ));
            return String::new();
        }

        self.stop_orders.insert(stop_orderid.clone(), stop_order.clone());
        self.active_stop_orders.insert(stop_orderid.clone(), stop_order);

        stop_orderid
    }

    /// Cancel order (called by strategy)
    pub fn cancel_order(&mut self, vt_orderid: &str) {
        if self.active_limit_orders.contains_key(vt_orderid) {
            self.active_limit_orders.remove(vt_orderid);
        } else if self.active_stop_orders.contains_key(vt_orderid) {
            self.active_stop_orders.remove(vt_orderid);
        }
    }

    /// Calculate backtesting result
    pub fn calculate_result(&self) -> BacktestingResult {
        let mut result = BacktestingResult::new(self.capital);
        result.daily_results = self.daily_results.clone();
        
        // Calculate end capital
        let total_pnl: f64 = self.daily_results.values().map(|r| r.net_pnl).sum();
        result.end_capital = self.capital + total_pnl;

        // Populate result fields from statistics
        let stats = self.calculate_statistics(false);
        result.total_return = if self.capital > 0.0 {
            (result.end_capital - self.capital) / self.capital
        } else {
            0.0
        };
        result.annual_return = stats.return_mean;
        result.max_drawdown = stats.max_drawdown;
        result.max_drawdown_percent = stats.max_drawdown_percent;
        result.sharpe_ratio = stats.sharpe_ratio;
        result.total_trade_count = stats.total_trade_count;
        result.total_days = stats.total_days;
        result.profit_days = stats.profit_days;
        result.loss_days = stats.loss_days;
        result.total_commission = stats.total_commission;
        result.total_slippage = stats.total_slippage;
        result.total_turnover = stats.total_turnover;
        
        result
    }

    /// Calculate statistics
    pub fn calculate_statistics(&self, output: bool) -> BacktestingStatistics {
        let stats = calculate_statistics(
            &self.daily_results,
            self.capital,
            self.risk_free,
            self.annual_days,
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
        self.write_log(&format!("索提诺比率: {:.4}", stats.sortino_ratio));
        self.write_log(&format!("卡尔玛比率: {:.4}", stats.calmar_ratio));
        self.write_log(&format!("年化收益: {:.2}%", stats.return_mean * 100.0));
        self.write_log(&format!("总成交笔数: {}", stats.total_trade_count));
        self.write_log(&format!("胜率: {:.2}%", stats.win_rate * 100.0));
        self.write_log(&format!("盈亏比: {:.4}", stats.profit_factor));
        self.write_log(&format!("平均每笔盈亏: {:.2}", stats.avg_trade_pnl));
        self.write_log(&format!("最大连胜次数: {}", stats.max_consecutive_wins));
        self.write_log(&format!("最大连亏次数: {}", stats.max_consecutive_losses));
        self.write_log(&format!("总手续费: {:.2}", stats.total_commission));
        self.write_log(&format!("总滑点: {:.2}", stats.total_slippage));
        self.write_log("====================================\n");
    }

    /// Write log
    fn write_log(&self, msg: &str) {
        tracing::info!("[BACKTEST] {}", msg);
        // In production, also save to logs vector
    }

    /// Get current position (enhanced Position reference)
    pub fn get_position(&self) -> &Position {
        &self.position
    }

    /// Get current position as simple signed quantity (backward compatibility)
    pub fn get_pos(&self) -> f64 {
        self.position.signed_qty()
    }

    /// Get realized PnL from position
    pub fn get_realized_pnl(&self) -> f64 {
        self.position.realized_pnl()
    }

    /// Get logs
    pub fn get_logs(&self) -> &[String] {
        &self.logs
    }

    /// Get vt_symbol
    pub fn get_vt_symbol(&self) -> &str {
        &self.vt_symbol
    }

    /// Set risk-free rate for Sharpe ratio calculation
    pub fn set_risk_free(&mut self, risk_free: f64) {
        self.risk_free = risk_free;
    }

    /// Set annual trading days for Sharpe ratio calculation
    pub fn set_annual_days(&mut self, annual_days: u32) {
        self.annual_days = annual_days;
    }
}

impl Default for BacktestingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{OrderType, OrderRequest};
    use chrono::{TimeZone, NaiveDate};

    #[test]
    fn test_engine_new_default_state() {
        let engine = BacktestingEngine::new();
        assert_eq!(engine.vt_symbol, "");
        assert_eq!(engine.symbol, "");
        assert_eq!(engine.exchange, Exchange::Binance);
        assert_eq!(engine.interval, Interval::Minute);
        assert!((engine.rate - 0.0).abs() < 1e-10);
        assert!((engine.slippage - 0.0).abs() < 1e-10);
        assert!((engine.size - 1.0).abs() < 1e-10);
        assert!((engine.pricetick - 0.01).abs() < 1e-10);
        assert!((engine.capital - 1_000_000.0).abs() < 1e-10);
        assert_eq!(engine.mode, BacktestingMode::Bar);
        assert!(engine.strategy.is_none());
        assert!(engine.history_data.is_empty());
        assert!(engine.limit_orders.is_empty());
        assert!(engine.active_limit_orders.is_empty());
        assert!(engine.stop_orders.is_empty());
        assert!(engine.active_stop_orders.is_empty());
        assert!(engine.trades.is_empty());
        assert!((engine.get_pos() - 0.0).abs() < 1e-10);
        assert!(engine.daily_results.is_empty());
        assert!(engine.daily_result.is_none());
    }

    #[test]
    fn test_engine_default_trait() {
        let engine = BacktestingEngine::default();
        assert!((engine.capital - 1_000_000.0).abs() < 1e-10);
    }

    #[test]
    fn test_set_parameters() {
        let mut engine = BacktestingEngine::new();
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2024, 12, 31, 23, 59, 59).unwrap();

        engine.set_parameters(
            "BTCUSDT.BINANCE".to_string(),
            Interval::Minute,
            start,
            end,
            0.001,
            0.5,
            1.0,
            0.01,
            100_000.0,
            BacktestingMode::Bar,
        );

        assert_eq!(engine.vt_symbol, "BTCUSDT.BINANCE");
        assert_eq!(engine.symbol, "BTCUSDT");
        assert_eq!(engine.exchange, Exchange::Binance);
        assert_eq!(engine.interval, Interval::Minute);
        assert!((engine.rate - 0.001).abs() < 1e-10);
        assert!((engine.slippage - 0.5).abs() < 1e-10);
        assert!((engine.capital - 100_000.0).abs() < 1e-10);
        assert_eq!(engine.mode, BacktestingMode::Bar);
    }

    #[test]
    fn test_clear_data() {
        let mut engine = BacktestingEngine::new();
        engine.limit_order_count = 5;
        engine.stop_order_count = 3;
        engine.trade_count = 2;

        engine.clear_data();

        assert_eq!(engine.limit_order_count, 0);
        assert!(engine.limit_orders.is_empty());
        assert!(engine.active_limit_orders.is_empty());
        assert_eq!(engine.stop_order_count, 0);
        assert!(engine.stop_orders.is_empty());
        assert!(engine.active_stop_orders.is_empty());
        assert_eq!(engine.trade_count, 0);
        assert!(engine.trades.is_empty());
        assert!((engine.get_pos() - 0.0).abs() < 1e-10);
        assert!(engine.daily_results.is_empty());
        assert!(engine.daily_result.is_none());
        assert!(engine.history_data.is_empty());
    }

    #[test]
    fn test_send_limit_order() {
        let mut engine = BacktestingEngine::new();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );

        let orderid = engine.send_limit_order(req);
        assert!(orderid.starts_with("BACKTEST_"));
        assert_eq!(engine.limit_order_count, 1);
        assert!(engine.limit_orders.contains_key(&orderid));
        assert!(engine.active_limit_orders.contains_key(&orderid));

        let order = &engine.limit_orders[&orderid];
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.direction, Some(Direction::Long));
        assert_eq!(order.status, Status::NotTraded);
    }

    #[test]
    fn test_send_stop_order() {
        let mut engine = BacktestingEngine::new();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Stop,
            1.0,
        );

        let stop_orderid = engine.send_stop_order(req);
        assert!(stop_orderid.starts_with("STOP_"));
        assert_eq!(engine.stop_order_count, 1);
        assert!(engine.stop_orders.contains_key(&stop_orderid));
        assert!(engine.active_stop_orders.contains_key(&stop_orderid));

        let stop_order = &engine.stop_orders[&stop_orderid];
        assert_eq!(stop_order.direction, Direction::Long);
        assert_eq!(stop_order.status, StopOrderStatus::Waiting);
    }

    #[test]
    fn test_cancel_limit_order() {
        let mut engine = BacktestingEngine::new();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );

        let orderid = engine.send_limit_order(req);
        assert!(engine.active_limit_orders.contains_key(&orderid));

        engine.cancel_order(&orderid);
        assert!(!engine.active_limit_orders.contains_key(&orderid));
        // Order should still be in the full orders map
        assert!(engine.limit_orders.contains_key(&orderid));
    }

    #[test]
    fn test_cancel_stop_order() {
        let mut engine = BacktestingEngine::new();
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Stop,
            1.0,
        );

        let stop_orderid = engine.send_stop_order(req);
        assert!(engine.active_stop_orders.contains_key(&stop_orderid));

        engine.cancel_order(&stop_orderid);
        assert!(!engine.active_stop_orders.contains_key(&stop_orderid));
    }

    #[test]
    fn test_cancel_nonexistent_order() {
        let mut engine = BacktestingEngine::new();
        // Should not panic when cancelling non-existent order
        engine.cancel_order("NONEXISTENT");
    }

    #[test]
    fn test_get_position() {
        let engine = BacktestingEngine::new();
        assert!(engine.get_position().is_flat());
        assert!((engine.get_pos() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_update_position_long_open() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        let trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            datetime: Some(Utc::now()),
            extra: None,
        };
        engine.position.apply_fill(&trade).expect("apply_fill failed");
        assert!((engine.get_pos() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_update_position_long_close() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        let open_trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            datetime: Some(Utc::now()),
            extra: None,
        };
        engine.position.apply_fill(&open_trade).expect("apply_fill failed");
        let close_trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "2".to_string(),
            tradeid: "2".to_string(),
            direction: Some(Direction::Short),  // Sell to close long
            offset: Offset::Close,
            price: 51000.0,
            volume: 1.0,
            datetime: Some(Utc::now()),
            extra: None,
        };
        engine.position.apply_fill(&close_trade).expect("apply_fill failed");
        assert!(engine.position.is_flat());
    }

    #[test]
    fn test_update_position_short_open() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        let trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Short),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            datetime: Some(Utc::now()),
            extra: None,
        };
        engine.position.apply_fill(&trade).expect("apply_fill failed");
        assert!((engine.get_pos() - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_update_position_short_close() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        let open_trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "1".to_string(),
            tradeid: "1".to_string(),
            direction: Some(Direction::Short),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            datetime: Some(Utc::now()),
            extra: None,
        };
        engine.position.apply_fill(&open_trade).expect("apply_fill failed");
        let close_trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "2".to_string(),
            tradeid: "2".to_string(),
            direction: Some(Direction::Long),  // Buy to close short
            offset: Offset::Close,
            price: 49000.0,
            volume: 1.0,
            datetime: Some(Utc::now()),
            extra: None,
        };
        engine.position.apply_fill(&close_trade).expect("apply_fill failed");
        assert!(engine.position.is_flat());
    }

    #[test]
    fn test_cross_limit_order_buy() {
        let mut engine = BacktestingEngine::new();
        engine.slippage = 0.1;
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Place a buy limit order at 50000
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let orderid = engine.send_limit_order(req);

        // Create a bar where low_price <= order price (should cross)
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50100.0,
            high_price: 50200.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.cross_limit_order(&bar);

        // Order should be removed from active list
        assert!(!engine.active_limit_orders.contains_key(&orderid));
        // A trade should have been recorded
        assert_eq!(engine.trade_count, 1);
        // Trade price should include slippage (50000 + 0.1)
        let trade = engine.trades.values().next().unwrap();
        assert!((trade.price - 50000.1).abs() < 1e-10);
        // Position should be updated
        assert!((engine.get_pos() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cross_limit_order_sell() {
        let mut engine = BacktestingEngine::new();
        engine.slippage = 0.1;
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Place a sell limit order at 50000
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let orderid = engine.send_limit_order(req);

        // Create a bar where high_price >= order price (should cross)
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 49900.0,
            high_price: 50100.0,
            low_price: 49800.0,
            close_price: 49950.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.cross_limit_order(&bar);

        assert!(!engine.active_limit_orders.contains_key(&orderid));
        assert_eq!(engine.trade_count, 1);
        let trade = engine.trades.values().next().unwrap();
        assert!((trade.price - 49999.9).abs() < 1e-10); // 50000 - 0.1 slippage
        assert!((engine.get_pos() - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_cross_limit_order_no_cross() {
        let mut engine = BacktestingEngine::new();

        // Place a buy limit order at 49000 (below the bar's low)
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 49000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let orderid = engine.send_limit_order(req);

        // Bar low is 49900, above our buy price of 49000 - should NOT cross
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50100.0,
            high_price: 50200.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.cross_limit_order(&bar);

        // Order should still be active
        assert!(engine.active_limit_orders.contains_key(&orderid));
        assert_eq!(engine.trade_count, 0);
    }

    #[test]
    fn test_cross_stop_order_long() {
        let mut engine = BacktestingEngine::new();
        engine.slippage = 0.1;
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Place a buy stop order at 50500
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Stop,
            volume: 1.0,
            price: 50500.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let stop_orderid = engine.send_stop_order(req);

        // Create a bar where high >= stop price (should trigger)
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50400.0,
            high_price: 50600.0,
            low_price: 50300.0,
            close_price: 50550.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.cross_stop_order(&bar);

        // Stop order should be removed from active list
        assert!(!engine.active_stop_orders.contains_key(&stop_orderid));
        // Stop order status should be Triggered
        assert_eq!(engine.stop_orders[&stop_orderid].status, StopOrderStatus::Triggered);
        // A trade should have been recorded with slippage
        assert_eq!(engine.trade_count, 1);
        let trade = engine.trades.values().next().unwrap();
        assert!((trade.price - 50550.1).abs() < 1e-10);
    }

    #[test]
    fn test_cross_stop_order_short() {
        let mut engine = BacktestingEngine::new();
        engine.slippage = 0.1;
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Place a sell stop order at 49500
        let req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Stop,
            volume: 1.0,
            price: 49500.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let stop_orderid = engine.send_stop_order(req);

        // Create a bar where low <= stop price (should trigger)
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 49600.0,
            high_price: 49700.0,
            low_price: 49400.0,
            close_price: 49450.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.cross_stop_order(&bar);

        assert!(!engine.active_stop_orders.contains_key(&stop_orderid));
        assert_eq!(engine.stop_orders[&stop_orderid].status, StopOrderStatus::Triggered);
        let trade = engine.trades.values().next().unwrap();
        assert!((trade.price - 49449.9).abs() < 1e-10);
    }

    #[test]
    fn test_new_day_creates_daily_result() {
        let mut engine = BacktestingEngine::new();
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.new_day(&bar);
        assert!(engine.daily_result.is_some());
        let result = engine.daily_result.as_ref().unwrap();
        assert_eq!(result.date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
        assert!((result.close_price - 50050.0).abs() < 1e-10);
    }

    #[test]
    fn test_close_day_saves_result() {
        let mut engine = BacktestingEngine::new();
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };

        engine.new_day(&bar);
        engine.close_day(&bar);

        assert!(engine.daily_result.is_none());
        assert_eq!(engine.daily_results.len(), 1);
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        assert!(engine.daily_results.contains_key(&date));
    }

    #[test]
    fn test_calculate_result() {
        let mut engine = BacktestingEngine::new();
        engine.capital = 100_000.0;

        // Manually insert a daily result
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let mut daily = DailyResult::new(date, 50000.0);
        daily.net_pnl = 1000.0;
        engine.daily_results.insert(date, daily);

        let result = engine.calculate_result();
        assert!((result.start_capital - 100_000.0).abs() < 1e-10);
        assert!((result.end_capital - 101_000.0).abs() < 1e-10);
    }

    #[test]
    fn test_calculate_statistics_empty() {
        let engine = BacktestingEngine::new();
        let stats = engine.calculate_statistics(false);
        // With empty daily_results, should return default stats
        assert_eq!(stats.total_days, 0);
    }

    #[test]
    fn test_set_history_data() {
        let mut engine = BacktestingEngine::new();
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

        let bars = vec![BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        }];

        engine.set_history_data(bars);
        assert_eq!(engine.history_data.len(), 1);
    }

    #[test]
    fn test_set_risk_free_and_annual_days() {
        let mut engine = BacktestingEngine::new();
        engine.set_risk_free(0.02);
        assert!((engine.risk_free - 0.02).abs() < 1e-10);

        engine.set_annual_days(365);
        assert_eq!(engine.annual_days, 365);
    }

    #[tokio::test]
    async fn test_run_backtesting_no_data() {
        let mut engine = BacktestingEngine::new();
        let result = engine.run_backtesting().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("历史数据为空"));
    }

    #[tokio::test]
    async fn test_run_backtesting_no_strategy() {
        let mut engine = BacktestingEngine::new();
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
        engine.set_history_data(vec![BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        }]);

        let result = engine.run_backtesting().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("未设置策略"));
    }

    #[tokio::test]
    async fn test_load_data_start_after_end() {
        let mut engine = BacktestingEngine::new();
        engine.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        engine.end = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        let result = engine.load_data().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("起始日期必须小于结束日期"));
    }
}
