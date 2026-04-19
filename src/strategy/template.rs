//! Strategy Template
//!
//! Abstract base template for implementing trading strategies

use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::base::{StrategySetting, StrategyState, StrategyType, StopOrderRequest, CancelRequestType};
#[cfg(feature = "gui")]
use crate::chart::Indicator;
use crate::trader::{
    BarData, Direction, Exchange, Interval, Offset, OrderData, OrderRequest, OrderType, TickData, TradeData,
    DepthData,
};
use crate::trader::database::BaseDatabase;

#[cfg(feature = "gui")]
type IndicatorMap = Arc<Mutex<HashMap<String, Vec<Box<dyn Indicator>>>>>;

/// Strategy context providing market data and trading interface
pub struct StrategyContext {
    pub tick_cache: Arc<Mutex<HashMap<String, TickData>>>,
    pub bar_cache: Arc<Mutex<HashMap<String, BarData>>>,
    pub historical_bars: Arc<Mutex<HashMap<String, Vec<BarData>>>>,
    /// Optional database for loading historical data
    database: Option<Arc<dyn BaseDatabase>>,
    #[cfg(feature = "gui")]
    indicators: IndicatorMap,
}

impl StrategyContext {
    pub fn new() -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
            database: None,
            #[cfg(feature = "gui")]
            indicators: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a StrategyContext with a database backend
    pub fn with_database(database: Arc<dyn BaseDatabase>) -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
            database: Some(database),
            #[cfg(feature = "gui")]
            indicators: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Set the database backend
    pub fn set_database(&mut self, database: Arc<dyn BaseDatabase>) {
        self.database = Some(database);
    }

    /// Get latest tick for symbol
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        self.tick_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(vt_symbol)
            .cloned()
    }

    /// Get latest bar for symbol
    pub fn get_bar(&self, vt_symbol: &str) -> Option<BarData> {
        self.bar_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(vt_symbol)
            .cloned()
    }

    /// Get historical bars for symbol
    pub fn get_bars(&self, vt_symbol: &str, count: usize) -> Vec<BarData> {
        if let Some(bars) = self
            .historical_bars
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(vt_symbol)
        {
            let start = bars.len().saturating_sub(count);
            bars[start..].to_vec()
        } else {
            Vec::new()
        }
    }

    /// Load historical bars from database (synchronous wrapper for async operation)
    /// Returns bars for the specified symbol, exchange, and interval over the given number of days
    pub fn load_bar(
        &self,
        vt_symbol: &str,
        exchange: Exchange,
        interval: Interval,
        days: i64,
    ) -> Option<Vec<BarData>> {
        // Try to get bars from cache first
        let cached = self.get_bars(vt_symbol, days as usize * 1440); // rough estimate
        if !cached.is_empty() {
            return Some(cached);
        }

        // If no database, return None
        let db = self.database.as_ref()?;
        
        // Calculate time range
        let end = Utc::now();
        let start = end - chrono::Duration::days(days);
        let symbol = vt_symbol.split('.').next().unwrap_or(vt_symbol).to_string();

        // Use tokio runtime to call async database method
        // This is a blocking call, but strategies typically call this during on_init
        let db_clone = Arc::clone(db);
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                db_clone.load_bar_data(&symbol, exchange, interval, start, end).await
            })
        });

        result.ok()
    }

    /// Update tick data
    pub fn update_tick(&self, tick: TickData) {
        self.tick_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tick.vt_symbol(), tick);
    }

    /// Update bar data
    pub fn update_bar(&self, bar: BarData) {
        let vt_symbol = bar.vt_symbol();

        // Update cache
        self.bar_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(vt_symbol.clone(), bar.clone());

        // Update historical bars
        let mut historical = self
            .historical_bars
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let bars = historical.entry(vt_symbol).or_default();
        bars.push(bar);
        bars.truncate(10000);
    }

    #[cfg(feature = "gui")]
    pub fn register_indicator(
        &self,
        vt_symbol: &str,
        indicator: Box<dyn Indicator>,
    ) -> IndicatorRef {
        let mut indicators = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        let list = indicators.entry(vt_symbol.to_string()).or_default();
        let index = list.len();
        list.push(indicator);
        IndicatorRef {
            key: vt_symbol.to_string(),
            index,
            indicators: Arc::clone(&self.indicators),
        }
    }

    #[cfg(feature = "gui")]
    pub fn get_indicator_refs(&self, vt_symbol: &str) -> Vec<IndicatorRef> {
        let indicators = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        match indicators.get(vt_symbol) {
            Some(list) => (0..list.len())
                .map(|index| IndicatorRef {
                    key: vt_symbol.to_string(),
                    index,
                    indicators: Arc::clone(&self.indicators),
                })
                .collect(),
            None => Vec::new(),
        }
    }

    #[cfg(feature = "gui")]
    pub fn update_indicators(&self, vt_symbol: &str, bar: &BarData) {
        let mut indicators = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(indicator_list) = indicators.get_mut(vt_symbol) {
            for indicator in indicator_list.iter_mut() {
                indicator.update(bar);
            }
        }
    }
}

impl Default for StrategyContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "gui")]
pub struct IndicatorRef {
    key: String,
    index: usize,
    indicators: IndicatorMap,
}

#[cfg(feature = "gui")]
impl IndicatorRef {
    pub fn is_ready(&self) -> bool {
        let map = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .map(|i| i.is_ready())
            .unwrap_or(false)
    }

    pub fn current_value(&self) -> Option<f64> {
        let map = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .and_then(|i| i.current_value())
    }

    pub fn name(&self) -> Option<String> {
        let map = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .map(|i| i.name().to_string())
    }
}

/// Strategy template trait
///
/// All strategies must implement this trait to work with the engine
pub trait StrategyTemplate: Send + Sync {
    /// Get strategy name
    fn strategy_name(&self) -> &str;

    /// Get subscribed symbols
    fn vt_symbols(&self) -> &[String];

    /// Get strategy type
    fn strategy_type(&self) -> StrategyType;

    /// Get current state
    fn state(&self) -> StrategyState;

    /// Get strategy parameters
    fn parameters(&self) -> HashMap<String, String>;

    /// Get strategy variables
    fn variables(&self) -> HashMap<String, String>;

    /// Initialize strategy
    fn on_init(&mut self, context: &StrategyContext);

    /// Start strategy
    fn on_start(&mut self);

    /// Stop strategy
    fn on_stop(&mut self);

    /// Tick data callback
    fn on_tick(&mut self, tick: &TickData, context: &StrategyContext);

    /// Bar data callback
    fn on_bar(&mut self, bar: &BarData, context: &StrategyContext);

    /// Depth/Order book data callback
    fn on_depth(&mut self, depth: &DepthData, context: &StrategyContext) {
        // Default implementation: no-op
        let _ = (depth, context);
    }

    /// Multiple bars callback (for strategies trading multiple symbols)
    fn on_bars(&mut self, bars: &HashMap<String, BarData>, context: &StrategyContext) {
        // Default implementation: call on_bar for each bar
        for bar in bars.values() {
            self.on_bar(bar, context);
        }
    }

    /// Order callback
    fn on_order(&mut self, order: &OrderData);

    /// Trade callback
    fn on_trade(&mut self, trade: &TradeData);

    /// Stop order callback
    fn on_stop_order(&mut self, stop_orderid: &str);

    /// Drain pending orders placed during on_bar/on_tick callback
    /// This is called by BacktestingEngine after each callback to collect orders
    /// that were placed by the strategy (e.g., via Python's buy/sell methods)
    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        Vec::new() // Default: no pending orders
    }

    /// Drain pending stop orders placed during on_bar/on_tick callback
    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        Vec::new() // Default: no pending stop orders
    }

    /// Drain pending cancellations placed during on_bar/on_tick callback
    fn drain_pending_cancellations(&mut self) -> Vec<CancelRequestType> {
        Vec::new() // Default: no pending cancellations
    }

    /// Update position
    fn update_position(&mut self, vt_symbol: &str, position: f64);

    /// Get current position
    fn get_position(&self, vt_symbol: &str) -> f64;

    /// Get target position (for target position strategies)
    fn get_target(&self, _vt_symbol: &str) -> Option<f64> {
        None // Default implementation
    }

    /// Set target position
    fn set_target(&mut self, _vt_symbol: &str, _target: f64) {
        // Default implementation (do nothing)
    }

    /// Called when a registered indicator updates (optional override)
    fn on_indicator(&mut self, _indicator_name: &str, _value: f64) {}

    #[cfg(feature = "gui")]
    fn register_indicator_for_bars(
        &self,
        context: &StrategyContext,
        vt_symbol: &str,
        indicator: Box<dyn Indicator>,
    ) -> IndicatorRef {
        context.register_indicator(vt_symbol, indicator)
    }
}

/// Base strategy implementation with common functionality
pub struct BaseStrategy {
    pub strategy_name: String,
    pub vt_symbols: Vec<String>,
    pub strategy_type: StrategyType,
    pub state: StrategyState,

    // Position tracking
    pub positions: Arc<Mutex<HashMap<String, f64>>>,

    // Target position (for grid/DMA strategies)
    pub targets: Arc<Mutex<HashMap<String, f64>>>,

    // Active order tracking
    pub active_orderids: Arc<Mutex<Vec<String>>>,

    // Active stop order tracking
    pub active_stop_orderids: Arc<Mutex<Vec<String>>>,

    // Pending orders queue (for order routing)
    pub pending_orders: Arc<Mutex<Vec<OrderRequest>>>,

    // Pending stop orders queue (for stop order routing)
    pub pending_stop_orders: Arc<Mutex<Vec<StopOrderRequest>>>,

    // Pending cancellations queue (for cancel routing)
    pub pending_cancellations: Arc<Mutex<Vec<CancelRequestType>>>,

    // Trading parameters
    pub parameters: HashMap<String, String>,
    pub variables: HashMap<String, String>,
}

impl BaseStrategy {
    pub fn new(
        strategy_name: String,
        vt_symbols: Vec<String>,
        strategy_type: StrategyType,
        setting: StrategySetting,
    ) -> Self {
        let parameters = setting
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();

        Self {
            strategy_name,
            vt_symbols,
            strategy_type,
            state: StrategyState::NotInited,
            positions: Arc::new(Mutex::new(HashMap::new())),
            targets: Arc::new(Mutex::new(HashMap::new())),
            active_orderids: Arc::new(Mutex::new(Vec::new())),
            active_stop_orderids: Arc::new(Mutex::new(Vec::new())),
            pending_orders: Arc::new(Mutex::new(Vec::new())),
            pending_stop_orders: Arc::new(Mutex::new(Vec::new())),
            pending_cancellations: Arc::new(Mutex::new(Vec::new())),
            parameters,
            variables: HashMap::new(),
        }
    }

    /// Buy order (open long for futures, buy for spot)
    pub fn buy(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        let req = self.create_order_request(vt_symbol, Direction::Long, price, volume, lock, Offset::Open);
        let vt_orderid = format!("BUY_{}_{}", vt_symbol, Utc::now().timestamp_millis());
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);
        vt_orderid
    }

    /// Sell order (close long for futures, sell for spot)
    pub fn sell(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        let offset = if self.strategy_type == StrategyType::Spot || lock {
            Offset::None
        } else {
            Offset::Close
        };
        let req = self.create_order_request(vt_symbol, Direction::Short, price, volume, lock, offset);
        let vt_orderid = format!("SELL_{}_{}", vt_symbol, Utc::now().timestamp_millis());
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);
        vt_orderid
    }

    /// Short order (open short for futures, not supported for spot)
    pub fn short(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        if self.strategy_type == StrategyType::Spot {
            tracing::warn!("Short not supported for spot trading");
            return String::new();
        }
        let req = self.create_order_request(vt_symbol, Direction::Short, price, volume, lock, Offset::Open);
        let vt_orderid = format!("SHORT_{}_{}", vt_symbol, Utc::now().timestamp_millis());
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);
        vt_orderid
    }

    /// Cover order (close short for futures, not supported for spot)
    pub fn cover(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        if self.strategy_type == StrategyType::Spot {
            tracing::warn!("Cover not supported for spot trading");
            return String::new();
        }
        let offset = if lock { Offset::CloseYesterday } else { Offset::Close };
        let req = self.create_order_request(vt_symbol, Direction::Long, price, volume, lock, offset);
        let vt_orderid = format!("COVER_{}_{}", vt_symbol, Utc::now().timestamp_millis());
        self.pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);
        vt_orderid
    }

    /// Create an OrderRequest from the given parameters
    fn create_order_request(
        &self,
        vt_symbol: &str,
        direction: Direction,
        price: f64,
        volume: f64,
        _lock: bool,
        offset: Offset,
    ) -> OrderRequest {
        let (symbol, exchange) = crate::trader::utility::extract_vt_symbol(vt_symbol)
            .unwrap_or((vt_symbol.to_string(), crate::trader::constant::Exchange::Local));
        OrderRequest {
            symbol,
            exchange,
            direction,
            order_type: crate::trader::constant::OrderType::Limit,
            volume,
            price,
            offset,
            reference: self.strategy_name.clone(),
            post_only: false,
            reduce_only: false,
        }
    }

    /// Cancel order
    pub fn cancel_order(&self, vt_orderid: &str) {
        tracing::info!("请求取消委托: {}", vt_orderid);
        // Remove from active orderids
        let mut orderids = self
            .active_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        orderids.retain(|id| id != vt_orderid);
        // Queue cancellation request for engine processing
        self.pending_cancellations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CancelRequestType::Order(vt_orderid.to_string()));
    }

    /// Cancel all orders
    pub fn cancel_all(&self) {
        let orderids = self
            .active_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for orderid in orderids.iter() {
            self.cancel_order(orderid);
        }
    }

    /// Drain pending orders (called by engine after on_bar/on_tick callback)
    pub fn drain_pending_orders(&self) -> Vec<OrderRequest> {
        let mut orders = self
            .pending_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *orders)
    }

    /// Drain pending stop orders (called by engine after on_bar/on_tick callback)
    pub fn drain_pending_stop_orders(&self) -> Vec<StopOrderRequest> {
        let mut orders = self
            .pending_stop_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *orders)
    }

    /// Drain pending cancellations (called by engine after on_bar/on_tick callback)
    pub fn drain_pending_cancellations(&self) -> Vec<CancelRequestType> {
        let mut cancellations = self
            .pending_cancellations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *cancellations)
    }

    /// Load historical bar data (placeholder — use StrategyContext.load_bar instead)
    pub fn load_bar(&self, _vt_symbol: &str, _days: i64, _interval: Interval) -> Vec<BarData> {
        // This cannot access the database directly. Use context.load_bar() in on_init instead.
        Vec::new()
    }

    /// Send stop order
    ///
    /// Creates a stop order request and queues it for engine processing.
    /// The engine will register the stop order and monitor for trigger conditions.
    /// Returns a generated stop order ID.
    pub fn send_stop_order(
        &self,
        vt_symbol: &str,
        price: f64,
        volume: f64,
        direction: Direction,
        offset: Option<Offset>,
    ) -> String {
        let stop_orderid = format!("STOP_{}_{}", vt_symbol, Utc::now().timestamp_millis());

        let req = StopOrderRequest::new(
            vt_symbol.to_string(),
            direction,
            offset,
            price,
            volume,
            OrderType::Stop,
            false,
        );

        // Track the stop order ID locally
        self.active_stop_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(stop_orderid.clone());

        // Queue stop order request for engine processing
        self.pending_stop_orders
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(req);

        tracing::info!("策略{}发送止损单: {} 价格={} 方向={:?}",
            self.strategy_name, stop_orderid, price, direction);

        stop_orderid
    }

    /// Cancel stop order
    ///
    /// Queues a stop order cancellation request for engine processing.
    pub fn cancel_stop_order(&self, stop_orderid: &str) {
        tracing::info!("策略{}请求取消止损单: {}", self.strategy_name, stop_orderid);

        // Remove from active stop orderids
        let mut orderids = self
            .active_stop_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        orderids.retain(|id| id != stop_orderid);

        // Queue cancellation request for engine processing
        self.pending_cancellations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(CancelRequestType::StopOrder(stop_orderid.to_string()));
    }

    /// Write log
    pub fn write_log(&self, msg: &str) {
        tracing::info!("[{}] {}", self.strategy_name, msg);
    }

    /// Get engine type
    pub fn get_engine_type(&self) -> &str {
        "LIVE" // or "BACKTESTING"
    }

    /// Synchronize position data from trading
    pub fn sync_position(&mut self, vt_symbol: &str, position: f64) {
        self.positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(vt_symbol.to_string(), position);
    }
}

/// Target position template for DMA/Grid strategies
pub trait TargetPosTemplate: StrategyTemplate {
    /// Calculate target positions
    fn calculate_target(&mut self, context: &StrategyContext);

    /// Rebalance positions to match targets
    fn rebalance_portfolio(&mut self);

    /// Get minimum order volume
    fn get_min_volume(&self, _vt_symbol: &str) -> f64 {
        0.001 // Default minimum
    }
}
