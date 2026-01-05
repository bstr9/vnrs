//! Strategy Template
//! 
//! Abstract base template for implementing trading strategies

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use chrono::Utc;

use crate::trader::{
    TickData, BarData, OrderData, TradeData, 
    Direction, Offset, Interval
};
use super::base::{StrategyType, StrategyState, StrategySetting};

/// Strategy context providing market data and trading interface
pub struct StrategyContext {
    /// Current tick data cache
    pub tick_cache: Arc<Mutex<HashMap<String, TickData>>>,
    /// Current bar data cache
    pub bar_cache: Arc<Mutex<HashMap<String, BarData>>>,
    /// Historical bars for each symbol
    pub historical_bars: Arc<Mutex<HashMap<String, Vec<BarData>>>>,
}

impl StrategyContext {
    pub fn new() -> Self {
        Self {
            tick_cache: Arc::new(Mutex::new(HashMap::new())),
            bar_cache: Arc::new(Mutex::new(HashMap::new())),
            historical_bars: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get latest tick for symbol
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        self.tick_cache.lock().unwrap().get(vt_symbol).cloned()
    }

    /// Get latest bar for symbol
    pub fn get_bar(&self, vt_symbol: &str) -> Option<BarData> {
        self.bar_cache.lock().unwrap().get(vt_symbol).cloned()
    }

    /// Get historical bars for symbol
    pub fn get_bars(&self, vt_symbol: &str, count: usize) -> Vec<BarData> {
        if let Some(bars) = self.historical_bars.lock().unwrap().get(vt_symbol) {
            let start = bars.len().saturating_sub(count);
            bars[start..].to_vec()
        } else {
            Vec::new()
        }
    }

    /// Update tick data
    pub fn update_tick(&self, tick: TickData) {
        self.tick_cache.lock().unwrap().insert(tick.vt_symbol(), tick);
    }

    /// Update bar data
    pub fn update_bar(&self, bar: BarData) {
        let vt_symbol = bar.vt_symbol();
        
        // Update cache
        self.bar_cache.lock().unwrap().insert(vt_symbol.clone(), bar.clone());
        
        // Update historical bars
        let mut historical = self.historical_bars.lock().unwrap();
        historical.entry(vt_symbol).or_insert_with(Vec::new).push(bar);
    }
}

impl Default for StrategyContext {
    fn default() -> Self {
        Self::new()
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
    fn get_target(&self, vt_symbol: &str) -> Option<f64> {
        None // Default implementation
    }

    /// Set target position
    fn set_target(&mut self, _vt_symbol: &str, _target: f64) {
        // Default implementation (do nothing)
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
    pub fn buy(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        // This will be called through the engine
        format!("BUY_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Sell order (close long for futures, sell for spot)
    pub fn sell(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        format!("SELL_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Short order (open short for futures, not supported for spot)
    pub fn short(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
        if self.strategy_type == StrategyType::Spot {
            tracing::warn!("Short not supported for spot trading");
            return String::new();
        }
        format!("SHORT_{}_{}", vt_symbol, Utc::now().timestamp_millis())
    }

    /// Cover order (close short for futures, not supported for spot)
    pub fn cover(&self, vt_symbol: &str, price: f64, volume: f64, lock: bool) -> String {
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
        let orderids = self.active_orderids.lock().unwrap();
        for orderid in orderids.iter() {
            self.cancel_order(orderid);
        }
    }

    /// Load historical bar data
    pub fn load_bar(
        &self,
        vt_symbol: &str,
        days: i64,
        interval: Interval,
    ) -> Vec<BarData> {
        // This will be implemented through the engine
        Vec::new()
    }

    /// Send stop order
    pub fn send_stop_order(
        &self,
        vt_symbol: &str,
        price: f64,
        volume: f64,
        direction: Direction,
        offset: Option<Offset>,
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
        self.positions.lock().unwrap().insert(vt_symbol.to_string(), position);
    }
}

/// Target position template for DMA/Grid strategies
pub trait TargetPosTemplate: StrategyTemplate {
    /// Calculate target positions
    fn calculate_target(&mut self, context: &StrategyContext);

    /// Rebalance positions to match targets
    fn rebalance_portfolio(&mut self);

    /// Get minimum order volume
    fn get_min_volume(&self, vt_symbol: &str) -> f64 {
        0.001 // Default minimum
    }
}
