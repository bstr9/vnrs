//! Strategy Template
//!
//! Abstract base template for implementing trading strategies

use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::base::{StrategySetting, StrategyState, StrategyType};
use crate::chart::Indicator;
use crate::trader::{BarData, Direction, Interval, Offset, OrderData, TickData, TradeData};

/// Type alias for the indicator storage map
type IndicatorMap = Arc<Mutex<HashMap<String, Vec<Box<dyn Indicator>>>>>;

/// Strategy context providing market data and trading interface
pub struct StrategyContext {
    /// Current tick data cache
    pub tick_cache: Arc<Mutex<HashMap<String, TickData>>>,
    /// Current bar data cache
    pub bar_cache: Arc<Mutex<HashMap<String, BarData>>>,
    /// Historical bars for each symbol
    pub historical_bars: Arc<Mutex<HashMap<String, Vec<BarData>>>>,
    /// Registered indicators, keyed by vt_symbol
    indicators: IndicatorMap,
}

impl StrategyContext {
    pub fn new() -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
            indicators: Arc::new(Mutex::new(HashMap::new())),
        }
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

    /// Register an indicator to receive bar updates for a specific symbol.
    /// Returns an IndicatorRef that can be used to query the indicator's state.
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

    /// Get all indicator references registered for a symbol
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

    /// Update all indicators for a symbol with new bar data.
    /// Should be called BEFORE strategy.on_bar() so indicators have latest values.
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

/// A reference to an indicator that can be queried safely.
///
/// Stores a key (vt_symbol), index, and a clone of the shared indicator map,
/// allowing concurrent read access to indicator state.
pub struct IndicatorRef {
    key: String,
    index: usize,
    indicators: IndicatorMap,
}

impl IndicatorRef {
    /// Check if the indicator is ready (has enough data)
    pub fn is_ready(&self) -> bool {
        let map = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .map(|i| i.is_ready())
            .unwrap_or(false)
    }

    /// Get the current value of the indicator
    pub fn current_value(&self) -> Option<f64> {
        let map = self.indicators.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&self.key)
            .and_then(|v| v.get(self.index))
            .and_then(|i| i.current_value())
    }

    /// Get indicator name
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

    /// Register an indicator for automatic bar updates (convenience method).
    /// Returns an IndicatorRef for querying the indicator's state.
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
            parameters,
            variables: HashMap::new(),
        }
    }

    /// Buy order (open long for futures, buy for spot)
    pub fn buy(&self, vt_symbol: &str, _price: f64, _volume: f64, _lock: bool) -> String {
        // This will be called through the engine
        format!("BUY_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Sell order (close long for futures, sell for spot)
    pub fn sell(&self, vt_symbol: &str, _price: f64, _volume: f64, _lock: bool) -> String {
        format!("SELL_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Short order (open short for futures, not supported for spot)
    pub fn short(&self, vt_symbol: &str, _price: f64, _volume: f64, _lock: bool) -> String {
        if self.strategy_type == StrategyType::Spot {
            tracing::warn!("Short not supported for spot trading");
            return String::new();
        }
        format!("SHORT_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Cover order (close short for futures, not supported for spot)
    pub fn cover(&self, vt_symbol: &str, _price: f64, _volume: f64, _lock: bool) -> String {
        if self.strategy_type == StrategyType::Spot {
            tracing::warn!("Cover not supported for spot trading");
            return String::new();
        }
        format!("COVER_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Cancel order
    pub fn cancel_order(&self, vt_orderid: &str) {
        tracing::info!("Cancelling order: {}", vt_orderid);
    }

    /// Cancel all orders
    pub fn cancel_all(&self) {
        let orderids = self
            .active_orderids
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for orderid in orderids.iter() {
            self.cancel_order(orderid);
        }
    }

    /// Load historical bar data
    pub fn load_bar(&self, _vt_symbol: &str, _days: i64, _interval: Interval) -> Vec<BarData> {
        // This will be implemented through the engine
        Vec::new()
    }

    /// Send stop order
    pub fn send_stop_order(
        &self,
        vt_symbol: &str,
        _price: f64,
        _volume: f64,
        _direction: Direction,
        _offset: Option<Offset>,
    ) -> String {
        format!("STOP_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Cancel stop order
    pub fn cancel_stop_order(&self, stop_orderid: &str) {
        tracing::info!("Cancelling stop order: {}", stop_orderid);
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
