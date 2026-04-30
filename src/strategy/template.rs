//! Strategy Template
//!
//! Abstract base template for implementing trading strategies

use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::base::{StrategySetting, StrategyState, StrategyType, StopOrderRequest, CancelRequestType, ExecutionType, TradingMode};
use crate::trader::{
    BarData, Direction, Exchange, Interval, Offset, OrderData, OrderRequest, OrderType, TickData, TradeData,
    DepthData,
};
use crate::trader::database::BaseDatabase;
use crate::trader::utility::ArrayManager;

/// Trailing stop configuration per symbol
#[derive(Debug, Clone)]
pub struct TrailingStopConfig {
    /// Price at which trailing stop activates
    pub activation_price: f64,
    /// Distance from current price for the trailing stop
    pub trailing_distance: f64,
    /// Whether trailing is based on percentage (true) or absolute price (false)
    pub is_percentage: bool,
    /// Current stop price (moves with price)
    pub current_stop_price: f64,
    /// Direction: Long or Short
    pub direction: Direction,
}

/// Daily risk statistics (reset at midnight)
#[derive(Debug, Clone, Default)]
pub struct DailyRiskStats {
    /// Number of trades today
    pub trade_count: usize,
    /// Realized PnL today
    pub daily_pnl: f64,
    /// Date of last reset (YYYY-MM-DD format string)
    pub last_reset_date: String,
}

/// Minimal indicator trait for strategy event dispatch.
///
/// Unlike the chart `Indicator` trait (which depends on egui), this trait
/// provides only the interface needed for the `on_indicator()` callback path:
/// updating with bar data and reading the current value.
pub trait StrategyIndicator: Send + Sync {
    /// Get indicator name
    fn name(&self) -> &str;

    /// Update indicator with a single bar.
    /// Returns `true` if the indicator produced a new value (i.e. is ready).
    fn update(&mut self, bar: &BarData) -> bool;

    /// Get the current (latest) value, if the indicator is ready
    fn current_value(&self) -> Option<f64>;
}

/// Adapter: a `Box<dyn Indicator>` (chart indicator) also implements `StrategyIndicator`.
#[cfg(feature = "gui")]
impl StrategyIndicator for Box<dyn crate::chart::Indicator> {
    fn name(&self) -> &str {
        let indicator: &dyn crate::chart::Indicator = self.as_ref();
        indicator.name()
    }

    fn update(&mut self, bar: &BarData) -> bool {
        let indicator: &mut dyn crate::chart::Indicator = self.as_mut();
        indicator.update(bar)
    }

    fn current_value(&self) -> Option<f64> {
        let indicator: &dyn crate::chart::Indicator = self.as_ref();
        indicator.current_value()
    }
}

type IndicatorMap = Arc<Mutex<HashMap<String, Vec<Box<dyn StrategyIndicator>>>>>;

/// Strategy context providing market data and trading interface
pub struct StrategyContext {
    pub tick_cache: Arc<Mutex<HashMap<String, TickData>>>,
    pub bar_cache: Arc<Mutex<HashMap<String, BarData>>>,
    pub historical_bars: Arc<Mutex<HashMap<String, Vec<BarData>>>>,
    /// Optional database for loading historical data
    database: Option<Arc<dyn BaseDatabase>>,
    indicators: IndicatorMap,
    /// Current trading mode — affects how load_bar() retrieves data
    pub trading_mode: TradingMode,
}

impl StrategyContext {
    pub fn new() -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
            database: None,
            indicators: Arc::new(Mutex::new(HashMap::new())),
            trading_mode: TradingMode::Live,
        }
    }

    /// Create a `StrategyContext` with a database backend
    pub fn with_database(database: Arc<dyn BaseDatabase>) -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
            database: Some(database),
            indicators: Arc::new(Mutex::new(HashMap::new())),
            trading_mode: TradingMode::Live,
        }
    }

    /// Set the trading mode (builder-style)
    pub fn with_trading_mode(mut self, mode: TradingMode) -> Self {
        self.trading_mode = mode;
        self
    }

    /// Set the database backend
    pub fn set_database(&mut self, database: Arc<dyn BaseDatabase>) {
        self.database = Some(database);
    }

    /// Get latest tick for symbol
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        self.tick_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .cloned()
    }

    /// Get latest bar for symbol
    pub fn get_bar(&self, vt_symbol: &str) -> Option<BarData> {
        self.bar_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .cloned()
    }

    /// Get historical bars for symbol
    pub fn get_bars(&self, vt_symbol: &str, count: usize) -> Vec<BarData> {
        if let Some(bars) = self
            .historical_bars
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
    ///
    /// In Backtest mode, only returns cached data (the backtesting engine provides data directly).
    /// In Live/Paper mode, falls back to database or datafeed if cache is empty.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // value fits in target type; value is non-negative
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

        // In Backtest mode, return None — the backtesting engine provides data directly
        if self.trading_mode == TradingMode::Backtest {
            return None;
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(tick.vt_symbol(), tick);
    }

    /// Update bar data
    pub fn update_bar(&self, bar: BarData) {
        let vt_symbol = bar.vt_symbol();

        // Update cache
        self.bar_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(vt_symbol.clone(), bar.clone());

        // Update historical bars
        let mut historical = self
            .historical_bars
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let bars = historical.entry(vt_symbol).or_default();
        bars.push(bar);
        bars.truncate(10000);
    }

    pub fn register_indicator(
        &self,
        vt_symbol: &str,
        indicator: Box<dyn StrategyIndicator>,
    ) -> IndicatorRef {
        let mut indicators = self.indicators.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let list = indicators.entry(vt_symbol.to_string()).or_default();
        let index = list.len();
        list.push(indicator);
        IndicatorRef {
            key: vt_symbol.to_string(),
            index,
            indicators: Arc::clone(&self.indicators),
        }
    }

    pub fn get_indicator_refs(&self, vt_symbol: &str) -> Vec<IndicatorRef> {
        let indicators = self.indicators.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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

    /// Update indicators for the given symbol with bar data.
    ///
    /// Returns a list of `(name, value)` pairs for indicators that produced
    /// a new value (i.e. `update()` returned `true`). This enables the
    /// engine to dispatch `on_indicator()` callbacks to strategies.
    pub fn update_indicators(&self, vt_symbol: &str, bar: &BarData) -> Vec<(String, f64)> {
        let mut indicators = self.indicators.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut updated = Vec::new();
        if let Some(indicator_list) = indicators.get_mut(vt_symbol) {
            for indicator in indicator_list.iter_mut() {
                let was_ready = indicator.update(bar);
                if was_ready {
                    if let Some(value) = indicator.current_value() {
                        updated.push((indicator.name().to_string(), value));
                    }
                }
            }
        }
        updated
    }
}

impl Default for StrategyContext {
    fn default() -> Self {
        Self::new()
    }
}

pub struct IndicatorRef {
    key: String,
    index: usize,
    indicators: IndicatorMap,
}

impl IndicatorRef {
    pub fn is_ready(&self) -> bool {
        let map = self.indicators.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .map(|i| i.current_value().is_some())
            .unwrap_or(false)
    }

    pub fn current_value(&self) -> Option<f64> {
        let map = self.indicators.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .and_then(|i| i.current_value())
    }

    pub fn name(&self) -> Option<String> {
        let map = self.indicators.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
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

    /// Reset strategy state (optional override).
    ///
    /// Called when a strategy is reset via `StrategyEngine::reset_strategy()`.
    /// Use this to clear internal variables, counters, positions, and any
    /// other state that should be re-initialized before the strategy starts again.
    /// After `on_reset()`, the strategy state transitions to `Inited`.
    fn on_reset(&mut self) {
        // Default implementation: no-op
        // Strategies that need custom reset behavior should override this
    }

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

    /// Drain pending orders placed during `on_bar`/`on_tick` callback
    /// This is called by `BacktestingEngine` after each callback to collect orders
    /// that were placed by the strategy (e.g., via Python's buy/sell methods)
    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        Vec::new() // Default: no pending orders
    }

    /// Drain pending stop orders placed during `on_bar`/`on_tick` callback
    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        Vec::new() // Default: no pending stop orders
    }

    /// Drain pending cancellations placed during `on_bar`/`on_tick` callback
    fn drain_pending_cancellations(&mut self) -> Vec<CancelRequestType> {
        Vec::new() // Default: no pending cancellations
    }

    /// Drain pending indicator registrations from the strategy
    #[cfg(feature = "python")]
    fn drain_pending_indicator_registrations(&mut self) -> Vec<crate::python::PendingIndicatorRegistration> {
        Vec::new() // Default: no indicator registrations
    }

    /// Drain pending indicator values from the strategy
    #[cfg(feature = "python")]
    fn drain_pending_indicator_values(&mut self) -> Vec<crate::python::PendingIndicatorValue> {
        Vec::new() // Default: no indicator values
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

    /// Rebalance position for a symbol with a specific execution type.
    ///
    /// This handles the full rebalance logic: cancel old orders, update target,
    /// calculate delta, determine order price based on execution type (Market vs Limit),
    /// split large orders, and place new orders.
    ///
    /// Default implementation: set target + simple market rebalance via buy/sell.
    /// `BaseStrategy` overrides this with full limit order support, order splitting,
    /// and pending order tracking.
    fn rebalance_symbol_with_execution(
        &mut self,
        _vt_symbol: &str,
        _target: f64,
        _price: f64,
        _bid_price: f64,
        _ask_price: f64,
        _execution_type: ExecutionType,
    ) {
        // Default: no-op. BaseStrategy provides the full implementation.
    }

    /// Called when a registered indicator updates (optional override)
    fn on_indicator(&mut self, _indicator_name: &str, _value: f64) {}

    /// Called when bars from all subscribed intervals are synchronized at the same time boundary.
    ///
    /// `bars` is keyed by interval string (e.g., "1m", "5m", "1h"). This callback fires only
    /// after all intervals that the strategy has subscribed to (via `register_bar_synthesizer`)
    /// have produced a completed bar for the current sync window.
    ///
    /// This is the primary callback for multi-timeframe strategies that need to combine
    /// signals from different bar intervals (e.g., trend from 1h + entry from 5m).
    fn on_bars_sync(&mut self, _bars: &HashMap<String, BarData>, _context: &StrategyContext) {}

    /// Called when a scheduled timer fires (optional override)
    fn on_timer(&mut self, _timer_id: &str) {}

    /// Called by StrategyEngine to write PnL data into the strategy.
    ///
    /// BaseStrategy implements this to update its `avg_entry_prices`,
    /// `unrealized_pnls`, `realized_pnls`, and `total_realized_pnl` fields.
    /// Other implementations (e.g., PythonStrategyAdapter) use a no-op default
    /// because they manage positions independently.
    fn update_pnl_fields(&mut self, _vt_symbol: &str, _avg_entry: f64, _unrealized: f64, _realized: f64, _total_realized: f64) {
        // Default no-op: PythonStrategyAdapter and MockStrategy don't use BaseStrategy PnL fields
    }

    /// Called when a risk alert is triggered (circuit breaker, max daily loss, etc.)
    fn on_risk_alert(&mut self, _reason: &str) {}

    /// Set strategy parameters from an optimization result
    /// Default implementation does nothing; override in strategies that support it
    fn set_parameters(&mut self, _params: &HashMap<String, f64>) {
        // Default: no-op. Strategies should override this to apply parameters.
    }

    /// Reset daily risk statistics (called at midnight or on strategy reset)
    fn reset_daily_risk_stats(&mut self) {}

    /// Get trailing stop configuration for a symbol (if any)
    fn get_trailing_stop(&self, _vt_symbol: &str) -> Option<TrailingStopConfig> {
        None
    }

    /// Cancel trailing stop for a symbol
    fn cancel_trailing_stop(&mut self, _vt_symbol: &str) {}

    /// Set a trailing stop for a position
    fn set_trailing_stop(
        &mut self,
        _vt_symbol: &str,
        _direction: Direction,
        _activation_price: f64,
        _trailing_distance: f64,
        _is_percentage: bool,
    ) {
    }

    /// Set strategy parameters from optimization results (numeric values).
    /// Default implementation does nothing; override in strategies that support
    /// runtime parameterization from the optimization engine.
    fn set_optimized_parameters(&mut self, _parameters: &HashMap<String, f64>) {}

    #[cfg(feature = "gui")]
    fn register_indicator_for_bars(
        &self,
        context: &StrategyContext,
        vt_symbol: &str,
        indicator: Box<dyn crate::chart::Indicator>,
    ) -> IndicatorRef {
        // Box<dyn chart::Indicator> implements StrategyIndicator via the adapter above,
        // so we can coerce it to Box<dyn StrategyIndicator>.
        let strategy_indicator: Box<dyn StrategyIndicator> = Box::new(indicator);
        context.register_indicator(vt_symbol, strategy_indicator)
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

    // Per-symbol PnL tracking (written by StrategyEngine on trade events)
    pub avg_entry_prices: Arc<Mutex<HashMap<String, f64>>>,
    pub unrealized_pnls: Arc<Mutex<HashMap<String, f64>>>,
    pub realized_pnls: Arc<Mutex<HashMap<String, f64>>>,
    pub total_realized_pnl: Arc<Mutex<f64>>,

    /// Per-interval ArrayManagers for multi-timeframe indicator calculation
    pub array_managers: Arc<Mutex<HashMap<String, ArrayManager>>>,

    // Trailing stop configuration per symbol
    pub trailing_stops: Arc<Mutex<HashMap<String, TrailingStopConfig>>>,

    // Daily risk statistics
    pub daily_risk_stats: Arc<Mutex<DailyRiskStats>>,

    // Trading parameters
    pub parameters: HashMap<String, String>,
    pub variables: HashMap<String, String>,

    // Rebalance execution settings
    /// Default execution type for rebalance orders
    pub default_execution_type: Arc<Mutex<ExecutionType>>,
    /// Maximum order size before splitting (0 = no splitting)
    pub max_order_size: Arc<Mutex<f64>>,
    /// Pending rebalance order IDs per symbol (for cancellation on target change)
    pub pending_rebalance_orders: Arc<Mutex<HashMap<String, Vec<String>>>>,
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
            avg_entry_prices: Arc::new(Mutex::new(HashMap::new())),
            unrealized_pnls: Arc::new(Mutex::new(HashMap::new())),
            realized_pnls: Arc::new(Mutex::new(HashMap::new())),
            total_realized_pnl: Arc::new(Mutex::new(0.0)),
            array_managers: Arc::new(Mutex::new(HashMap::new())),
            trailing_stops: Arc::new(Mutex::new(HashMap::new())),
            daily_risk_stats: Arc::new(Mutex::new(DailyRiskStats::default())),
            parameters,
            variables: HashMap::new(),
            default_execution_type: Arc::new(Mutex::new(ExecutionType::Market)),
            max_order_size: Arc::new(Mutex::new(0.0)),
            pending_rebalance_orders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Buy order (open long for futures, buy for spot)
    pub fn buy(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        let req = self.create_order_request(vt_symbol, Direction::Long, price, volume, lock, Offset::Open);
        let vt_orderid = format!("BUY_{}_{}", vt_symbol, Utc::now().timestamp_millis());
        self.pending_orders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(req);
        vt_orderid
    }

    /// Create an `OrderRequest` from the given parameters
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
            expire_time: None,
            gateway_name: String::new(),
        }
    }

    /// Cancel order
    pub fn cancel_order(&self, vt_orderid: &str) {
        tracing::info!("请求取消委托: {}", vt_orderid);
        // Remove from active orderids
        let mut orderids = self
            .active_orderids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        orderids.retain(|id| id != vt_orderid);
        // Queue cancellation request for engine processing
        self.pending_cancellations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(CancelRequestType::Order(vt_orderid.to_string()));
    }

    /// Cancel all orders
    pub fn cancel_all(&self) {
        let orderids = self
            .active_orderids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for orderid in orderids.iter() {
            self.cancel_order(orderid);
        }
    }

    /// Drain pending orders (called by engine after `on_bar`/`on_tick` callback)
    pub fn drain_pending_orders(&self) -> Vec<OrderRequest> {
        let mut orders = self
            .pending_orders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *orders)
    }

    /// Drain pending stop orders (called by engine after `on_bar`/`on_tick` callback)
    pub fn drain_pending_stop_orders(&self) -> Vec<StopOrderRequest> {
        let mut orders = self
            .pending_stop_orders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *orders)
    }

    /// Drain pending cancellations (called by engine after `on_bar`/`on_tick` callback)
    pub fn drain_pending_cancellations(&self) -> Vec<CancelRequestType> {
        let mut cancellations = self
            .pending_cancellations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *cancellations)
    }

    /// Load historical bar data (placeholder — use `StrategyContext`.`load_bar` instead)
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(stop_orderid.clone());

        // Queue stop order request for engine processing
        self.pending_stop_orders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        orderids.retain(|id| id != stop_orderid);

        // Queue cancellation request for engine processing
        self.pending_cancellations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(CancelRequestType::StopOrder(stop_orderid.to_string()));
    }

    /// Write log
    pub fn write_log(&self, msg: &str) {
        tracing::info!("[{}] {}", self.strategy_name, msg);
    }

    /// Set strategy parameters from an optimization result
    ///
    /// Merges the provided parameters into the strategy's `parameters` map,
    /// converting f64 values to serde_json::Value.
    pub fn set_parameters(&mut self, params: &HashMap<String, f64>) {
        for (k, &v) in params {
            self.parameters.insert(k.clone(), v.to_string());
        }
    }

    /// Set strategy parameters from string key-value pairs
    ///
    /// Merges the provided parameters into the strategy's `parameters` map.
    pub fn set_parameters_from_strings(&mut self, params: HashMap<String, String>) {
        for (k, v) in params {
            self.parameters.insert(k, v);
        }
    }

    /// Get engine type
    pub fn get_engine_type(&self) -> &str {
        "LIVE" // or "BACKTESTING"
    }

    /// Synchronize position data from trading
    pub fn sync_position(&mut self, vt_symbol: &str, position: f64) {
        self.positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(vt_symbol.to_string(), position);
    }

    // ========================================================================
    // Rebalance / target position methods
    // ========================================================================

    /// Set target position for a symbol
    pub fn set_target(&self, vt_symbol: &str, target: f64) {
        self.targets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(vt_symbol.to_string(), target);
    }

    /// Get target position for a symbol
    pub fn get_target(&self, vt_symbol: &str) -> f64 {
        self.targets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get current position for a symbol
    pub fn get_position(&self, vt_symbol: &str) -> f64 {
        self.positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }

    /// Rebalance position to target using market orders (convenience method)
    ///
    /// Calculates the delta between current position and target, then places
    /// market orders to reach the target.
    pub fn rebalance_symbol(&self, vt_symbol: &str, target: f64, price: f64) {
        self.rebalance_symbol_with_execution(vt_symbol, target, price, price, price, ExecutionType::Market);
    }

    /// Rebalance position to target using specified execution type
    ///
    /// When `execution_type` is `Limit`:
    ///   - Buy orders use `ask_price` (pay the ask for immediate fill)
    ///   - Sell orders use `bid_price` (sell at the bid for immediate fill)
    ///   - Short orders use `bid_price`
    ///   - Cover orders use `ask_price`
    /// When `Market`: uses `price` for all orders.
    ///
    /// Large orders above `max_order_size` are automatically split.
    /// Any existing stop orders and pending rebalance orders for the symbol
    /// are cancelled before new orders are placed.
    pub fn rebalance_symbol_with_execution(
        &self,
        vt_symbol: &str,
        target: f64,
        price: f64,
        bid_price: f64,
        ask_price: f64,
        execution_type: ExecutionType,
    ) {
        // Cancel existing stop orders and pending rebalance orders first
        self.cancel_stop_orders(vt_symbol);
        self.cancel_rebalance_orders(vt_symbol);

        // Update target
        self.set_target(vt_symbol, target);

        // Calculate delta
        let current_pos = self.get_position(vt_symbol);
        let delta = target - current_pos;

        if delta.abs() < 1e-10 {
            return;
        }

        // Determine order price based on execution type
        let order_price = match execution_type {
            ExecutionType::Market => price,
            ExecutionType::Limit => {
                if delta > 0.0 {
                    // Buying: use ask price (or lower if price is more favorable)
                    ask_price.min(price)
                } else {
                    // Selling: use bid price (or higher if price is more favorable)
                    bid_price.max(price)
                }
            }
        };

        // Split large orders
        let volume = delta.abs();
        let sub_orders = self.split_order(volume);

        // Track new rebalance order IDs
        let mut new_order_ids = Vec::new();

        for sub_volume in sub_orders {
            let vt_orderid = if delta > 0.0 {
                // Need to increase long position
                if current_pos >= 0.0 {
                    // Currently long or flat — buy to open/increase
                    self.buy(vt_symbol, order_price, sub_volume, false)
                } else {
                    // Currently short — buy to cover (close) first
                    self.cover(vt_symbol, order_price, sub_volume, false)
                }
            } else {
                // Need to decrease position (delta < 0)
                if current_pos > 0.0 {
                    // Currently long — sell to close
                    self.sell(vt_symbol, order_price, sub_volume, false)
                } else {
                    // Currently short or flat — short to open/increase short
                    self.short(vt_symbol, order_price, sub_volume, false)
                }
            };
            if !vt_orderid.is_empty() {
                new_order_ids.push(vt_orderid);
            }
        }

        // Store pending rebalance order IDs for this symbol
        if !new_order_ids.is_empty() {
            self.pending_rebalance_orders
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(vt_symbol.to_string(), new_order_ids);
        }
    }

    /// Split a large order into multiple smaller orders
    ///
    /// If `max_order_size` is 0 or the volume fits within the limit,
    /// returns a single-element vector. Otherwise splits into chunks.
    fn split_order(&self, volume: f64) -> Vec<f64> {
        let max_size = *self.max_order_size
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if max_size <= 0.0 || volume <= max_size {
            return vec![volume];
        }
        let mut orders = Vec::new();
        let mut remaining = volume;
        while remaining > 0.0 {
            let order_size = remaining.min(max_size);
            orders.push(order_size);
            remaining -= order_size;
        }
        orders
    }

    /// Cancel all stop orders for a symbol
    pub fn cancel_stop_orders(&self, vt_symbol: &str) {
        let stop_orderids: Vec<String> = self
            .active_stop_orderids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|id| id.contains(vt_symbol))
            .cloned()
            .collect();

        for stop_orderid in stop_orderids {
            self.cancel_stop_order(&stop_orderid);
        }
    }

    /// Cancel all active (unfilled) orders for a symbol before rebalancing
    pub fn cancel_active_orders(&self, vt_symbol: &str) {
        // Cancel stop orders for the symbol
        self.cancel_stop_orders(vt_symbol);
        // Cancel tracked rebalance orders
        self.cancel_rebalance_orders(vt_symbol);
        // Also cancel any active regular orders for the symbol
        let orderids: Vec<String> = self
            .active_orderids
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|id| id.contains(vt_symbol))
            .cloned()
            .collect();
        for orderid in orderids {
            self.cancel_order(&orderid);
        }
    }

    /// Cancel all pending rebalance orders for a symbol
    ///
    /// Removes tracking entries and queues cancellation requests
    /// for any orders that the engine can cancel.
    pub fn cancel_rebalance_orders(&self, vt_symbol: &str) {
        let orders = self.pending_rebalance_orders
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(vt_symbol);

        if let Some(order_ids) = orders {
            for orderid in &order_ids {
                // Queue cancellation via the existing cancel_order mechanism
                self.cancel_order(orderid);
            }
        }
    }

    /// Get or create the ArrayManager for a specific interval.
    ///
    /// The key is the interval string (e.g., "1m", "5m", "1h").
    /// If no ArrayManager exists for the given interval, one is created with default size (100).
    /// Returns a `MutexGuard` providing access to the internal HashMap; use
    /// `guard.get(interval)` or `guard.get_mut(interval)` to access the desired ArrayManager.
    pub fn get_array_managers(&self) -> std::sync::MutexGuard<'_, HashMap<String, ArrayManager>> {
        self.array_managers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Ensure an ArrayManager exists for the given interval and return whether it's initialized.
    ///
    /// Creates a new ArrayManager with size 100 if one doesn't exist for the interval.
    /// Returns `true` if the ArrayManager has enough data for indicator calculations.
    pub fn ensure_array_manager(&self, interval: &str) -> bool {
        let mut managers = self.array_managers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let am = managers.entry(interval.to_string())
            .or_insert_with(|| ArrayManager::new(100));
        am.is_inited()
    }

    /// Check if the ArrayManager for a specific interval is initialized (has enough data)
    pub fn is_array_manager_inited(&self, interval: &str) -> bool {
        let managers = self.array_managers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        managers.get(interval).map_or(false, |am| am.is_inited())
    }

    /// Update the ArrayManager for a specific interval with a new bar.
    ///
    /// Creates a new ArrayManager with size 100 if one doesn't exist for the interval.
    pub fn update_array_manager(&self, interval: &str, bar: &BarData) {
        let mut managers = self.array_managers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        managers.entry(interval.to_string())
            .or_insert_with(|| ArrayManager::new(100))
            .update_bar(bar);
    }

    // ========================================================================
    // Per-symbol PnL accessors
    // ========================================================================

    /// Get average entry price for a symbol
    pub fn get_avg_entry_price(&self, vt_symbol: &str) -> f64 {
        self.avg_entry_prices
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get unrealized PnL for a symbol
    pub fn get_unrealized_pnl(&self, vt_symbol: &str) -> f64 {
        self.unrealized_pnls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get realized PnL for a symbol
    pub fn get_realized_pnl(&self, vt_symbol: &str) -> f64 {
        self.realized_pnls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }

    /// Get total realized PnL across all symbols
    pub fn get_total_realized_pnl(&self) -> f64 {
        *self.total_realized_pnl
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Get total unrealized PnL across all symbols
    pub fn get_total_unrealized_pnl(&self) -> f64 {
        self.unrealized_pnls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .sum()
    }

    /// Internal: write PnL fields (called by StrategyEngine on trade events)
    pub(crate) fn write_pnl_fields(&self, vt_symbol: &str, avg_entry: f64, unrealized: f64, realized: f64, total_realized: f64) {
        let mut avg_prices = self.avg_entry_prices.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut unrealized_map = self.unrealized_pnls.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut realized_map = self.realized_pnls.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut total = self.total_realized_pnl.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        if avg_entry == 0.0 {
            avg_prices.remove(vt_symbol);
        } else {
            avg_prices.insert(vt_symbol.to_string(), avg_entry);
        }
        unrealized_map.insert(vt_symbol.to_string(), unrealized);
        realized_map.insert(vt_symbol.to_string(), realized);
        *total = total_realized;
    }

    // ========================================================================
    // Trailing stop methods
    // ========================================================================

    /// Set a trailing stop for a position. When price moves favorably by trailing_distance,
    /// the stop price follows. Only activates once price reaches activation_price.
    pub fn set_trailing_stop(
        &self,
        vt_symbol: &str,
        direction: Direction,
        activation_price: f64,
        trailing_distance: f64,
        is_percentage: bool,
    ) {
        let current_stop = if direction == Direction::Long {
            activation_price
                - if is_percentage {
                    activation_price * trailing_distance / 100.0
                } else {
                    trailing_distance
                }
        } else {
            activation_price
                + if is_percentage {
                    activation_price * trailing_distance / 100.0
                } else {
                    trailing_distance
                }
        };
        let config = TrailingStopConfig {
            activation_price,
            trailing_distance,
            is_percentage,
            current_stop_price: current_stop,
            direction,
        };
        self.trailing_stops
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(vt_symbol.to_string(), config);
    }

    /// Cancel trailing stop for a symbol
    pub fn cancel_trailing_stop(&self, vt_symbol: &str) {
        self.trailing_stops
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(vt_symbol);
    }

    /// Get the current trailing stop config for a symbol
    pub fn get_trailing_stop(&self, vt_symbol: &str) -> Option<TrailingStopConfig> {
        self.trailing_stops
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .cloned()
    }

    // ========================================================================
    // ATR dynamic stop
    // ========================================================================

    /// Set stop loss using ATR-based dynamic distance.
    /// Calculates stop as: entry_price ± (atr_multiplier × atr_value)
    /// Delegates to `send_stop_order` so the engine monitors the stop price.
    pub fn set_atr_stop(
        &self,
        vt_symbol: &str,
        direction: Direction,
        entry_price: f64,
        atr_value: f64,
        atr_multiplier: f64,
        volume: f64,
        offset: Option<Offset>,
    ) -> String {
        let stop_distance = atr_value * atr_multiplier;
        let stop_price = if direction == Direction::Long {
            entry_price - stop_distance
        } else {
            entry_price + stop_distance
        };
        // For a long position, the stop order direction is Short (sell to close)
        // For a short position, the stop order direction is Long (buy to cover)
        let stop_direction = if direction == Direction::Long {
            Direction::Short
        } else {
            Direction::Long
        };
        self.send_stop_order(vt_symbol, stop_price, volume, stop_direction, offset)
    }

    // ========================================================================
    // Daily risk stats
    // ========================================================================

    /// Reset daily risk statistics (called at midnight or on strategy reset)
    pub fn reset_daily_risk_stats(&mut self) {
        let mut stats = self
            .daily_risk_stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        stats.trade_count = 0;
        stats.daily_pnl = 0.0;
        stats.last_reset_date = chrono::Local::now().format("%Y-%m-%d").to_string();
    }

    /// Get a copy of the current daily risk stats
    pub fn get_daily_risk_stats(&self) -> DailyRiskStats {
        self.daily_risk_stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Increment daily trade count (called by StrategyEngine after each trade)
    #[allow(dead_code)]
    pub(crate) fn increment_daily_trade_count(&self) {
        let mut stats = self
            .daily_risk_stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        stats.trade_count += 1;
    }

    /// Add to daily realized PnL (called by StrategyEngine after each trade)
    #[allow(dead_code)]
    pub(crate) fn add_daily_pnl(&self, pnl: f64) {
        let mut stats = self
            .daily_risk_stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        stats.daily_pnl += pnl;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::database::MemoryDatabase;

    fn create_test_base_strategy() -> BaseStrategy {
        let mut setting = StrategySetting::new();
        setting.insert("param1".to_string(), serde_json::Value::from("value1"));
        BaseStrategy::new(
            "TestStrategy".to_string(),
            vec!["BTCUSDT.BINANCE".to_string()],
            StrategyType::Futures,
            setting,
        )
    }

    fn create_test_tick(_vt_symbol: &str) -> TickData {
        let mut tick = TickData::new(
            "TEST".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );
        tick.bid_price_1 = 50000.0;
        tick.ask_price_1 = 50001.0;
        tick.last_price = 50000.5;
        tick.volume = 1000.0;
        tick
    }

    fn create_test_bar() -> BarData {
        let mut bar = BarData::new(
            "TEST".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );
        bar.interval = Some(Interval::Minute);
        bar.open_price = 50000.0;
        bar.high_price = 50100.0;
        bar.low_price = 49900.0;
        bar.close_price = 50050.0;
        bar.volume = 1000.0;
        bar
    }

    #[test]
    fn test_strategy_context_new() {
        let ctx = StrategyContext::new();
        // Caches should be empty
        assert!(ctx.get_tick("BTCUSDT.BINANCE").is_none());
        assert!(ctx.get_bar("BTCUSDT.BINANCE").is_none());
    }

    #[test]
    fn test_strategy_context_get_tick_missing() {
        let ctx = StrategyContext::new();
        assert!(ctx.get_tick("NONEXISTENT").is_none());
    }

    #[test]
    fn test_strategy_context_get_bar_missing() {
        let ctx = StrategyContext::new();
        assert!(ctx.get_bar("NONEXISTENT").is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_strategy_context_tick_caching() {
        let ctx = StrategyContext::new();
        let tick = create_test_tick("BTCUSDT.BINANCE");
        let vt_symbol = tick.vt_symbol();

        ctx.update_tick(tick);

        let retrieved = ctx.get_tick(&vt_symbol);
        assert!(retrieved.is_some());
        let t = retrieved.unwrap();
        assert!((t.bid_price_1 - 50000.0).abs() < 0.01);
        assert!((t.ask_price_1 - 50001.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_strategy_context_bar_caching() {
        let ctx = StrategyContext::new();
        let bar = create_test_bar();
        let vt_symbol = bar.vt_symbol();

        ctx.update_bar(bar);

        let retrieved = ctx.get_bar(&vt_symbol);
        assert!(retrieved.is_some());
        let b = retrieved.unwrap();
        assert!((b.close_price - 50050.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_base_strategy_buy() {
        let strategy = create_test_base_strategy();
        strategy.buy("BTCUSDT.BINANCE", 50000.0, 1.0, false);

        let orders = strategy.drain_pending_orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
        assert_eq!(orders[0].offset, Offset::Open);
        assert!((orders[0].price - 50000.0).abs() < 0.01);
        assert!((orders[0].volume - 1.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_base_strategy_sell() {
        let strategy = create_test_base_strategy();
        strategy.sell("BTCUSDT.BINANCE", 49000.0, 1.0, false);

        let orders = strategy.drain_pending_orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Short);
        assert_eq!(orders[0].offset, Offset::Close); // Futures strategy
        assert!((orders[0].price - 49000.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_base_strategy_short() {
        let strategy = create_test_base_strategy();
        strategy.short("BTCUSDT.BINANCE", 50000.0, 2.0, false);

        let orders = strategy.drain_pending_orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Short);
        assert_eq!(orders[0].offset, Offset::Open);
        assert!((orders[0].volume - 2.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_base_strategy_cover() {
        let strategy = create_test_base_strategy();
        strategy.cover("BTCUSDT.BINANCE", 51000.0, 1.0, false);

        let orders = strategy.drain_pending_orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
        assert_eq!(orders[0].offset, Offset::Close); // Not locked
        assert!((orders[0].price - 51000.0).abs() < 0.01);
    }

    #[test]
    fn test_base_strategy_cancel_order() {
        let strategy = create_test_base_strategy();

        // Add an active order ID first
        strategy
            .active_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push("ORDER_123".to_string());

        strategy.cancel_order("ORDER_123");

        let cancellations = strategy.drain_pending_cancellations();
        assert_eq!(cancellations.len(), 1);
        match &cancellations[0] {
            CancelRequestType::Order(id) => assert_eq!(id, "ORDER_123"),
            CancelRequestType::StopOrder(_) => panic!("Expected Order cancellation"),
        }

        // Active order should be removed
        let orderids = strategy
            .active_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(!orderids.contains(&"ORDER_123".to_string()));
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_base_strategy_send_stop_order() {
        let strategy = create_test_base_strategy();
        let stop_id = strategy.send_stop_order(
            "BTCUSDT.BINANCE",
            49000.0,
            1.0,
            Direction::Short,
            Some(Offset::Close),
        );

        assert!(!stop_id.is_empty());

        let stop_orders = strategy.drain_pending_stop_orders();
        assert_eq!(stop_orders.len(), 1);
        assert!((stop_orders[0].price - 49000.0).abs() < 0.01);
        assert_eq!(stop_orders[0].direction, Direction::Short);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_base_strategy_set_get_target() {
        let strategy = create_test_base_strategy();

        // Initially no target
        let targets = strategy.targets.lock().unwrap_or_else(|e| e.into_inner());
        assert!(targets.get("BTCUSDT.BINANCE").is_none());
        drop(targets);

        // Set target via targets field
        strategy
            .targets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("BTCUSDT.BINANCE".to_string(), 5.0);

        // Retrieve target
        let targets = strategy.targets.lock().unwrap_or_else(|e| e.into_inner());
        let target = targets.get("BTCUSDT.BINANCE");
        assert!(target.is_some());
        assert!((target.unwrap() - 5.0).abs() < 0.01);
    }

    #[test]
    fn test_strategy_context_with_database() {
        let db = Arc::new(MemoryDatabase::new()) as Arc<dyn BaseDatabase>;
        let ctx = StrategyContext::with_database(db);

        // Should be able to create context with database
        assert!(ctx.get_tick("BTCUSDT.BINANCE").is_none());
        assert!(ctx.get_bar("BTCUSDT.BINANCE").is_none());
    }
}
