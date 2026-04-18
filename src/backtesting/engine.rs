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
use crate::trader::OrderType;

/// Emulated order type for backtesting (order types not natively supported by exchanges)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestEmulatedType {
    /// Trailing stop with percentage distance
    TrailingStopPct,
    /// Trailing stop with absolute price distance
    TrailingStopAbs,
    /// Market-If-Touched: trigger at price, submit market order
    Mit,
    /// Limit-If-Touched: trigger at price, submit limit order
    Lit,
}

/// An emulated order tracked by the backtesting engine
#[derive(Debug, Clone)]
pub struct BacktestEmulatedOrder {
    pub id: u64,
    pub order_type: BacktestEmulatedType,
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,
    pub offset: Offset,
    pub volume: f64,
    pub trail_pct: Option<f64>,
    pub trail_abs: Option<f64>,
    pub trigger_price: Option<f64>,
    pub limit_price: Option<f64>,
    pub current_stop: Option<f64>,
    pub highest_price: Option<f64>,
    pub lowest_price: Option<f64>,
    pub is_active: bool,
}

/// Bracket order group type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestBracketType {
    Bracket,
    Oco,
    Oto,
}

/// Bracket order group state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacktestBracketState {
    Pending,
    EntryActive,
    SecondaryActive,
    Completed,
    Cancelled,
}

/// Child order in a bracket group
#[derive(Debug, Clone)]
pub struct BacktestChildOrder {
    pub role: String,
    pub vt_orderid: Option<String>,
    pub request: OrderRequest,
    pub filled_volume: f64,
    pub is_active: bool,
}

/// Bracket order group
#[derive(Debug, Clone)]
pub struct BacktestBracketGroup {
    pub id: u64,
    pub bracket_type: BacktestBracketType,
    pub state: BacktestBracketState,
    pub vt_symbol: String,
    pub children: HashMap<String, BacktestChildOrder>,
}

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
    
    // Emulated order management
    emulated_order_count: u64,
    emulated_orders: HashMap<u64, BacktestEmulatedOrder>,
    active_emulated_orders: HashMap<u64, BacktestEmulatedOrder>,

    // Bracket order management
    bracket_group_count: u64,
    bracket_groups: HashMap<u64, BacktestBracketGroup>,
    active_bracket_groups: HashMap<u64, BacktestBracketGroup>,
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
            emulated_order_count: 0,
            emulated_orders: HashMap::new(),
            active_emulated_orders: HashMap::new(),
            bracket_group_count: 0,
            bracket_groups: HashMap::new(),
            active_bracket_groups: HashMap::new(),
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

        self.emulated_order_count = 0;
        self.emulated_orders.clear();
        self.active_emulated_orders.clear();
        self.bracket_group_count = 0;
        self.bracket_groups.clear();
        self.active_bracket_groups.clear();
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

    /// Load historical data from Binance REST API directly
    ///
    /// Reads gateway config from .rstrader/binance/gateway_configs.json,
    /// constructs a REST client, downloads klines via DataDownloadManager,
    /// and feeds them into the backtesting engine.
    pub async fn load_data_from_binance(&mut self) -> Result<(), String> {
        self.write_log("从Binance REST API加载历史数据");

        if self.start >= self.end {
            return Err("起始日期必须小于结束日期".to_string());
        }

        // 1. Load gateway config from disk
        let configs = crate::gateway::binance::BinanceConfigs::load();
        let config = configs.get("BINANCE_SPOT")
            .or_else(|| configs.get("BINANCE"))
            .ok_or_else(|| "未找到Binance网关配置，请先配置API密钥".to_string())?;

        // 2. Determine REST host based on server mode
        let host = if config.server.to_uppercase() == "TESTNET" {
            crate::gateway::binance::SPOT_TESTNET_REST_HOST
        } else {
            crate::gateway::binance::SPOT_REST_HOST
        };

        // 3. Create and initialize REST client
        let rest_client = crate::gateway::binance::BinanceRestClient::new();
        rest_client.init(
            &config.key,
            &config.secret,
            host,
            &config.proxy_host,
            config.proxy_port,
        ).await;

        // 4. Download klines via DataDownloadManager
        let download_manager = crate::trader::data_download::DataDownloadManager::new();
        let bars = download_manager.download_klines(
            &rest_client,
            &self.symbol,
            self.exchange,
            self.interval,
            self.start,
            self.end,
        ).await?;

        if bars.is_empty() {
            return Err("下载的数据为空，请检查日期范围和网络连接".to_string());
        }

        // 5. Feed bars into backtesting engine
        self.history_data = bars;
        self.write_log(&format!("从Binance加载{}条Bar数据", self.history_data.len()));

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
    /// 5. Cross emulated orders (trailing stops, MIT, LIT)
    /// 6. Call strategy on_bar() - new orders placed here are evaluated on NEXT bar
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
            
            // 5. Cross emulated orders (trailing stops, MIT, LIT)
            self.cross_emulated_order(bar);
            
            #[cfg(feature = "gui")]
            self.strategy_context.update_indicators(&self.vt_symbol, bar);
            
            // 6. Call strategy on_bar AFTER fills are settled
            //    Orders placed here won't be evaluated until next bar's step 3-5
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
            
            // Cross emulated orders (trailing stops, MIT, LIT)
            self.cross_emulated_order(&synthetic_bar);
            
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

                // Process bracket order state machine
                self.process_bracket_on_trade(&trade);

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

                // Process bracket order state machine
                self.process_bracket_on_trade(&trade);

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

                // Process bracket order state machine
                self.process_bracket_on_trade(&trade);

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

                // Process bracket order state machine
                self.process_bracket_on_trade(&trade);

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

    // ========================================================================
    // Emulated order methods (trailing stops, MIT, LIT)
    // ========================================================================

    /// Send a trailing stop order with percentage distance
    ///
    /// For Long: stop ratchets up as price rises; triggers sell when price drops below stop
    /// For Short: stop ratchets down as price falls; triggers buy when price rises above stop
    pub fn send_trailing_stop_pct(
        &mut self,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        volume: f64,
        trail_pct: f64,
    ) -> u64 {
        self.emulated_order_count += 1;
        let id = self.emulated_order_count;

        // Initial stop is None - will be computed on first bar cross
        let order = BacktestEmulatedOrder {
            id,
            order_type: BacktestEmulatedType::TrailingStopPct,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: Some(trail_pct),
            trail_abs: None,
            trigger_price: None,
            limit_price: None,
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            is_active: true,
        };

        self.emulated_orders.insert(id, order.clone());
        self.active_emulated_orders.insert(id, order);

        self.write_log(&format!(
            "发送百分比追踪止损单: id={}, 方向={:?}, 回撤比例={}%",
            id, direction, trail_pct
        ));

        id
    }

    /// Send a trailing stop order with absolute price distance
    pub fn send_trailing_stop_abs(
        &mut self,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        volume: f64,
        trail_abs: f64,
    ) -> u64 {
        self.emulated_order_count += 1;
        let id = self.emulated_order_count;

        let order = BacktestEmulatedOrder {
            id,
            order_type: BacktestEmulatedType::TrailingStopAbs,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: Some(trail_abs),
            trigger_price: None,
            limit_price: None,
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            is_active: true,
        };

        self.emulated_orders.insert(id, order.clone());
        self.active_emulated_orders.insert(id, order);

        self.write_log(&format!(
            "发送绝对值追踪止损单: id={}, 方向={:?}, 回撤距离={}",
            id, direction, trail_abs
        ));

        id
    }

    /// Send a Market-If-Touched order
    ///
    /// Long MIT: triggers when bar.low <= trigger_price → submit market buy
    /// Short MIT: triggers when bar.high >= trigger_price → submit market sell
    pub fn send_mit(
        &mut self,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        volume: f64,
        trigger_price: f64,
    ) -> u64 {
        self.emulated_order_count += 1;
        let id = self.emulated_order_count;

        let order = BacktestEmulatedOrder {
            id,
            order_type: BacktestEmulatedType::Mit,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: None,
            trigger_price: Some(trigger_price),
            limit_price: None,
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            is_active: true,
        };

        self.emulated_orders.insert(id, order.clone());
        self.active_emulated_orders.insert(id, order);

        self.write_log(&format!(
            "发送MIT单: id={}, 方向={:?}, 触发价={}",
            id, direction, trigger_price
        ));

        id
    }

    /// Send a Limit-If-Touched order
    ///
    /// LIT: triggers like MIT, but submits a limit order at limit_price instead of market
    pub fn send_lit(
        &mut self,
        symbol: &str,
        exchange: Exchange,
        direction: Direction,
        offset: Offset,
        volume: f64,
        trigger_price: f64,
        limit_price: f64,
    ) -> u64 {
        self.emulated_order_count += 1;
        let id = self.emulated_order_count;

        let order = BacktestEmulatedOrder {
            id,
            order_type: BacktestEmulatedType::Lit,
            symbol: symbol.to_string(),
            exchange,
            direction,
            offset,
            volume,
            trail_pct: None,
            trail_abs: None,
            trigger_price: Some(trigger_price),
            limit_price: Some(limit_price),
            current_stop: None,
            highest_price: None,
            lowest_price: None,
            is_active: true,
        };

        self.emulated_orders.insert(id, order.clone());
        self.active_emulated_orders.insert(id, order);

        self.write_log(&format!(
            "发送LIT单: id={}, 方向={:?}, 触发价={}, 限价={}",
            id, direction, trigger_price, limit_price
        ));

        id
    }

    // ========================================================================
    // Bracket order methods (Bracket, OCO, OTO)
    // ========================================================================

    /// Send a bracket order: entry → take_profit + stop_loss
    ///
    /// State machine: Pending → EntryActive → SecondaryActive → Completed
    pub fn send_bracket_order(
        &mut self,
        entry_req: OrderRequest,
        tp_req: OrderRequest,
        sl_req: OrderRequest,
    ) -> u64 {
        self.bracket_group_count += 1;
        let id = self.bracket_group_count;
        let vt_symbol = format!("{}.{}", entry_req.symbol, entry_req.exchange.value());

        let entry_child = BacktestChildOrder {
            role: "entry".to_string(),
            vt_orderid: None,
            request: entry_req.clone(),
            filled_volume: 0.0,
            is_active: false,
        };

        let tp_child = BacktestChildOrder {
            role: "take_profit".to_string(),
            vt_orderid: None,
            request: tp_req,
            filled_volume: 0.0,
            is_active: false,
        };

        let sl_child = BacktestChildOrder {
            role: "stop_loss".to_string(),
            vt_orderid: None,
            request: sl_req,
            filled_volume: 0.0,
            is_active: false,
        };

        let mut children = HashMap::new();
        children.insert("entry".to_string(), entry_child);
        children.insert("take_profit".to_string(), tp_child);
        children.insert("stop_loss".to_string(), sl_child);

        let group = BacktestBracketGroup {
            id,
            bracket_type: BacktestBracketType::Bracket,
            state: BacktestBracketState::Pending,
            vt_symbol: vt_symbol.clone(),
            children,
        };

        self.bracket_groups.insert(id, group.clone());
        self.active_bracket_groups.insert(id, group);

        // Submit entry order immediately
        let entry_vt_orderid = self.send_limit_order(entry_req);
        if let Some(group) = self.bracket_groups.get_mut(&id) {
            group.state = BacktestBracketState::EntryActive;
            if let Some(entry) = group.children.get_mut("entry") {
                entry.vt_orderid = Some(entry_vt_orderid.clone());
                entry.is_active = true;
            }
        }
        if let Some(group) = self.active_bracket_groups.get_mut(&id) {
            group.state = BacktestBracketState::EntryActive;
            if let Some(entry) = group.children.get_mut("entry") {
                entry.vt_orderid = Some(entry_vt_orderid.clone());
                entry.is_active = true;
            }
        }

        self.write_log(&format!(
            "发送Bracket单: group_id={}, 入场单={}", id, entry_vt_orderid
        ));

        id
    }

    /// Send an OCO (One-Cancels-Other) order: two orders, if one fills, cancel the other
    ///
    /// State machine: Pending → SecondaryActive → Completed
    pub fn send_oco_order(
        &mut self,
        order_a_req: OrderRequest,
        order_b_req: OrderRequest,
    ) -> u64 {
        self.bracket_group_count += 1;
        let id = self.bracket_group_count;
        let vt_symbol = format!("{}.{}", order_a_req.symbol, order_a_req.exchange.value());

        let child_a = BacktestChildOrder {
            role: "order_a".to_string(),
            vt_orderid: None,
            request: order_a_req.clone(),
            filled_volume: 0.0,
            is_active: false,
        };

        let child_b = BacktestChildOrder {
            role: "order_b".to_string(),
            vt_orderid: None,
            request: order_b_req.clone(),
            filled_volume: 0.0,
            is_active: false,
        };

        let mut children = HashMap::new();
        children.insert("order_a".to_string(), child_a);
        children.insert("order_b".to_string(), child_b);

        let group = BacktestBracketGroup {
            id,
            bracket_type: BacktestBracketType::Oco,
            state: BacktestBracketState::Pending,
            vt_symbol: vt_symbol.clone(),
            children,
        };

        self.bracket_groups.insert(id, group.clone());
        self.active_bracket_groups.insert(id, group);

        // Submit both orders immediately
        let vt_a = self.send_limit_order(order_a_req);
        let vt_b = self.send_limit_order(order_b_req);

        if let Some(group) = self.bracket_groups.get_mut(&id) {
            group.state = BacktestBracketState::SecondaryActive;
            if let Some(a) = group.children.get_mut("order_a") {
                a.vt_orderid = Some(vt_a.clone());
                a.is_active = true;
            }
            if let Some(b) = group.children.get_mut("order_b") {
                b.vt_orderid = Some(vt_b.clone());
                b.is_active = true;
            }
        }
        if let Some(group) = self.active_bracket_groups.get_mut(&id) {
            group.state = BacktestBracketState::SecondaryActive;
            if let Some(a) = group.children.get_mut("order_a") {
                a.vt_orderid = Some(vt_a.clone());
                a.is_active = true;
            }
            if let Some(b) = group.children.get_mut("order_b") {
                b.vt_orderid = Some(vt_b.clone());
                b.is_active = true;
            }
        }

        self.write_log(&format!(
            "发送OCO单: group_id={}, order_a={}, order_b={}", id, vt_a, vt_b
        ));

        id
    }

    /// Send an OTO (One-Triggers-Other) order: primary fills → submit secondary
    ///
    /// State machine: Pending → SecondaryActive → Completed
    pub fn send_oto_order(
        &mut self,
        primary_req: OrderRequest,
        secondary_req: OrderRequest,
    ) -> u64 {
        self.bracket_group_count += 1;
        let id = self.bracket_group_count;
        let vt_symbol = format!("{}.{}", primary_req.symbol, primary_req.exchange.value());

        let primary_child = BacktestChildOrder {
            role: "primary".to_string(),
            vt_orderid: None,
            request: primary_req.clone(),
            filled_volume: 0.0,
            is_active: false,
        };

        let secondary_child = BacktestChildOrder {
            role: "secondary".to_string(),
            vt_orderid: None,
            request: secondary_req,
            filled_volume: 0.0,
            is_active: false,
        };

        let mut children = HashMap::new();
        children.insert("primary".to_string(), primary_child);
        children.insert("secondary".to_string(), secondary_child);

        let group = BacktestBracketGroup {
            id,
            bracket_type: BacktestBracketType::Oto,
            state: BacktestBracketState::Pending,
            vt_symbol: vt_symbol.clone(),
            children,
        };

        self.bracket_groups.insert(id, group.clone());
        self.active_bracket_groups.insert(id, group);

        // Submit primary order immediately
        let vt_primary = self.send_limit_order(primary_req);
        if let Some(group) = self.bracket_groups.get_mut(&id) {
            group.state = BacktestBracketState::EntryActive;
            if let Some(p) = group.children.get_mut("primary") {
                p.vt_orderid = Some(vt_primary.clone());
                p.is_active = true;
            }
        }
        if let Some(group) = self.active_bracket_groups.get_mut(&id) {
            group.state = BacktestBracketState::EntryActive;
            if let Some(p) = group.children.get_mut("primary") {
                p.vt_orderid = Some(vt_primary.clone());
                p.is_active = true;
            }
        }

        self.write_log(&format!(
            "发送OTO单: group_id={}, 主单={}", id, vt_primary
        ));

        id
    }

    // ========================================================================
    // Emulated order crossing (called per bar)
    // ========================================================================

    /// Cross emulated orders against current bar
    ///
    /// For each active emulated order:
    /// - TrailingStop: update trail, trigger if price crosses stop
    /// - MIT: trigger if price touches trigger level
    /// - LIT: trigger if price touches trigger level, submit limit order
    fn cross_emulated_order(&mut self, bar: &BarData) {
        let active_orders: Vec<_> = self.active_emulated_orders.iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        let mut triggered_ids = Vec::new();

        for (id, mut order) in active_orders {
            let mut should_trigger = false;
            let mut trigger_direction = order.direction;
            let mut trigger_offset = order.offset;
            let trigger_volume = order.volume;
            let mut trigger_order_type = OrderType::Market;
            let mut trigger_price = 0.0_f64;

            match order.order_type {
                BacktestEmulatedType::TrailingStopPct => {
                    let trail_pct = order.trail_pct.unwrap_or(0.0);

                    match order.direction {
                        Direction::Long => {
                            // Check if this is the first bar (highest not yet set)
                            let was_first_bar = order.highest_price.is_none();

                            // Update highest price seen
                            let prev_highest = order.highest_price.unwrap_or(0.0);
                            let new_highest = if bar.high_price > prev_highest {
                                bar.high_price
                            } else {
                                prev_highest
                            };
                            order.highest_price = Some(new_highest);

                            // Compute stop: highest * (1 - pct/100)
                            let new_stop = new_highest * (1.0 - trail_pct / 100.0);
                            // Only ratchet stop upward
                            let prev_stop = order.current_stop.unwrap_or(0.0);
                            order.current_stop = Some(if new_stop > prev_stop { new_stop } else { prev_stop });

                            // Trigger if bar.low <= current_stop (only after stop has been established)
                            if !was_first_bar && order.current_stop.unwrap_or(0.0) > 0.0 && bar.low_price <= order.current_stop.unwrap_or(0.0) {
                                should_trigger = true;
                                // Trailing stop for long triggers a sell (Short direction to close)
                                trigger_direction = Direction::Short;
                                trigger_offset = Offset::Close;
                                trigger_order_type = OrderType::Market;
                                trigger_price = order.current_stop.unwrap_or(0.0);
                            }
                        }
                        Direction::Short => {
                            // Check if this is the first bar (lowest not yet set)
                            let was_first_bar = order.lowest_price.is_none();

                            // Update lowest price seen
                            let prev_lowest = order.lowest_price.unwrap_or(f64::MAX);
                            let new_lowest = if bar.low_price < prev_lowest {
                                bar.low_price
                            } else {
                                prev_lowest
                            };
                            order.lowest_price = Some(new_lowest);

                            // Compute stop: lowest * (1 + pct/100)
                            let new_stop = new_lowest * (1.0 + trail_pct / 100.0);
                            // Only ratchet stop downward
                            let prev_stop = order.current_stop.unwrap_or(f64::MAX);
                            order.current_stop = Some(if new_stop < prev_stop { new_stop } else { prev_stop });

                            // Trigger if bar.high >= current_stop (only after stop has been established)
                            let stop = order.current_stop.unwrap_or(f64::MAX);
                            if !was_first_bar && stop < f64::MAX && bar.high_price >= stop {
                                should_trigger = true;
                                // Trailing stop for short triggers a buy (Long direction to close)
                                trigger_direction = Direction::Long;
                                trigger_offset = Offset::Close;
                                trigger_order_type = OrderType::Market;
                                trigger_price = stop;
                            }
                        }
                        _ => {}
                    }
                }
                BacktestEmulatedType::TrailingStopAbs => {
                    let trail_abs = order.trail_abs.unwrap_or(0.0);

                    match order.direction {
                        Direction::Long => {
                            // Check if this is the first bar
                            let was_first_bar = order.highest_price.is_none();

                            let prev_highest = order.highest_price.unwrap_or(0.0);
                            let new_highest = if bar.high_price > prev_highest { bar.high_price } else { prev_highest };
                            order.highest_price = Some(new_highest);

                            let new_stop = new_highest - trail_abs;
                            let prev_stop = order.current_stop.unwrap_or(0.0);
                            order.current_stop = Some(if new_stop > prev_stop { new_stop } else { prev_stop });

                            // Only trigger after stop has been established
                            if !was_first_bar && order.current_stop.unwrap_or(0.0) > 0.0 && bar.low_price <= order.current_stop.unwrap_or(0.0) {
                                should_trigger = true;
                                trigger_direction = Direction::Short;
                                trigger_offset = Offset::Close;
                                trigger_order_type = OrderType::Market;
                                trigger_price = order.current_stop.unwrap_or(0.0);
                            }
                        }
                        Direction::Short => {
                            // Check if this is the first bar
                            let was_first_bar = order.lowest_price.is_none();

                            let prev_lowest = order.lowest_price.unwrap_or(f64::MAX);
                            let new_lowest = if bar.low_price < prev_lowest { bar.low_price } else { prev_lowest };
                            order.lowest_price = Some(new_lowest);

                            let new_stop = new_lowest + trail_abs;
                            let prev_stop = order.current_stop.unwrap_or(f64::MAX);
                            order.current_stop = Some(if new_stop < prev_stop { new_stop } else { prev_stop });

                            let stop = order.current_stop.unwrap_or(f64::MAX);
                            // Only trigger after stop has been established
                            if !was_first_bar && stop < f64::MAX && bar.high_price >= stop {
                                should_trigger = true;
                                trigger_direction = Direction::Long;
                                trigger_offset = Offset::Close;
                                trigger_order_type = OrderType::Market;
                                trigger_price = stop;
                            }
                        }
                        _ => {}
                    }
                }
                BacktestEmulatedType::Mit => {
                    let trigger = order.trigger_price.unwrap_or(0.0);
                    match order.direction {
                        Direction::Long => {
                            if bar.low_price <= trigger {
                                should_trigger = true;
                                trigger_order_type = OrderType::Market;
                                trigger_price = trigger;
                            }
                        }
                        Direction::Short => {
                            if bar.high_price >= trigger {
                                should_trigger = true;
                                trigger_order_type = OrderType::Market;
                                trigger_price = trigger;
                            }
                        }
                        _ => {}
                    }
                }
                BacktestEmulatedType::Lit => {
                    let trigger = order.trigger_price.unwrap_or(0.0);
                    match order.direction {
                        Direction::Long => {
                            if bar.low_price <= trigger {
                                should_trigger = true;
                                trigger_order_type = OrderType::Limit;
                                trigger_price = order.limit_price.unwrap_or(trigger);
                            }
                        }
                        Direction::Short => {
                            if bar.high_price >= trigger {
                                should_trigger = true;
                                trigger_order_type = OrderType::Limit;
                                trigger_price = order.limit_price.unwrap_or(trigger);
                            }
                        }
                        _ => {}
                    }
                }
            }

            if should_trigger {
                order.is_active = false;

                // Update the stored order
                if let Some(stored) = self.emulated_orders.get_mut(&id) {
                    stored.is_active = false;
                    stored.current_stop = order.current_stop;
                    stored.highest_price = order.highest_price;
                    stored.lowest_price = order.lowest_price;
                }

                triggered_ids.push(id);

                // Submit the generated order
                let req = OrderRequest {
                    symbol: order.symbol.clone(),
                    exchange: order.exchange,
                    direction: trigger_direction,
                    order_type: trigger_order_type,
                    volume: trigger_volume,
                    price: trigger_price,
                    offset: trigger_offset,
                    reference: format!("EMULATED_{}", id),
                };

                self.send_limit_order(req);

                self.write_log(&format!(
                    "模拟订单触发: id={}, 类型={:?}, 方向={:?}",
                    id, order.order_type, trigger_direction
                ));
            } else {
                // Update tracking prices even if not triggered
                if let Some(stored) = self.emulated_orders.get_mut(&id) {
                    stored.current_stop = order.current_stop;
                    stored.highest_price = order.highest_price;
                    stored.lowest_price = order.lowest_price;
                }
                if let Some(stored) = self.active_emulated_orders.get_mut(&id) {
                    stored.current_stop = order.current_stop;
                    stored.highest_price = order.highest_price;
                    stored.lowest_price = order.lowest_price;
                }
            }
        }

        // Remove triggered orders from active set
        for id in triggered_ids {
            self.active_emulated_orders.remove(&id);
        }
    }

    // ========================================================================
    // Bracket order processing (called after each fill)
    // ========================================================================

    /// Process bracket order state machine after a trade fill
    ///
    /// Bracket: entry fill → submit TP+SL; TP fill → cancel SL; SL fill → cancel TP
    /// OCO: one fills → cancel the other
    /// OTO: primary fills → submit secondary; secondary fills → complete
    fn process_bracket_on_trade(&mut self, trade: &TradeData) {
        let trade_orderid = &trade.orderid;

        // Find which bracket group this trade belongs to
        let mut matching_group_id: Option<u64> = None;
        let mut matching_role: Option<String> = None;

        for (group_id, group) in &self.active_bracket_groups {
            for (role, child) in &group.children {
                if let Some(ref vt_orderid) = child.vt_orderid {
                    if vt_orderid == trade_orderid {
                        matching_group_id = Some(*group_id);
                        matching_role = Some(role.clone());
                        break;
                    }
                }
            }
            if matching_group_id.is_some() {
                break;
            }
        }

        let (group_id, role) = match (matching_group_id, matching_role) {
            (Some(g), Some(r)) => (g, r),
            _ => return, // Trade doesn't belong to any bracket group
        };

        // Update filled volume in the child order
        if let Some(group) = self.bracket_groups.get_mut(&group_id) {
            if let Some(child) = group.children.get_mut(&role) {
                child.filled_volume += trade.volume;
            }
        }
        if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
            if let Some(child) = group.children.get_mut(&role) {
                child.filled_volume += trade.volume;
            }
        }

        // Get bracket_type from the stored group (we need to read before mutating)
        let bracket_type = self.bracket_groups.get(&group_id)
            .map(|g| g.bracket_type)
            .unwrap_or(BacktestBracketType::Bracket);

        match bracket_type {
            BacktestBracketType::Bracket => {
                match role.as_str() {
                    "entry" => {
                        // Entry filled → submit TP (limit) and SL (stop)
                        let tp_req = self.bracket_groups.get(&group_id)
                            .and_then(|g| g.children.get("take_profit"))
                            .map(|c| c.request.clone());
                        let sl_req = self.bracket_groups.get(&group_id)
                            .and_then(|g| g.children.get("stop_loss"))
                            .map(|c| c.request.clone());

                        if let Some(req) = tp_req {
                            let vt_id = self.send_limit_order(req);
                            if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                                group.state = BacktestBracketState::SecondaryActive;
                                if let Some(tp) = group.children.get_mut("take_profit") {
                                    tp.vt_orderid = Some(vt_id.clone());
                                    tp.is_active = true;
                                }
                            }
                            if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                                group.state = BacktestBracketState::SecondaryActive;
                                if let Some(tp) = group.children.get_mut("take_profit") {
                                    tp.vt_orderid = Some(vt_id.clone());
                                    tp.is_active = true;
                                }
                            }
                        }

                        if let Some(req) = sl_req {
                            let vt_id = self.send_stop_order(req);
                            if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                                if let Some(sl) = group.children.get_mut("stop_loss") {
                                    sl.vt_orderid = Some(vt_id.clone());
                                    sl.is_active = true;
                                }
                            }
                            if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                                if let Some(sl) = group.children.get_mut("stop_loss") {
                                    sl.vt_orderid = Some(vt_id);
                                    sl.is_active = true;
                                }
                            }
                        }

                        // Deactivate entry child
                        if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                            if let Some(entry) = group.children.get_mut("entry") {
                                entry.is_active = false;
                            }
                        }
                        if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                            if let Some(entry) = group.children.get_mut("entry") {
                                entry.is_active = false;
                            }
                        }

                        self.write_log(&format!("Bracket入场成交: group_id={}", group_id));
                    }
                    "take_profit" => {
                        // TP filled → cancel SL → Completed
                        let sl_vt_orderid = self.bracket_groups.get(&group_id)
                            .and_then(|g| g.children.get("stop_loss"))
                            .and_then(|c| c.vt_orderid.clone());

                        if let Some(sl_id) = sl_vt_orderid {
                            self.cancel_order(&sl_id);
                        }

                        if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                            group.state = BacktestBracketState::Completed;
                            if let Some(tp) = group.children.get_mut("take_profit") {
                                tp.is_active = false;
                            }
                            if let Some(sl) = group.children.get_mut("stop_loss") {
                                sl.is_active = false;
                            }
                        }
                        if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                            group.state = BacktestBracketState::Completed;
                            if let Some(tp) = group.children.get_mut("take_profit") {
                                tp.is_active = false;
                            }
                            if let Some(sl) = group.children.get_mut("stop_loss") {
                                sl.is_active = false;
                            }
                        }

                        self.active_bracket_groups.remove(&group_id);
                        self.write_log(&format!("Bracket止盈成交: group_id={}, 已取消止损", group_id));
                    }
                    "stop_loss" => {
                        // SL filled → cancel TP → Completed
                        let tp_vt_orderid = self.bracket_groups.get(&group_id)
                            .and_then(|g| g.children.get("take_profit"))
                            .and_then(|c| c.vt_orderid.clone());

                        if let Some(tp_id) = tp_vt_orderid {
                            self.cancel_order(&tp_id);
                        }

                        if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                            group.state = BacktestBracketState::Completed;
                            if let Some(tp) = group.children.get_mut("take_profit") {
                                tp.is_active = false;
                            }
                            if let Some(sl) = group.children.get_mut("stop_loss") {
                                sl.is_active = false;
                            }
                        }
                        if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                            group.state = BacktestBracketState::Completed;
                            if let Some(tp) = group.children.get_mut("take_profit") {
                                tp.is_active = false;
                            }
                            if let Some(sl) = group.children.get_mut("stop_loss") {
                                sl.is_active = false;
                            }
                        }

                        self.active_bracket_groups.remove(&group_id);
                        self.write_log(&format!("Bracket止损成交: group_id={}, 已取消止盈", group_id));
                    }
                    _ => {}
                }
            }
            BacktestBracketType::Oco => {
                // One fills → cancel the other
                let other_role = if role == "order_a" { "order_b" } else { "order_a" };

                let other_vt_orderid = self.bracket_groups.get(&group_id)
                    .and_then(|g| g.children.get(other_role))
                    .and_then(|c| c.vt_orderid.clone());

                if let Some(other_id) = other_vt_orderid {
                    self.cancel_order(&other_id);
                }

                if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                    group.state = BacktestBracketState::Completed;
                    if let Some(child) = group.children.get_mut(&role) {
                        child.is_active = false;
                    }
                    if let Some(other) = group.children.get_mut(other_role) {
                        other.is_active = false;
                    }
                }
                if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                    group.state = BacktestBracketState::Completed;
                    if let Some(child) = group.children.get_mut(&role) {
                        child.is_active = false;
                    }
                    if let Some(other) = group.children.get_mut(other_role) {
                        other.is_active = false;
                    }
                }

                self.active_bracket_groups.remove(&group_id);
                self.write_log(&format!(
                    "OCO单成交: group_id={}, 成交方={}, 已取消{}",
                    group_id, role, other_role
                ));
            }
            BacktestBracketType::Oto => {
                match role.as_str() {
                    "primary" => {
                        // Primary filled → submit secondary
                        let sec_req = self.bracket_groups.get(&group_id)
                            .and_then(|g| g.children.get("secondary"))
                            .map(|c| c.request.clone());

                        if let Some(req) = sec_req {
                            let vt_id = self.send_limit_order(req);
                            if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                                group.state = BacktestBracketState::SecondaryActive;
                                if let Some(sec) = group.children.get_mut("secondary") {
                                    sec.vt_orderid = Some(vt_id.clone());
                                    sec.is_active = true;
                                }
                            }
                            if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                                group.state = BacktestBracketState::SecondaryActive;
                                if let Some(sec) = group.children.get_mut("secondary") {
                                    sec.vt_orderid = Some(vt_id.clone());
                                    sec.is_active = true;
                                }
                            }
                        }

                        if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                            if let Some(p) = group.children.get_mut("primary") {
                                p.is_active = false;
                            }
                        }
                        if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                            if let Some(p) = group.children.get_mut("primary") {
                                p.is_active = false;
                            }
                        }

                        self.write_log(&format!("OTO主单成交: group_id={}", group_id));
                    }
                    "secondary" => {
                        // Secondary filled → Completed
                        if let Some(group) = self.bracket_groups.get_mut(&group_id) {
                            group.state = BacktestBracketState::Completed;
                            if let Some(sec) = group.children.get_mut("secondary") {
                                sec.is_active = false;
                            }
                        }
                        if let Some(group) = self.active_bracket_groups.get_mut(&group_id) {
                            group.state = BacktestBracketState::Completed;
                            if let Some(sec) = group.children.get_mut("secondary") {
                                sec.is_active = false;
                            }
                        }

                        self.active_bracket_groups.remove(&group_id);
                        self.write_log(&format!("OTO副单成交: group_id={}", group_id));
                    }
                    _ => {}
                }
            }
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

    #[tokio::test]
    async fn test_load_data_from_binance_start_after_end() {
        let mut engine = BacktestingEngine::new();
        engine.start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        engine.end = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        let result = engine.load_data_from_binance().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("起始日期必须小于结束日期"));
    }

    // ========================================================================
    // Emulated order tests
    // ========================================================================

    #[test]
    fn test_send_trailing_stop_pct() {
        let mut engine = BacktestingEngine::new();
        let id = engine.send_trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Close, 1.0, 5.0,
        );
        assert_eq!(id, 1);
        assert_eq!(engine.emulated_order_count, 1);
        assert!(engine.emulated_orders.contains_key(&id));
        assert!(engine.active_emulated_orders.contains_key(&id));
        let order = &engine.emulated_orders[&id];
        assert_eq!(order.order_type, BacktestEmulatedType::TrailingStopPct);
        assert_eq!(order.direction, Direction::Long);
        assert!((order.trail_pct.unwrap_or(0.0) - 5.0).abs() < 1e-10);
        assert!(order.is_active);
    }

    #[test]
    fn test_trailing_stop_long_triggers() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.pricetick = 0.01;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        let id = engine.send_trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Close, 1.0, 5.0,  // 5% trailing
        );

        // Bar 1: price rises, highest=51000, stop = 51000*(1-0.05) = 48450
        let bar1 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 51000.0,
            low_price: 49900.0,
            close_price: 50900.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar1);
        // Should not trigger yet
        assert!(engine.active_emulated_orders.contains_key(&id));
        // Stop should be updated
        let order = &engine.active_emulated_orders[&id];
        let expected_stop = 51000.0 * (1.0 - 5.0 / 100.0); // 48450.0
        assert!((order.current_stop.unwrap_or(0.0) - expected_stop).abs() < 1e-6);

        // Bar 2: price drops below stop (low=48000 < 48450)
        let bar2 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 48500.0,
            high_price: 48600.0,
            low_price: 48000.0,
            close_price: 48100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar2);
        // Should be triggered and removed from active
        assert!(!engine.active_emulated_orders.contains_key(&id));
        assert!(!engine.emulated_orders[&id].is_active);
    }

    #[test]
    fn test_trailing_stop_short_triggers() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.pricetick = 0.01;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        let id = engine.send_trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Short,
            Offset::Close, 1.0, 5.0,  // 5% trailing
        );

        // Bar 1: price falls, lowest=49000, stop = 49000*(1+0.05) = 51450
        let bar1 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49000.0,
            close_price: 49100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar1);
        assert!(engine.active_emulated_orders.contains_key(&id));
        let order = &engine.active_emulated_orders[&id];
        let expected_stop = 49000.0 * (1.0 + 5.0 / 100.0); // 51450.0
        assert!((order.current_stop.unwrap_or(0.0) - expected_stop).abs() < 1e-6);

        // Bar 2: price rises above stop (high=52000 > 51450)
        let bar2 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 51300.0,
            high_price: 52000.0,
            low_price: 51200.0,
            close_price: 51900.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar2);
        assert!(!engine.active_emulated_orders.contains_key(&id));
        assert!(!engine.emulated_orders[&id].is_active);
    }

    #[test]
    fn test_mit_order_triggers() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Long MIT: triggers when bar.low <= trigger_price
        let id = engine.send_mit(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Open, 1.0, 49000.0,
        );

        // Bar where low=48900 <= 49000 → should trigger
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 49500.0,
            high_price: 49600.0,
            low_price: 48900.0,
            close_price: 49100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar);

        // Should be triggered
        assert!(!engine.active_emulated_orders.contains_key(&id));
        assert!(!engine.emulated_orders[&id].is_active);
        // A limit order should have been submitted (via send_limit_order)
        assert_eq!(engine.limit_order_count, 1);
    }

    #[test]
    fn test_mit_order_no_trigger() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;

        let id = engine.send_mit(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Open, 1.0, 48000.0,
        );

        // Bar where low=49000 > 48000 → should NOT trigger
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 49500.0,
            high_price: 49600.0,
            low_price: 49000.0,
            close_price: 49100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar);
        assert!(engine.active_emulated_orders.contains_key(&id));
    }

    #[test]
    fn test_lit_order_triggers() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Long LIT: triggers when bar.low <= trigger_price, submits limit at limit_price
        let id = engine.send_lit(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Open, 1.0, 49000.0, 48900.0,
        );

        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 49500.0,
            high_price: 49600.0,
            low_price: 48900.0,
            close_price: 49100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar);

        assert!(!engine.active_emulated_orders.contains_key(&id));
        assert!(!engine.emulated_orders[&id].is_active);
        // A limit order should have been submitted with limit price
        assert_eq!(engine.limit_order_count, 1);
        // The submitted order should have the limit_price (48900)
        let submitted = engine.active_limit_orders.values().next().unwrap();
        assert!((submitted.price - 48900.0).abs() < 1e-10);
    }

    #[test]
    fn test_trailing_stop_abs_triggers() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.pricetick = 0.01;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));

        // Long trailing stop with absolute distance of 1000
        let id = engine.send_trailing_stop_abs(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Close, 1.0, 1000.0,
        );

        // Bar 1: highest=51000, stop = 51000-1000 = 50000
        let bar1 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 51000.0,
            low_price: 49900.0,
            close_price: 50900.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar1);
        assert!(engine.active_emulated_orders.contains_key(&id));

        // Bar 2: low=49900 < 50000 → triggers
        let bar2 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 50100.0,
            high_price: 50200.0,
            low_price: 49900.0,
            close_price: 49950.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_emulated_order(&bar2);
        assert!(!engine.active_emulated_orders.contains_key(&id));
    }

    // ========================================================================
    // Bracket order tests
    // ========================================================================

    #[test]
    fn test_send_bracket_order() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;

        let entry_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let tp_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Close,
            reference: String::new(),
        };
        let sl_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Stop,
            volume: 1.0,
            price: 49000.0,
            offset: Offset::Close,
            reference: String::new(),
        };

        let group_id = engine.send_bracket_order(entry_req, tp_req, sl_req);
        assert_eq!(group_id, 1);
        assert_eq!(engine.bracket_group_count, 1);
        assert!(engine.bracket_groups.contains_key(&group_id));
        assert!(engine.active_bracket_groups.contains_key(&group_id));

        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.bracket_type, BacktestBracketType::Bracket);
        assert_eq!(group.state, BacktestBracketState::EntryActive);
        assert!(group.children.contains_key("entry"));
        assert!(group.children.contains_key("take_profit"));
        assert!(group.children.contains_key("stop_loss"));
        // Entry should be active
        assert!(group.children["entry"].is_active);
        // TP/SL should not be active yet
        assert!(!group.children["take_profit"].is_active);
        assert!(!group.children["stop_loss"].is_active);
        // An entry limit order should have been submitted
        assert_eq!(engine.limit_order_count, 1);
    }

    #[test]
    fn test_bracket_entry_fills_activates_tp_sl() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.1)));
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );

        let entry_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let tp_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Close,
            reference: String::new(),
        };
        let sl_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Stop,
            volume: 1.0,
            price: 49000.0,
            offset: Offset::Close,
            reference: String::new(),
        };

        let group_id = engine.send_bracket_order(entry_req, tp_req, sl_req);

        // Get entry order id
        let entry_vt_orderid = engine.bracket_groups[&group_id].children["entry"].vt_orderid.clone().unwrap_or_default();

        // Bar that fills the entry order
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

        // After entry fills: state should be SecondaryActive
        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.state, BacktestBracketState::SecondaryActive);
        // TP should now be active (limit order submitted)
        assert!(group.children["take_profit"].is_active);
        assert!(group.children["take_profit"].vt_orderid.is_some());
        // SL should now be active (stop order submitted)
        assert!(group.children["stop_loss"].is_active);
        assert!(group.children["stop_loss"].vt_orderid.is_some());
        // Entry should be inactive
        assert!(!group.children["entry"].is_active);
    }

    #[test]
    fn test_bracket_tp_fills_cancels_sl() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.rate = 0.0;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.0)));
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );

        let entry_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let tp_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Close,
            reference: String::new(),
        };
        let sl_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Stop,
            volume: 1.0,
            price: 49000.0,
            offset: Offset::Close,
            reference: String::new(),
        };

        let group_id = engine.send_bracket_order(entry_req, tp_req, sl_req);

        // Fill entry order
        let bar1 = BarData {
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
        engine.cross_limit_order(&bar1);

        // Get TP and SL order ids
        let tp_vt_orderid = engine.bracket_groups[&group_id].children["take_profit"].vt_orderid.clone().unwrap_or_default();
        let sl_vt_orderid = engine.bracket_groups[&group_id].children["stop_loss"].vt_orderid.clone().unwrap_or_default();

        // Fill TP order (sell limit at 51000, bar high >= 51000)
        let bar2 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 51000.0,
            high_price: 51200.0,
            low_price: 50900.0,
            close_price: 51100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_limit_order(&bar2);

        // After TP fills: bracket should be Completed
        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.state, BacktestBracketState::Completed);
        // SL should be cancelled (removed from active)
        assert!(!engine.active_stop_orders.contains_key(&sl_vt_orderid));
        // TP child should be inactive
        assert!(!group.children["take_profit"].is_active);
        assert!(!group.children["stop_loss"].is_active);
    }

    #[test]
    fn test_send_oco_order() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;

        let order_a_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let order_b_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 49000.0,
            offset: Offset::Open,
            reference: String::new(),
        };

        let group_id = engine.send_oco_order(order_a_req, order_b_req);
        assert_eq!(group_id, 1);
        assert_eq!(engine.bracket_group_count, 1);

        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.bracket_type, BacktestBracketType::Oco);
        assert_eq!(group.state, BacktestBracketState::SecondaryActive);
        // Both orders should be active
        assert!(group.children["order_a"].is_active);
        assert!(group.children["order_b"].is_active);
        // Both limit orders should have been submitted
        assert_eq!(engine.limit_order_count, 2);
    }

    #[test]
    fn test_oco_one_fills_cancels_other() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.rate = 0.0;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.0)));
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );

        let order_a_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let order_b_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 49000.0,
            offset: Offset::Open,
            reference: String::new(),
        };

        let group_id = engine.send_oco_order(order_a_req, order_b_req);

        let order_b_vt = engine.bracket_groups[&group_id].children["order_b"].vt_orderid.clone().unwrap_or_default();

        // Fill order_a (Short limit at 51000 → bar high >= 51000)
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now(),
            interval: Some(Interval::Minute),
            open_price: 51000.0,
            high_price: 51200.0,
            low_price: 50900.0,
            close_price: 51100.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        engine.cross_limit_order(&bar);

        // After order_a fills: bracket should be Completed
        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.state, BacktestBracketState::Completed);
        // order_b should be cancelled
        assert!(!engine.active_limit_orders.contains_key(&order_b_vt));
        assert!(!group.children["order_a"].is_active);
        assert!(!group.children["order_b"].is_active);
    }

    #[test]
    fn test_send_oto_order() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;

        let primary_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let secondary_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Close,
            reference: String::new(),
        };

        let group_id = engine.send_oto_order(primary_req, secondary_req);
        assert_eq!(group_id, 1);

        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.bracket_type, BacktestBracketType::Oto);
        assert_eq!(group.state, BacktestBracketState::EntryActive);
        // Primary should be active, secondary should not
        assert!(group.children["primary"].is_active);
        assert!(!group.children["secondary"].is_active);
        // Only one limit order should be submitted (primary)
        assert_eq!(engine.limit_order_count, 1);
    }

    #[test]
    fn test_oto_primary_fills_submits_secondary() {
        let mut engine = BacktestingEngine::new();
        engine.vt_symbol = "BTCUSDT.BINANCE".to_string();
        engine.symbol = "BTCUSDT".to_string();
        engine.exchange = Exchange::Binance;
        engine.rate = 0.0;
        engine.set_fill_model(Box::new(BestPriceFillModel::new(0.0)));
        engine.position = crate::backtesting::position::Position::new(
            crate::backtesting::position::Position::generate_position_id("BTCUSDT", Exchange::Binance, 0),
            "BTCUSDT".to_string(),
            Exchange::Binance,
        );

        let primary_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 50000.0,
            offset: Offset::Open,
            reference: String::new(),
        };
        let secondary_req = OrderRequest {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume: 1.0,
            price: 51000.0,
            offset: Offset::Close,
            reference: String::new(),
        };

        let group_id = engine.send_oto_order(primary_req, secondary_req);

        // Fill primary order
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

        // After primary fills: secondary should be submitted
        let group = &engine.bracket_groups[&group_id];
        assert_eq!(group.state, BacktestBracketState::SecondaryActive);
        assert!(!group.children["primary"].is_active);
        assert!(group.children["secondary"].is_active);
        assert!(group.children["secondary"].vt_orderid.is_some());
    }

    #[test]
    fn test_clear_data_resets_emulated_and_bracket() {
        let mut engine = BacktestingEngine::new();
        engine.send_trailing_stop_pct(
            "BTCUSDT", Exchange::Binance, Direction::Long,
            Offset::Close, 1.0, 5.0,
        );
        engine.send_bracket_order(
            OrderRequest::new("BTCUSDT".to_string(), Exchange::Binance, Direction::Long, OrderType::Limit, 1.0),
            OrderRequest::new("BTCUSDT".to_string(), Exchange::Binance, Direction::Short, OrderType::Limit, 1.0),
            OrderRequest::new("BTCUSDT".to_string(), Exchange::Binance, Direction::Short, OrderType::Stop, 1.0),
        );

        assert_eq!(engine.emulated_order_count, 1);
        assert_eq!(engine.bracket_group_count, 1);

        engine.clear_data();

        assert_eq!(engine.emulated_order_count, 0);
        assert!(engine.emulated_orders.is_empty());
        assert!(engine.active_emulated_orders.is_empty());
        assert_eq!(engine.bracket_group_count, 0);
        assert!(engine.bracket_groups.is_empty());
        assert!(engine.active_bracket_groups.is_empty());
    }
}
