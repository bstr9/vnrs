use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use chrono::{Utc, Duration};

use crate::trader::{
    MainEngine, TickData, OrderData, OrderRequest, TradeData, BarData, DepthData,
    SubscribeRequest, CancelRequest, HistoryRequest,
    Direction, Interval, Exchange, Offset, Status, BaseEngine, GatewayEvent,
    BarSynthesizer,
    EVENT_TICK, EVENT_BAR, EVENT_ORDER, EVENT_TRADE, EVENT_DEPTH,
};
use crate::trader::database::BaseDatabase;
use crate::event::EventEngine;
use super::template::{StrategyTemplate, StrategyContext};
use super::base::{
    StrategyState, StopOrder, StopOrderStatus, 
    StrategySetting, StopOrderRequest, CancelRequestType,
    StrategyRiskConfig,
};
#[cfg(feature = "python")]
use crate::python::strategy_adapter::PythonStrategyAdapter;

// Event type constants for strategy
pub const EVENT_STRATEGY_TICK: &str = "eStrategyTick";
pub const EVENT_STRATEGY_BAR: &str = "eStrategyBar";
pub const EVENT_STRATEGY_ORDER: &str = "eStrategyOrder";
pub const EVENT_STRATEGY_TRADE: &str = "eStrategyTrade";

/// Strategy engine managing all strategies
pub struct StrategyEngine {
    /// Main trading engine
    main_engine: Arc<MainEngine>,
    /// Event engine (DEPRECATED: kept for backward compatibility only)
    /// 
    /// **Note**: This sync EventEngine is never started. All event routing now flows
    /// through MainEngine's async event loop via `process_event()`. This field exists
    /// solely to maintain backward compatibility with existing code that passes an
    /// EventEngine to the constructor. It should be removed in a future major version.
    #[allow(dead_code)]
    event_engine: Arc<EventEngine>,
    
    /// Strategy instances
    strategies: Arc<RwLock<HashMap<String, Box<dyn StrategyTemplate>>>>,
    /// Strategy settings
    strategy_settings: Arc<RwLock<HashMap<String, StrategySetting>>>,
    
    /// Symbol to strategy mapping (one symbol can have multiple strategies)
    symbol_strategy_map: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// Order ID to strategy mapping
    orderid_strategy_map: Arc<RwLock<HashMap<String, String>>>,
    /// Strategy to order IDs mapping
    strategy_orderid_map: Arc<RwLock<HashMap<String, Vec<String>>>>,
    
    /// Stop orders
    stop_orders: Arc<RwLock<HashMap<String, StopOrder>>>,
    stop_order_count: Arc<Mutex<usize>>,
    
    /// Strategy contexts (market data cache)
    contexts: Arc<RwLock<HashMap<String, StrategyContext>>>,
    
    /// Processed trade IDs (for deduplication)
    processed_tradeids: Arc<RwLock<std::collections::HashSet<String>>>,
    /// Order tracker for LRU eviction of processed_tradeids
    processed_tradeids_order: Arc<RwLock<Vec<String>>>,
    
    /// Multi-period bar synthesis accumulators: (strategy_name, vt_symbol) �?(target_interval, bar_count, accumulated_bar)
    /// Accumulates 1-minute bars and delivers higher-timeframe bars when complete
    bar_synthesizers: Arc<RwLock<HashMap<(String, String), BarSynthesizer>>>,
    
    /// Per-strategy realized PnL: strategy_name �?realized PnL
    strategy_pnl: Arc<RwLock<HashMap<String, f64>>>,
    /// Per-strategy unrealized PnL: strategy_name �?unrealized PnL
    strategy_unrealized_pnl: Arc<RwLock<HashMap<String, f64>>>,
    /// Per-strategy trade count: strategy_name �?number of trades
    strategy_trade_count: Arc<RwLock<HashMap<String, usize>>>,
    /// Per-strategy average entry price: (strategy_name, vt_symbol) �?avg entry price
    strategy_avg_price: Arc<RwLock<HashMap<(String, String), f64>>>,
    /// Frozen close volume per strategy per symbol (to prevent double-closing)
    /// Key: strategy_name, Value: HashMap<vt_symbol, frozen_close_volume>
    strategy_frozen_closes: Arc<RwLock<HashMap<String, HashMap<String, f64>>>>,
    /// Maps vt_orderid → CloseOrderInfo for unfreezing on fill/cancel/reject
    order_close_info: Arc<RwLock<HashMap<String, CloseOrderInfo>>>,
    /// Per-strategy risk configuration (limits enforced before MainEngine)
    strategy_risk_configs: Arc<RwLock<HashMap<String, StrategyRiskConfig>>>,
    /// Optional database for loading historical data
    database: Option<Arc<dyn BaseDatabase>>,
}

/// Tracks a close order for frozen volume management
#[derive(Debug, Clone)]
struct CloseOrderInfo {
    strategy_name: String,
    vt_symbol: String,
    /// Original volume of the close order (for unfreezing on cancel/reject)
    volume: f64,
    /// Remaining unfilled volume (decremented on partial fills)
    remaining: f64,
}

impl StrategyEngine {
    pub fn new(main_engine: Arc<MainEngine>, event_engine: Arc<EventEngine>) -> Self {
        Self::with_database(main_engine, event_engine, None)
    }

    /// Create a StrategyEngine with an optional database backend
    pub fn with_database(
        main_engine: Arc<MainEngine>,
        event_engine: Arc<EventEngine>,
        database: Option<Arc<dyn BaseDatabase>>,
    ) -> Self {
        Self {
            main_engine,
            event_engine,
            strategies: Arc::new(RwLock::new(HashMap::new())),
            strategy_settings: Arc::new(RwLock::new(HashMap::new())),
            symbol_strategy_map: Arc::new(RwLock::new(HashMap::new())),
            orderid_strategy_map: Arc::new(RwLock::new(HashMap::new())),
            strategy_orderid_map: Arc::new(RwLock::new(HashMap::new())),
            stop_orders: Arc::new(RwLock::new(HashMap::new())),
            stop_order_count: Arc::new(Mutex::new(0)),
            contexts: Arc::new(RwLock::new(HashMap::new())),
            processed_tradeids: Arc::new(RwLock::new(std::collections::HashSet::new())),
            processed_tradeids_order: Arc::new(RwLock::new(Vec::new())),
            bar_synthesizers: Arc::new(RwLock::new(HashMap::new())),
            strategy_pnl: Arc::new(RwLock::new(HashMap::new())),
            strategy_unrealized_pnl: Arc::new(RwLock::new(HashMap::new())),
            strategy_trade_count: Arc::new(RwLock::new(HashMap::new())),
            strategy_avg_price: Arc::new(RwLock::new(HashMap::new())),
            strategy_frozen_closes: Arc::new(RwLock::new(HashMap::new())),
            order_close_info: Arc::new(RwLock::new(HashMap::new())),
            strategy_risk_configs: Arc::new(RwLock::new(HashMap::new())),
            database,
        }
    }

    /// Set the database backend
    pub fn set_database(&mut self, database: Arc<dyn BaseDatabase>) {
        self.database = Some(database);
    }

    /// Initialize the engine
    pub fn init(&self) {
        // Load strategy settings
        self.load_strategy_settings();
        
        tracing::info!("Strategy engine initialized successfully");
    }

    /// Process gateway events from MainEngine (BaseEngine implementation)
    /// Routes tick/bar/order/trade events to the appropriate strategies
    fn process_event_internal(&self, event_type: &str, event: &GatewayEvent) {
        match event_type {
            EVENT_TICK => {
                if let GatewayEvent::Tick(tick) = event {
                    self.process_tick_event(tick);
                }
            }
            EVENT_BAR => {
                if let GatewayEvent::Bar(bar) = event {
                    self.process_bar_event(bar);
                }
            }
            EVENT_ORDER => {
                if let GatewayEvent::Order(order) = event {
                    self.process_order_event(order);
                }
            }
            EVENT_TRADE => {
                if let GatewayEvent::Trade(trade) = event {
                    self.process_trade_event(trade);
                }
            }
            depth_type if depth_type.starts_with(EVENT_DEPTH) => {
                if let GatewayEvent::DepthBook(depth) = event {
                    self.process_depth_event(depth);
                }
            }
            _ => {}
        }
    }

    /// Process tick event and dispatch to subscribed strategies
    fn process_tick_event(&self, tick: &TickData) {
        let vt_symbol = tick.vt_symbol();
        let strategy_names: Vec<String> = self.symbol_strategy_map.read().unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol).cloned()
            .unwrap_or_default();

        for strategy_name in &strategy_names {
            let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
            if let Some(context) = contexts.get(strategy_name) {
                context.update_tick(tick.clone());
                let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
                if let Some(strategy) = strategies.get_mut(strategy_name) {
                    strategy.on_tick(tick, context);
                }
            }

            // Update unrealized PnL for this strategy on every tick
            let pos = self.strategies.read().unwrap_or_else(|e| e.into_inner())
                .get(strategy_name)
                .map(|s| s.get_position(&vt_symbol))
                .unwrap_or(0.0);

            if pos != 0.0 {
                let avg_entry = self.strategy_avg_price.read().unwrap_or_else(|e| e.into_inner())
                    .get(&(strategy_name.to_string(), vt_symbol.clone()))
                    .copied()
                    .unwrap_or(tick.last_price);

                let unrealized = if pos > 0.0 {
                    (tick.last_price - avg_entry) * pos
                } else {
                    (avg_entry - tick.last_price) * pos.abs()
                };

                self.strategy_unrealized_pnl.write().unwrap_or_else(|e| e.into_inner())
                    .insert(strategy_name.to_string(), unrealized);
            }
        }

        // Check stop orders
        {
            let mut stop_orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
            let mut triggered = Vec::new();
            for (stop_orderid, stop_order) in stop_orders.iter() {
                if stop_order.vt_symbol != vt_symbol { continue; }
                if stop_order.status != StopOrderStatus::Waiting { continue; }
                let should_trigger = match stop_order.direction {
                    Direction::Long => tick.last_price >= stop_order.price,
                    Direction::Short => tick.last_price <= stop_order.price,
                    Direction::Net => false,
                };
                if should_trigger {
                    triggered.push(stop_orderid.clone());
                }
            }
            for stop_orderid in triggered {
                if let Some(stop_order) = stop_orders.get_mut(&stop_orderid) {
                    stop_order.status = StopOrderStatus::Triggered;
                    let strategy_name = stop_order.strategy_name.clone();
                    // Notify the owning strategy about the stop order trigger
                    let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(strategy) = strategies.get_mut(&strategy_name) {
                        strategy.on_stop_order(&stop_orderid);
                    }
                }
            }
        }
    }

    /// Process bar event and dispatch to subscribed strategies
    fn process_bar_event(&self, bar: &BarData) {
        let vt_symbol = bar.vt_symbol();
        let strategy_names: Vec<String> = self.symbol_strategy_map.read().unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol).cloned()
            .unwrap_or_default();

        for strategy_name in &strategy_names {
            let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
            if let Some(context) = contexts.get(strategy_name) {
                context.update_bar(bar.clone());
                let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
                if let Some(strategy) = strategies.get_mut(strategy_name) {
                    strategy.on_bar(bar, context);
                }
            }
        }

        // Multi-period bar synthesis: feed base bars into synthesizers
        // and deliver synthesized higher-timeframe bars to strategies
        let mut synthesizers = self.bar_synthesizers.write().unwrap_or_else(|e| e.into_inner());
        for ((strategy_name, syn_vt_symbol), synthesizer) in synthesizers.iter_mut() {
            // Only process synthesizers that match this bar's symbol
            if syn_vt_symbol != &vt_symbol {
                continue;
            }

            // Feed the base bar into the synthesizer
            if let Some(synthesized_bar) = synthesizer.update_bar(bar) {
                // A higher-timeframe bar was completed �?deliver to strategy
                let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
                if let Some(context) = contexts.get(strategy_name) {
                    context.update_bar(synthesized_bar.clone());
                    let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(strategy) = strategies.get_mut(strategy_name) {
                        strategy.on_bar(&synthesized_bar, context);
                    }
                }
            }
        }
    }

    /// Process depth/order book event and dispatch to subscribed strategies
    fn process_depth_event(&self, depth: &DepthData) {
        let vt_symbol = depth.vt_symbol();
        let strategy_names: Vec<String> = self.symbol_strategy_map.read().unwrap_or_else(|e| e.into_inner())
            .get(&vt_symbol).cloned()
            .unwrap_or_default();

        for strategy_name in &strategy_names {
            let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
            if let Some(context) = contexts.get(strategy_name) {
                let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
                if let Some(strategy) = strategies.get_mut(strategy_name) {
                    strategy.on_depth(depth, context);
                }
            }
        }
    }

    /// Process order event and dispatch to owning strategy
    /// Also handles unfreezing of close volume when orders are cancelled or rejected
    fn process_order_event(&self, order: &OrderData) {
        let strategy_name = self.orderid_strategy_map.read().unwrap_or_else(|e| e.into_inner())
            .get(&order.vt_orderid())
            .cloned();

        if let Some(strategy_name) = strategy_name {
            // Unfreeze close volume on order cancellation or rejection
            if order.status == Status::Cancelled || order.status == Status::Rejected {
                let vt_orderid = order.vt_orderid();
                let mut close_info_map = self.order_close_info.write().unwrap_or_else(|e| e.into_inner());
                if let Some(close_info) = close_info_map.remove(&vt_orderid) {
                    // For cancelled/rejected orders, unfreeze the remaining volume
                    let remaining = close_info.remaining;
                    if remaining > 0.0 {
                        drop(close_info_map);
                        let mut frozen = self.strategy_frozen_closes.write().unwrap_or_else(|e| e.into_inner());
                        if let Some(strategy_frozen) = frozen.get_mut(&close_info.strategy_name) {
                            let current = strategy_frozen.get(&close_info.vt_symbol).copied().unwrap_or(0.0);
                            let new_val = (current - remaining).max(0.0);
                            if new_val > 0.0 {
                                strategy_frozen.insert(close_info.vt_symbol.clone(), new_val);
                            } else {
                                strategy_frozen.remove(&close_info.vt_symbol);
                            }
                        }
                        tracing::debug!(
                            "Unfrozen {} close volume for {} on {} due to order {:?}",
                            remaining, close_info.strategy_name, close_info.vt_symbol, order.status
                        );
                    }
                }
            } else if order.status == Status::PartTraded {
                // Update remaining volume in close order info on partial fill
                let mut close_info_map = self.order_close_info.write().unwrap_or_else(|e| e.into_inner());
                if let Some(close_info) = close_info_map.get_mut(&order.vt_orderid()) {
                    let filled = order.traded;
                    let old_remaining = close_info.remaining;
                    // remaining = original volume - total filled
                    close_info.remaining = (close_info.volume - filled).max(0.0);
                    let unfreeze_amount = old_remaining - close_info.remaining;
                    if unfreeze_amount > 0.0 {
                        let strategy_name = close_info.strategy_name.clone();
                        let vt_symbol = close_info.vt_symbol.clone();
                        drop(close_info_map);
                        let mut frozen = self.strategy_frozen_closes.write().unwrap_or_else(|e| e.into_inner());
                        if let Some(strategy_frozen) = frozen.get_mut(&strategy_name) {
                            let current = strategy_frozen.get(&vt_symbol).copied().unwrap_or(0.0);
                            let new_val = (current - unfreeze_amount).max(0.0);
                            if new_val > 0.0 {
                                strategy_frozen.insert(vt_symbol.clone(), new_val);
                            } else {
                                strategy_frozen.remove(&vt_symbol);
                            }
                        }
                    }
                }
            }

            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if let Some(strategy) = strategies.get_mut(&strategy_name) {
                strategy.on_order(order);
            }
        }
    }

    /// Process trade event and dispatch to owning strategy (with deduplication)
    /// Also tracks per-strategy realized PnL and unfreezes close volume
    fn process_trade_event(&self, trade: &TradeData) {
        // Deduplicate trades using LRU-style eviction instead of clear-all (#27)
        const TRADEID_CAPACITY: usize = 10000;
        let vt_tradeid = trade.vt_tradeid();
        {
            let mut processed = self.processed_tradeids.write().unwrap_or_else(|e| e.into_inner());
            if processed.contains(&vt_tradeid) {
                return;
            }
            processed.insert(vt_tradeid.clone());
            let mut order = self.processed_tradeids_order.write().unwrap_or_else(|e| e.into_inner());
            order.push(vt_tradeid);
            // Evict oldest 50% when over capacity (instead of clearing all)
            if order.len() > TRADEID_CAPACITY {
                let evict_count = TRADEID_CAPACITY / 2;
                let to_evict: Vec<String> = order.drain(..evict_count).collect();
                for id in &to_evict {
                    processed.remove(id);
                }
            }
        }

        let strategy_name = self.orderid_strategy_map.read().unwrap_or_else(|e| e.into_inner())
            .get(&trade.vt_orderid())
            .cloned();

        if let Some(strategy_name) = strategy_name {
            // Unfreeze close volume on trade fill
            if Self::is_close_offset(trade.offset) {
                let vt_orderid = trade.vt_orderid();
                let mut close_info_map = self.order_close_info.write().unwrap_or_else(|e| e.into_inner());
                if let Some(close_info) = close_info_map.get_mut(&vt_orderid) {
                    // Unfreeze the traded volume
                    let unfreeze_amount = trade.volume.min(close_info.remaining);
                    close_info.remaining = (close_info.remaining - unfreeze_amount).max(0.0);

                    let strategy_name_clone = close_info.strategy_name.clone();
                    let vt_symbol_clone = close_info.vt_symbol.clone();
                    let is_complete = close_info.remaining <= 0.0;
                    drop(close_info_map);

                    // Remove the close info if fully filled
                    if is_complete {
                        self.order_close_info.write().unwrap_or_else(|e| e.into_inner()).remove(&vt_orderid);
                    }

                    // Unfreeze the volume
                    if unfreeze_amount > 0.0 {
                        let mut frozen = self.strategy_frozen_closes.write().unwrap_or_else(|e| e.into_inner());
                        if let Some(strategy_frozen) = frozen.get_mut(&strategy_name_clone) {
                            let current = strategy_frozen.get(&vt_symbol_clone).copied().unwrap_or(0.0);
                            let new_val = (current - unfreeze_amount).max(0.0);
                            if new_val > 0.0 {
                                strategy_frozen.insert(vt_symbol_clone.clone(), new_val);
                            } else {
                                strategy_frozen.remove(&vt_symbol_clone);
                            }
                        }
                        tracing::debug!(
                            "Unfrozen {} close volume for {} on {} due to trade fill",
                            unfreeze_amount, strategy_name_clone, vt_symbol_clone
                        );
                    }
                }
            }

            let volume_change = match trade.direction {
                Some(Direction::Long) => trade.volume,
                Some(Direction::Short) => -trade.volume,
                Some(Direction::Net) => 0.0,
                None => 0.0,
            };

            // Calculate realized PnL from this trade
            let vt_symbol = trade.vt_symbol();
            let key = (strategy_name.clone(), vt_symbol.clone());
            let mut avg_prices = self.strategy_avg_price.write().unwrap_or_else(|e| e.into_inner());
            let mut pnl_map = self.strategy_pnl.write().unwrap_or_else(|e| e.into_inner());
            let mut trade_count_map = self.strategy_trade_count.write().unwrap_or_else(|e| e.into_inner());

            let current_pos = {
                let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
                strategies.get(&strategy_name)
                    .map(|s| s.get_position(&vt_symbol))
                    .unwrap_or(0.0)
            };

            // Calculate realized PnL when reducing position
            let mut realized_pnl = 0.0;
            if current_pos != 0.0 && volume_change != 0.0 {
                let is_closing = (current_pos > 0.0 && volume_change < 0.0)
                    || (current_pos < 0.0 && volume_change > 0.0);
                if is_closing {
                    let avg_entry = avg_prices.get(&key).copied().unwrap_or(trade.price);
                    let close_volume = volume_change.abs().min(current_pos.abs());
                    if current_pos > 0.0 {
                        // Long position closing: PnL = (trade price - avg entry) * close volume
                        realized_pnl = (trade.price - avg_entry) * close_volume;
                    } else {
                        // Short position closing: PnL = (avg entry - trade price) * close volume
                        realized_pnl = (avg_entry - trade.price) * close_volume;
                    }
                }
            }

            // Update average entry price
            let new_pos = current_pos + volume_change;
            if new_pos == 0.0 {
                avg_prices.remove(&key);
            } else if current_pos == 0.0 {
                // Opening new position
                avg_prices.insert(key.clone(), trade.price);
            } else if (current_pos > 0.0 && volume_change > 0.0) || (current_pos < 0.0 && volume_change < 0.0) {
                // Adding to position - recalculate average
                let old_avg = avg_prices.get(&key).copied().unwrap_or(trade.price);
                let new_avg = (old_avg * current_pos.abs() + trade.price * volume_change.abs())
                    / (current_pos.abs() + volume_change.abs());
                avg_prices.insert(key.clone(), new_avg);
            }

            // Update PnL tracking
            if realized_pnl != 0.0 {
                pnl_map.entry(strategy_name.clone())
                    .and_modify(|pnl| *pnl += realized_pnl)
                    .or_insert(realized_pnl);
            }
            trade_count_map.entry(strategy_name.clone())
                .and_modify(|c| *c += 1)
                .or_insert(1);

            drop(avg_prices);
            drop(pnl_map);
            drop(trade_count_map);

            // Update strategy position and notify
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if let Some(strategy) = strategies.get_mut(&strategy_name) {
                strategy.update_position(&vt_symbol, new_pos);
                strategy.on_trade(trade);
            }
        }
    }

    /// Add a strategy
    pub async fn add_strategy(
        &self,
        strategy: Box<dyn StrategyTemplate>,
        setting: StrategySetting,
    ) -> Result<(), String> {
        let strategy_name = strategy.strategy_name().to_string();
        
        // Check and insert atomically under write lock to prevent TOCTOU race
        {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if strategies.contains_key(&strategy_name) {
                return Err(format!("Strategy {} already exists", strategy_name));
            }
            strategies.insert(strategy_name.clone(), strategy);
        }

        // Create context for this strategy (with database if available)
        let context = match &self.database {
            Some(db) => StrategyContext::with_database(Arc::clone(db)),
            None => StrategyContext::new(),
        };
        
        // Subscribe to symbols - collect first to release lock before await
        let vt_symbols: Vec<String> = self.strategies.read().unwrap_or_else(|e| e.into_inner())
            .get(&strategy_name)
            .map(|s| s.vt_symbols().to_vec())
            .unwrap_or_default();

        for vt_symbol in vt_symbols {
            self.subscribe_symbol(&strategy_name, &vt_symbol).await;
        }

        self.strategy_settings.write().unwrap_or_else(|e| e.into_inner()).insert(strategy_name.clone(), setting);
        self.contexts.write().unwrap_or_else(|e| e.into_inner()).insert(strategy_name.clone(), context);

        tracing::info!("Strategy {} added successfully", strategy_name);
        Ok(())
    }

    /// Add a Python strategy adapter to the engine for live trading
    ///
    /// This method creates a `PythonStrategyAdapter` wrapping the Python strategy
    /// and inserts it into the engine so it receives live market data events
    /// through the normal StrategyEngine event routing path.
    #[cfg(feature = "python")]
    pub async fn add_python_strategy(
        &self,
        adapter: PythonStrategyAdapter,
        setting: StrategySetting,
    ) -> Result<(), String> {
        let strategy_name = adapter.strategy_name().to_string();

        // Check and insert atomically under write lock to prevent TOCTOU race
        {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if strategies.contains_key(&strategy_name) {
                return Err(format!("Strategy {} already exists", strategy_name));
            }
            strategies.insert(strategy_name.clone(), Box::new(adapter));
        }

        // Create context for this strategy (with database if available)
        let context = match &self.database {
            Some(db) => StrategyContext::with_database(Arc::clone(db)),
            None => StrategyContext::new(),
        };

        // Subscribe to symbols - collect first to release lock before await
        let vt_symbols: Vec<String> = self.strategies.read().unwrap_or_else(|e| e.into_inner())
            .get(&strategy_name)
            .map(|s| s.vt_symbols().to_vec())
            .unwrap_or_default();

        for vt_symbol in vt_symbols {
            self.subscribe_symbol(&strategy_name, &vt_symbol).await;
        }

        self.strategy_settings.write().unwrap_or_else(|e| e.into_inner()).insert(strategy_name.clone(), setting);
        self.contexts.write().unwrap_or_else(|e| e.into_inner()).insert(strategy_name.clone(), context);

        tracing::info!("Python strategy {} added successfully", strategy_name);
        Ok(())
    }

    /// Subscribe to market data for a symbol
    async fn subscribe_symbol(&self, strategy_name: &str, vt_symbol: &str) {
        // Parse vt_symbol (format: "symbol.exchange")
        let parts: Vec<&str> = vt_symbol.split('.').collect();
        if parts.len() != 2 {
            tracing::error!("Invalid vt_symbol format: {}", vt_symbol);
            return;
        }

        let symbol = parts[0].to_string();
        let exchange = match parts[1].to_uppercase().as_str() {
            "BINANCE" => Exchange::Binance,
            _ => {
                tracing::error!("Unsupported exchange: {}", parts[1]);
                return;
            }
        };

        // Subscribe through main engine
        let req = SubscribeRequest { symbol, exchange };
        if let Some(gw_name) = self.main_engine.find_gateway_name_for_exchange(exchange) {
            if let Err(e) = self.main_engine.subscribe(req, &gw_name).await {
                tracing::error!("Failed to subscribe {}: {}", vt_symbol, e);
            }
        } else {
            tracing::error!("No gateway found for exchange {:?}, cannot subscribe {}", exchange, vt_symbol);
        }

        // Update symbol-strategy mapping
        let mut map = self.symbol_strategy_map.write().unwrap_or_else(|e| e.into_inner());
        map.entry(vt_symbol.to_string())
            .or_insert_with(Vec::new)
            .push(strategy_name.to_string());
    }

    /// Initialize a strategy
    pub async fn init_strategy(&self, strategy_name: &str) -> Result<(), String> {
        // Check strategy exists and get context reference under lock
        let context_exists = {
            let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
            let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
            strategies.contains_key(strategy_name) && contexts.contains_key(strategy_name)
        };

        if !context_exists {
            if !self.strategies.read().unwrap_or_else(|e| e.into_inner()).contains_key(strategy_name) {
                return Err(format!("Strategy {} not found", strategy_name));
            } else {
                return Err(format!("Context not found for strategy {}", strategy_name));
            }
        }

        // Load historical data (no locks held across await)
        // Pass self to load_historical_data so it can acquire locks internally after each await
        self.load_historical_data(strategy_name).await?;

        // Initialize strategy
        {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
            if let Some(strategy) = strategies.get_mut(strategy_name) {
                if let Some(context) = contexts.get(strategy_name) {
                    strategy.on_init(context);
                }
            }
        }

        tracing::info!("Strategy {} initialized", strategy_name);
        Ok(())
    }

    /// Start a strategy
    pub fn start_strategy(&self, strategy_name: &str) -> Result<(), String> {
        let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());

        if let Some(strategy) = strategies.get_mut(strategy_name) {
            if strategy.state() != StrategyState::Inited {
                return Err(format!(
                    "Strategy {} not initialized, current state: {:?}",
                    strategy_name, strategy.state()
                ));
            }

            strategy.on_start();
            tracing::info!("Strategy {} started", strategy_name);
            Ok(())
        } else {
            Err(format!("Strategy {} not found", strategy_name))
        }
    }

    /// Stop a strategy
    pub async fn stop_strategy(&self, strategy_name: &str) -> Result<(), String> {
        {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());

            if let Some(strategy) = strategies.get_mut(strategy_name) {
                if strategy.state() != StrategyState::Trading {
                    return Err(format!(
                        "Strategy {} not trading, current state: {:?}",
                        strategy_name, strategy.state()
                    ));
                }

                strategy.on_stop();
            } else {
                return Err(format!("Strategy {} not found", strategy_name));
            }
        }

        self.cancel_all_orders(strategy_name).await;

        tracing::info!("Strategy {} stopped", strategy_name);
        Ok(())
    }

    /// Load historical data for strategy initialization
    ///
    /// Data source priority:
    /// 1. Database (if configured) — fast local access, no network calls
    /// 2. Gateway REST API — fallback when database has no data
    ///
    /// Loads 30 days of 1-minute bars by default and feeds them into the
    /// strategy's context for indicator warmup.
    async fn load_historical_data(
        &self,
        strategy_name: &str,
    ) -> Result<(), String> {
        // Collect vt_symbols under lock, then release before async work
        let vt_symbols: Vec<String> = {
            let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
            strategies.get(strategy_name)
                .map(|s| s.vt_symbols().to_vec())
                .unwrap_or_default()
        };
        
        for vt_symbol in &vt_symbols {
            let parts: Vec<&str> = vt_symbol.split('.').collect();
            if parts.len() != 2 {
                continue;
            }

            let symbol = parts[0].to_string();
            let exchange = crate::trader::utility::extract_vt_symbol(vt_symbol)
                .map(|(_, e)| e)
                .unwrap_or(Exchange::Binance);

            let end = Utc::now();
            let start = end - Duration::days(30);

            // Try loading from database first
            let mut bars_loaded = false;
            if let Some(db) = &self.database {
                match db.load_bar_data(&symbol, exchange, Interval::Minute, start, end).await {
                    Ok(bars) if !bars.is_empty() => {
                        tracing::info!(
                            "Loaded {} historical bars for {} from database",
                            bars.len(), vt_symbol
                        );
                        // Feed bars into context (short-lived lock, no await)
                        {
                            let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
                            if let Some(context) = contexts.get(strategy_name) {
                                for bar in &bars {
                                    context.update_bar(bar.clone());
                                }
                            }
                        }
                        bars_loaded = true;
                    }
                    Ok(_) => {
                        tracing::info!(
                            "No historical bars in database for {}, falling back to gateway",
                            vt_symbol
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to load history from database for {}: {}, falling back to gateway",
                            vt_symbol, e
                        );
                    }
                }
            }

            // Fallback to gateway query if database had no data
            if !bars_loaded {
                let req = HistoryRequest {
                    symbol,
                    exchange,
                    start,
                    end: Some(end),
                    interval: Some(Interval::Minute),
                };

                if let Some(gw_name) = self.main_engine.find_gateway_name_for_exchange(exchange) {
                    match self.main_engine.query_history(req, &gw_name).await {
                        Ok(bars) => {
                            tracing::info!(
                                "Loaded {} historical bars for {} from gateway",
                                bars.len(), vt_symbol
                            );
                            // Feed bars into context (short-lived lock, no await)
                            {
                                let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
                                if let Some(context) = contexts.get(strategy_name) {
                                    for bar in &bars {
                                        context.update_bar(bar.clone());
                                    }
                                }
                            }

                            // Save loaded bars to database for future warmups
                            if !bars.is_empty() {
                                if let Some(db) = &self.database {
                                    if let Err(e) = db.save_bar_data(bars, false).await {
                                        tracing::warn!(
                                            "Failed to cache historical bars for {}: {}",
                                            vt_symbol, e
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to load history for {} from gateway: {}",
                                vt_symbol, e
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Remove a strategy from the engine
    /// Stops the strategy if currently trading, then removes all associated mappings
    pub async fn remove_strategy(&self, strategy_name: &str) -> Result<(), String> {
        // Stop the strategy first if it's trading
        // Check state under lock, then release before async work
        let needs_stop = {
            let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
            match strategies.get(strategy_name) {
                Some(strategy) => strategy.state() == StrategyState::Trading,
                None => return Err(format!("Strategy {} not found", strategy_name)),
            }
        };
        if needs_stop {
            self.stop_strategy(strategy_name).await?;
        }

        // Cancel all open orders for this strategy
        self.cancel_all_orders(strategy_name).await;

        // Remove from strategies map
        {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            strategies.remove(strategy_name);
        }

        // Remove from symbol-strategy mapping
        {
            let mut map = self.symbol_strategy_map.write().unwrap_or_else(|e| e.into_inner());
            for strategies in map.values_mut() {
                strategies.retain(|name| name != strategy_name);
            }
            // Clean up empty entries
            map.retain(|_, strategies| !strategies.is_empty());
        }

        // Remove from orderid-strategy mapping
        {
            let mut orderid_map = self.orderid_strategy_map.write().unwrap_or_else(|e| e.into_inner());
            let mut strategy_map = self.strategy_orderid_map.write().unwrap_or_else(|e| e.into_inner());

            if let Some(orderids) = strategy_map.remove(strategy_name) {
                for orderid in &orderids {
                    orderid_map.remove(orderid);
                }
            }
        }

        // Remove from stop orders
        {
            let mut stop_orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
            stop_orders.retain(|_, so| so.strategy_name != strategy_name);
        }

        // Remove context and settings
        {
            let mut contexts = self.contexts.write().unwrap_or_else(|e| e.into_inner());
            contexts.remove(strategy_name);
        }
        {
            let mut settings = self.strategy_settings.write().unwrap_or_else(|e| e.into_inner());
            settings.remove(strategy_name);
        }

        // Remove PnL tracking data
        {
            let mut pnl = self.strategy_pnl.write().unwrap_or_else(|e| e.into_inner());
            pnl.remove(strategy_name);
        }
        {
            let mut unrealized = self.strategy_unrealized_pnl.write().unwrap_or_else(|e| e.into_inner());
            unrealized.remove(strategy_name);
        }
        {
            let mut trade_count = self.strategy_trade_count.write().unwrap_or_else(|e| e.into_inner());
            trade_count.remove(strategy_name);
        }
        {
            let mut avg_prices = self.strategy_avg_price.write().unwrap_or_else(|e| e.into_inner());
            avg_prices.retain(|(name, _), _| name != strategy_name);
        }

        // Remove frozen close tracking data
        {
            let mut frozen = self.strategy_frozen_closes.write().unwrap_or_else(|e| e.into_inner());
            frozen.remove(strategy_name);
        }
        {
            let mut close_info = self.order_close_info.write().unwrap_or_else(|e| e.into_inner());
            close_info.retain(|_, info| info.strategy_name != strategy_name);
        }

        // Unregister bar synthesizers for this strategy
        self.unregister_bar_synthesizers(strategy_name);

        // Remove risk config for this strategy
        self.remove_strategy_risk_config(strategy_name);

        tracing::info!("Strategy {} removed", strategy_name);
        Ok(())
    }

    /// Load historical bars for a strategy (public API for on_init warmup)
    ///
    /// This method can be called during strategy initialization to load historical
    /// bar data into the strategy's context. Data is loaded from the database first,
    /// falling back to the gateway REST API if the database has no data.
    ///
    /// # Arguments
    /// * `strategy_name` - Name of the strategy
    /// * `vt_symbol` - Symbol in "SYMBOL.EXCHANGE" format
    /// * `interval` - Bar interval (e.g., Interval::Minute)
    /// * `days` - Number of days of history to load
    ///
    /// # Returns
    /// Number of bars loaded, or error
    pub async fn load_bars(
        &self,
        strategy_name: &str,
        vt_symbol: &str,
        interval: Interval,
        days: i64,
    ) -> Result<usize, String> {
        let contexts = self.contexts.read().unwrap_or_else(|e| e.into_inner());
        let context = contexts.get(strategy_name)
            .ok_or_else(|| format!("Context not found for strategy {}", strategy_name))?;

        let parts: Vec<&str> = vt_symbol.split('.').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid vt_symbol format: {}", vt_symbol));
        }

        let symbol = parts[0].to_string();
        let exchange = crate::trader::utility::extract_vt_symbol(vt_symbol)
            .map(|(_, e)| e)
            .unwrap_or(Exchange::Binance);

        let end = Utc::now();
        let start = end - Duration::days(days);

        // Try database first
        if let Some(db) = &self.database {
            match db.load_bar_data(&symbol, exchange, interval, start, end).await {
                Ok(bars) if !bars.is_empty() => {
                    let count = bars.len();
                    for bar in &bars {
                        context.update_bar(bar.clone());
                    }
                    tracing::info!(
                        "load_bars: {} bars from database for {} ({:?}, {}d)",
                        count, vt_symbol, interval, days
                    );
                    return Ok(count);
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("load_bars: database error for {}: {}", vt_symbol, e);
                }
            }
        }

        // Fallback to gateway
        let req = HistoryRequest {
            symbol,
            exchange,
            start,
            end: Some(end),
            interval: Some(interval),
        };

        if let Some(gw_name) = self.main_engine.find_gateway_name_for_exchange(exchange) {
            match self.main_engine.query_history(req, &gw_name).await {
                Ok(bars) => {
                    let count = bars.len();
                    for bar in &bars {
                        context.update_bar(bar.clone());
                    }

                    // Cache to database for future loads
                    if !bars.is_empty() {
                        if let Some(db) = &self.database {
                            if let Err(e) = db.save_bar_data(bars, false).await {
                                tracing::warn!("load_bars: failed to cache for {}: {}", vt_symbol, e);
                            }
                        }
                    }

                    tracing::info!(
                        "load_bars: {} bars from gateway for {} ({:?}, {}d)",
                        count, vt_symbol, interval, days
                    );
                    Ok(count)
                }
                Err(e) => Err(format!("Failed to load bars for {}: {}", vt_symbol, e)),
            }
        } else {
            Err(format!("No gateway found for exchange {:?}", exchange))
        }
    }

    /// Cancel all orders for a strategy
    async fn cancel_all_orders(&self, strategy_name: &str) {
        // Collect cancel requests under lock, then release before async work
        let cancel_requests: Vec<(CancelRequest, String)> = {
            let orderid_map = self.strategy_orderid_map.read().unwrap_or_else(|e| e.into_inner());
            let mut requests = Vec::new();
            if let Some(orderids) = orderid_map.get(strategy_name) {
                for vt_orderid in orderids {
                    if let Some(order) = self.main_engine.get_order(vt_orderid) {
                        let req = CancelRequest::new(
                            order.orderid.clone(),
                            order.symbol.clone(),
                            order.exchange,
                        );
                        if let Some(gw_name) = self.main_engine.find_gateway_name_for_exchange(order.exchange) {
                            requests.push((req, gw_name));
                        }
                    }
                }
            }
            requests
        };

        for (req, gw_name) in cancel_requests {
            if let Err(e) = self.main_engine.cancel_order(req, &gw_name).await {
                tracing::warn!("Failed to cancel order: {}", e);
            }
        }
    }

    /// Load strategy settings from file
    fn load_strategy_settings(&self) {
        let path = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("trade_engine")
            .join("strategy_settings.json");

        if !path.exists() {
            tracing::info!("No strategy settings file found at {:?}", path);
            return;
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => {
                match serde_json::from_str::<HashMap<String, StrategySetting>>(&content) {
                    Ok(settings_map) => {
                        let mut settings = self.strategy_settings.write().unwrap_or_else(|e| e.into_inner());
                        for (name, setting) in settings_map {
                            settings.insert(name, setting);
                        }
                        tracing::info!("Loaded strategy settings from {:?}", path);
                    }
                    Err(e) => {
                        tracing::error!("Failed to parse strategy settings: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to read strategy settings file: {}", e);
            }
        }
    }

    /// Save strategy settings to file
    pub fn save_strategy_settings(&self) {
        let path = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("trade_engine")
            .join("strategy_settings.json");

        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::error!("Failed to create config directory: {}", e);
                return;
            }
        }

        let settings = self.strategy_settings.read().unwrap_or_else(|e| e.into_inner());
        match serde_json::to_string_pretty(&*settings) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&path, content) {
                    tracing::error!("Failed to write strategy settings: {}", e);
                } else {
                    tracing::info!("Saved strategy settings to {:?}", path);
                }
            }
            Err(e) => {
                tracing::error!("Failed to serialize strategy settings: {}", e);
            }
        }
    }

    /// Get all strategy names
    pub fn get_all_strategy_names(&self) -> Vec<String> {
        self.strategies.read().unwrap_or_else(|e| e.into_inner()).keys().cloned().collect()
    }

    /// Get strategy information
    pub fn get_strategy_info(&self, strategy_name: &str) -> Option<HashMap<String, String>> {
        let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
        
        if let Some(strategy) = strategies.get(strategy_name) {
            let mut info = HashMap::new();
            info.insert("name".to_string(), strategy.strategy_name().to_string());
            info.insert("type".to_string(), format!("{:?}", strategy.strategy_type()));
            info.insert("state".to_string(), format!("{:?}", strategy.state()));
            info.extend(strategy.parameters());
            info.extend(strategy.variables());
            Some(info)
        } else {
            None
        }
    }

    /// Set the per-strategy risk configuration.
    ///
    /// When set, orders from this strategy are validated against the configured
    /// limits before being forwarded to `MainEngine`. This complements the
    /// global `RiskEngine` with strategy-scoped guard rails.
    pub fn set_strategy_risk_config(
        &self,
        strategy_name: &str,
        config: StrategyRiskConfig,
    ) {
        self.strategy_risk_configs.write().unwrap_or_else(|e| e.into_inner())
            .insert(strategy_name.to_string(), config);
        tracing::info!("Set risk config for strategy {}", strategy_name);
    }

    /// Get the current risk configuration for a strategy.
    pub fn get_strategy_risk_config(
        &self,
        strategy_name: &str,
    ) -> StrategyRiskConfig {
        self.strategy_risk_configs.read().unwrap_or_else(|e| e.into_inner())
            .get(strategy_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Remove risk config for a strategy (called during cleanup).
    fn remove_strategy_risk_config(&self, strategy_name: &str) {
        self.strategy_risk_configs.write().unwrap_or_else(|e| e.into_inner()).remove(strategy_name);
    }

    /// Send an order on behalf of a strategy, routing through MainEngine (with risk check)
    /// Also populates orderid_strategy_map and strategy_orderid_map for callback routing
    ///
    /// For close orders, this method:
    /// 1. Checks available position (actual position minus frozen close volume from pending orders)
    /// 2. Reduces order volume if it exceeds available position
    /// 3. Freezes the close volume to prevent other strategies from double-closing
    ///
    /// Strategy-level risk checks (max order volume, max position, max notional, max active orders)
    /// are enforced BEFORE the close-order logic so that limits are never exceeded.
    pub async fn send_order(
        &self,
        strategy_name: &str,
        mut req: OrderRequest,
    ) -> Result<String, String> {
        // ── Strategy-level risk checks ──────────────────────────────────
        let risk_config = self.strategy_risk_configs.read().unwrap_or_else(|e| e.into_inner())
            .get(strategy_name)
            .cloned()
            .unwrap_or_default();

        // Check order volume
        if risk_config.check_order_volume && req.volume > risk_config.max_order_volume {
            return Err(format!(
                "策略风控: 订单数量 {} 超过限制 {} (策略: {})",
                req.volume, risk_config.max_order_volume, strategy_name
            ));
        }

        // Check order notional
        if risk_config.check_order_notional {
            let notional = req.price * req.volume;
            if notional > risk_config.max_order_notional {
                return Err(format!(
                    "策略风控: 订单金额 {} 超过限制 {} (策略: {})",
                    notional, risk_config.max_order_notional, strategy_name
                ));
            }
        }

        // Check active orders limit
        if risk_config.check_active_orders {
            let active_count = self.strategy_orderid_map.read().unwrap_or_else(|e| e.into_inner())
                .get(strategy_name)
                .map(|orders| orders.len())
                .unwrap_or(0);
            if active_count >= risk_config.max_active_orders {
                return Err(format!(
                    "策略风控: 活跃订单数 {} 超过限制 {} (策略: {})",
                    active_count, risk_config.max_active_orders, strategy_name
                ));
            }
        }

        // Check projected position for open orders
        if risk_config.check_position_volume && !Self::is_close_offset(req.offset) {
            let vt_symbol = req.vt_symbol();
            let current_pos = self.get_strategy_position(strategy_name, &vt_symbol);
            let projected = current_pos.abs() + req.volume;
            if projected > risk_config.max_position_volume {
                return Err(format!(
                    "策略风控: 预计持仓 {} 超过限制 {} (策略: {}, 合约: {})",
                    projected, risk_config.max_position_volume, strategy_name, vt_symbol
                ));
            }
        }

        // Check if this is a close order and enforce position isolation
        if Self::is_close_offset(req.offset) {
            let vt_symbol = req.vt_symbol();
            let pos = self.get_strategy_position(strategy_name, &vt_symbol);
            let frozen = self.get_frozen_close_volume(strategy_name, &vt_symbol);
            let available = (pos - frozen).max(0.0);

            if req.volume > available {
                if available > 0.0 {
                    tracing::warn!(
                        "Close order volume {} exceeds available {} for {} on {} (pos={}, frozen={}) - reducing to available",
                        req.volume, available, strategy_name, vt_symbol, pos, frozen
                    );
                    req.volume = available;
                } else {
                    return Err(format!(
                        "No available position to close for {} on {} (pos={}, frozen={})",
                        strategy_name, vt_symbol, pos, frozen
                    ));
                }
            }

            // Freeze the close volume
            self.freeze_close_volume(strategy_name, &vt_symbol, req.volume);
        }

        let exchange = req.exchange;
        let gw_name = self.main_engine.find_gateway_name_for_exchange(exchange)
            .ok_or_else(|| format!("No gateway found for exchange {:?}", exchange))?;

        let result = self.main_engine.send_order(req.clone(), &gw_name).await?;

        // Track order -> strategy mapping for callback routing
        {
            let mut orderid_map = self.orderid_strategy_map.write().unwrap_or_else(|e| e.into_inner());
            orderid_map.insert(result.clone(), strategy_name.to_string());
        }
        {
            let mut strategy_map = self.strategy_orderid_map.write().unwrap_or_else(|e| e.into_inner());
            strategy_map.entry(strategy_name.to_string())
                .or_default()
                .push(result.clone());
        }

        // Track close order info for later unfreezing
        if Self::is_close_offset(req.offset) {
            let vt_symbol = req.vt_symbol();
            let close_info = CloseOrderInfo {
                strategy_name: strategy_name.to_string(),
                vt_symbol: vt_symbol.clone(),
                volume: req.volume,
                remaining: req.volume,
            };
            self.order_close_info.write().unwrap_or_else(|e| e.into_inner()).insert(result.clone(), close_info);
        }

        Ok(result)
    }

    /// Process pending orders from a strategy (called after on_bar/on_tick callbacks)
    pub async fn process_pending_orders(&self, strategy_name: &str) -> Vec<Result<String, String>> {
        let pending: Vec<OrderRequest> = {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if let Some(strategy) = strategies.get_mut(strategy_name) {
                strategy.drain_pending_orders()
            } else {
                return Vec::new()
            }
        };

        let mut results = Vec::new();
        for req in pending {
            let result = self.send_order(strategy_name, req).await;
            results.push(result);
        }
        results
    }

    /// Process pending stop orders from a strategy (called after on_bar/on_tick callbacks)
    ///
    /// Registers stop orders with the engine. When the trigger price is reached,
    /// the stop order will be converted to a market/limit order.
    pub async fn process_pending_stop_orders(&self, strategy_name: &str) -> Vec<String> {
        let pending: Vec<StopOrderRequest> = {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if let Some(strategy) = strategies.get_mut(strategy_name) {
                strategy.drain_pending_stop_orders()
            } else {
                return Vec::new()
            }
        };

        let mut results = Vec::new();
        for req in pending {
            let stop_orderid = self.register_stop_order(strategy_name, req);
            results.push(stop_orderid);
        }
        results
    }

    /// Process pending cancellations from a strategy (called after on_bar/on_tick callbacks)
    ///
    /// Handles both regular order and stop order cancellation requests.
    pub async fn process_pending_cancellations(&self, strategy_name: &str) -> Vec<Result<(), String>> {
        let pending: Vec<CancelRequestType> = {
            let mut strategies = self.strategies.write().unwrap_or_else(|e| e.into_inner());
            if let Some(strategy) = strategies.get_mut(strategy_name) {
                strategy.drain_pending_cancellations()
            } else {
                return Vec::new()
            }
        };

        let mut results = Vec::new();
        for cancel_req in pending {
            let result = match cancel_req {
                CancelRequestType::Order(vt_orderid) => {
                    self.cancel_strategy_order(strategy_name, &vt_orderid).await
                }
                CancelRequestType::StopOrder(stop_orderid) => {
                    self.cancel_strategy_stop_order(&stop_orderid)
                }
            };
            results.push(result);
        }
        results
    }

    /// Register a stop order for a strategy
    ///
    /// Creates a StopOrder tracked by the engine. When the trigger price is reached
    /// (checked in process_tick_event), the stop order will be submitted as a
    /// market/limit order through the gateway.
    fn register_stop_order(&self, strategy_name: &str, req: StopOrderRequest) -> String {
        let stop_orderid = {
            let mut count = self.stop_order_count.lock()
                .unwrap_or_else(|e| e.into_inner());
            *count += 1;
            format!("{}.{}{}", strategy_name, super::base::STOPORDER_PREFIX, count)
        };

        let mut stop_order = StopOrder::new(
            stop_orderid.clone(),
            req.vt_symbol,
            req.direction,
            req.offset,
            req.price,
            req.volume,
            req.order_type,
            strategy_name.to_string(),
        );
        stop_order.lock = req.lock;

        self.stop_orders.write().unwrap_or_else(|e| e.into_inner()).insert(stop_orderid.clone(), stop_order);

        tracing::info!(
            "策略{}注册止损单: {} 价格={} 方向={:?}",
            strategy_name, stop_orderid, req.price, req.direction
        );

        stop_orderid
    }

    /// Cancel a stop order on behalf of a strategy
    fn cancel_strategy_stop_order(&self, stop_orderid: &str) -> Result<(), String> {
        let mut stop_orders = self.stop_orders.write().unwrap_or_else(|e| e.into_inner());
        let stop_order = stop_orders.get_mut(stop_orderid)
            .ok_or_else(|| format!("止损单{}不存在", stop_orderid))?;

        if stop_order.status != StopOrderStatus::Waiting {
            return Err(format!("止损单{}状态不是Waiting，无法取消", stop_orderid));
        }

        stop_order.status = StopOrderStatus::Cancelled;
        tracing::info!("止损单{}已取消", stop_orderid);
        Ok(())
    }

    /// Process all pending actions from a strategy (orders, stop orders, cancellations)
    ///
    /// Convenience method that calls process_pending_orders, process_pending_stop_orders,
    /// and process_pending_cancellations in sequence.
    pub async fn process_all_pending(&self, strategy_name: &str) {
        self.process_pending_orders(strategy_name).await;
        self.process_pending_stop_orders(strategy_name).await;
        self.process_pending_cancellations(strategy_name).await;
    }

    /// Cancel an order on behalf of a strategy
    pub async fn cancel_strategy_order(&self, _strategy_name: &str, vt_orderid: &str) -> Result<(), String> {
        let order = self.main_engine.get_order(vt_orderid)
            .ok_or_else(|| format!("Order {} not found", vt_orderid))?;

        let req = CancelRequest::new(
            order.orderid.clone(),
            order.symbol.clone(),
            order.exchange,
        );

        let gw_name = self.main_engine.find_gateway_name_for_exchange(order.exchange)
            .ok_or_else(|| format!("No gateway found for exchange {:?}", order.exchange))?;

        self.main_engine.cancel_order(req, &gw_name).await
    }

    /// Register a multi-period bar synthesizer for a strategy symbol.
    /// When 1-minute bars arrive, they will be accumulated into the target interval
    /// and delivered to the strategy via on_bar() when complete.
    /// 
    /// For example, if a strategy needs 5-minute bars for BTCUSDT.BINANCE,
    /// call `register_bar_synthesizer("MyStrategy", "BTCUSDT.BINANCE", Interval::Minute5)`.
    /// The engine will accumulate 5 consecutive 1-minute bars and deliver one 5-minute bar.
    pub fn register_bar_synthesizer(
        &self,
        strategy_name: &str,
        vt_symbol: &str,
        interval: Interval,
    ) {
        // Only create synthesizers for intervals other than Minute/Tick (base intervals)
        if interval == Interval::Minute || interval == Interval::Tick || interval == Interval::Second {
            return;
        }

        let key = (strategy_name.to_string(), vt_symbol.to_string());
        let synthesizer = BarSynthesizer::new(Interval::Minute, interval);
        
        self.bar_synthesizers.write().unwrap_or_else(|e| e.into_inner()).insert(key, synthesizer);
        tracing::info!(
            "Registered bar synthesizer for {} on {} with interval {:?}",
            strategy_name, vt_symbol, interval
        );
    }

    /// Unregister all bar synthesizers for a strategy
    fn unregister_bar_synthesizers(&self, strategy_name: &str) {
        let mut synthesizers = self.bar_synthesizers.write().unwrap_or_else(|e| e.into_inner());
        synthesizers.retain(|(name, _), _| name != strategy_name);
    }

    // ========================================================================
    // Per-strategy PnL tracking
    // ========================================================================

    /// Get realized PnL for a strategy
    pub fn get_strategy_pnl(&self, strategy_name: &str) -> f64 {
        self.strategy_pnl.read().unwrap_or_else(|e| e.into_inner()).get(strategy_name).copied().unwrap_or(0.0)
    }

    /// Get unrealized PnL for a strategy
    pub fn get_strategy_unrealized_pnl(&self, strategy_name: &str) -> f64 {
        self.strategy_unrealized_pnl.read().unwrap_or_else(|e| e.into_inner()).get(strategy_name).copied().unwrap_or(0.0)
    }

    /// Get total PnL (realized + unrealized) for a strategy
    pub fn get_strategy_total_pnl(&self, strategy_name: &str) -> f64 {
        let realized = self.strategy_pnl.read().unwrap_or_else(|e| e.into_inner()).get(strategy_name).copied().unwrap_or(0.0);
        let unrealized = self.strategy_unrealized_pnl.read().unwrap_or_else(|e| e.into_inner()).get(strategy_name).copied().unwrap_or(0.0);
        realized + unrealized
    }

    /// Get trade count for a strategy
    pub fn get_strategy_trade_count(&self, strategy_name: &str) -> usize {
        self.strategy_trade_count.read().unwrap_or_else(|e| e.into_inner()).get(strategy_name).copied().unwrap_or(0)
    }

    /// Update unrealized PnL for a strategy based on current market prices
    /// Call this when ticks arrive to keep unrealized PnL up to date
    pub fn update_unrealized_pnl(&self, strategy_name: &str, vt_symbol: &str) {
        let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
        let pos = strategies.get(strategy_name)
            .map(|s| s.get_position(vt_symbol))
            .unwrap_or(0.0);
        drop(strategies);

        if pos == 0.0 {
            self.strategy_unrealized_pnl.write().unwrap_or_else(|e| e.into_inner()).insert(strategy_name.to_string(), 0.0);
            return;
        }

        // Get last price from context
        let last_price = self.contexts.read().unwrap_or_else(|e| e.into_inner())
            .get(strategy_name)
            .and_then(|ctx| ctx.get_tick(vt_symbol))
            .map(|tick| tick.last_price);

        if let Some(last_price) = last_price {
            let avg_entry = self.strategy_avg_price.read().unwrap_or_else(|e| e.into_inner())
                .get(&(strategy_name.to_string(), vt_symbol.to_string()))
                .copied()
                .unwrap_or(last_price);

            let unrealized = if pos > 0.0 {
                (last_price - avg_entry) * pos
            } else {
                (avg_entry - last_price) * pos.abs()
            };

            self.strategy_unrealized_pnl.write().unwrap_or_else(|e| e.into_inner()).insert(strategy_name.to_string(), unrealized);
        }
    }

    // ========================================================================
    // Strategy-level frozen volume management (position isolation)
    // ========================================================================

    /// Check if an offset is a close offset
    fn is_close_offset(offset: Offset) -> bool {
        matches!(offset, Offset::Close | Offset::CloseToday | Offset::CloseYesterday)
    }

    /// Freeze volume for a pending close order
    fn freeze_close_volume(&self, strategy_name: &str, vt_symbol: &str, volume: f64) {
        let mut frozen = self.strategy_frozen_closes.write().unwrap_or_else(|e| e.into_inner());
        let strategy_frozen = frozen.entry(strategy_name.to_string()).or_default();
        let current = strategy_frozen.get(vt_symbol).copied().unwrap_or(0.0);
        strategy_frozen.insert(vt_symbol.to_string(), current + volume);
        tracing::debug!(
            "Frozen {} close volume for {} on {} (total frozen: {})",
            volume, strategy_name, vt_symbol, current + volume
        );
    }

    /// Get frozen close volume for a strategy-symbol
    fn get_frozen_close_volume(&self, strategy_name: &str, vt_symbol: &str) -> f64 {
        self.strategy_frozen_closes.read().unwrap_or_else(|e| e.into_inner())
            .get(strategy_name)
            .and_then(|s| s.get(vt_symbol).copied())
            .unwrap_or(0.0)
    }

    /// Get available position for a strategy (actual position minus frozen close volume)
    pub fn get_available_position(&self, strategy_name: &str, vt_symbol: &str) -> f64 {
        let pos = self.get_strategy_position(strategy_name, vt_symbol);
        let frozen = self.get_frozen_close_volume(strategy_name, vt_symbol);
        (pos - frozen).max(0.0)
    }

    /// Get current position for a strategy-symbol
    pub fn get_strategy_position(&self, strategy_name: &str, vt_symbol: &str) -> f64 {
        let strategies = self.strategies.read().unwrap_or_else(|e| e.into_inner());
        strategies.get(strategy_name)
            .map(|s| s.get_position(vt_symbol))
            .unwrap_or(0.0)
    }
}

/// Implement BaseEngine for StrategyEngine so it can receive events directly from MainEngine
impl BaseEngine for StrategyEngine {
    fn engine_name(&self) -> &str {
        "strategy"
    }

    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        self.process_event_internal(event_type, event);
    }
}

// Implement Clone for StrategyEngine (needed for event handlers)
impl Clone for StrategyEngine {
    fn clone(&self) -> Self {
        Self {
            main_engine: self.main_engine.clone(),
            event_engine: self.event_engine.clone(),
            strategies: self.strategies.clone(),
            strategy_settings: self.strategy_settings.clone(),
            symbol_strategy_map: self.symbol_strategy_map.clone(),
            orderid_strategy_map: self.orderid_strategy_map.clone(),
            strategy_orderid_map: self.strategy_orderid_map.clone(),
            stop_orders: self.stop_orders.clone(),
            stop_order_count: self.stop_order_count.clone(),
            contexts: self.contexts.clone(),
            processed_tradeids: self.processed_tradeids.clone(),
            processed_tradeids_order: self.processed_tradeids_order.clone(),
            bar_synthesizers: self.bar_synthesizers.clone(),
            strategy_pnl: self.strategy_pnl.clone(),
            strategy_unrealized_pnl: self.strategy_unrealized_pnl.clone(),
            strategy_trade_count: self.strategy_trade_count.clone(),
            strategy_avg_price: self.strategy_avg_price.clone(),
            strategy_frozen_closes: self.strategy_frozen_closes.clone(),
            order_close_info: self.order_close_info.clone(),
            strategy_risk_configs: self.strategy_risk_configs.clone(),
            database: self.database.clone(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::trader::MainEngine;
    use crate::event::EventEngine;
    use crate::{StrategyType, BarData};

    fn create_strategy_engine() -> StrategyEngine {
        let main_engine = Arc::new(MainEngine::new());
        let event_engine = Arc::new(EventEngine::new(1));
        StrategyEngine::new(main_engine, event_engine)
    }

    #[test]
    fn test_strategy_engine_new() {
        let engine = create_strategy_engine();
        let names = engine.get_all_strategy_names();
        assert!(names.is_empty());
    }

    #[test]
    fn test_get_all_strategy_names_empty() {
        let engine = create_strategy_engine();
        let names = engine.get_all_strategy_names();
        assert!(names.is_empty());
    }

    #[test]
    fn test_get_strategy_info_not_found() {
        let engine = create_strategy_engine();
        let info = engine.get_strategy_info("nonexistent");
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_init_strategy_not_found() {
        let engine = create_strategy_engine();
        let result = engine.init_strategy("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_start_strategy_not_found() {
        let engine = create_strategy_engine();
        let result = engine.start_strategy("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_stop_strategy_not_found() {
        let engine = create_strategy_engine();
        let result = engine.stop_strategy("nonexistent").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn test_add_strategy_and_get_names() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        let names = engine.get_all_strategy_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"TestStrategy".to_string()));
    }

    #[tokio::test]
    async fn test_add_duplicate_strategy() {
        let engine = create_strategy_engine();

        let strategy1 = MockStrategy::new("TestStrategy".to_string());
        let setting1 = StrategySetting::new();
        engine.add_strategy(Box::new(strategy1), setting1).await.unwrap();

        let strategy2 = MockStrategy::new("TestStrategy".to_string());
        let setting2 = StrategySetting::new();
        let result = engine.add_strategy(Box::new(strategy2), setting2).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[tokio::test]
    async fn test_get_strategy_info_found() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        let info = engine.get_strategy_info("TestStrategy");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.get("name").unwrap(), "TestStrategy");
    }

    #[tokio::test]
    async fn test_start_strategy_not_inited() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        let result = engine.start_strategy("TestStrategy");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not initialized"));
    }

    #[tokio::test]
    async fn test_stop_strategy_not_trading() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        let result = engine.stop_strategy("TestStrategy").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not trading"));
    }

    #[test]
    fn test_strategy_engine_clone() {
        let engine = create_strategy_engine();
        let _cloned = engine.clone();
    }

    struct MockStrategy {
        name: String,
        vt_symbols: Vec<String>,
        state: StrategyState,
        positions: std::collections::HashMap<String, f64>,
    }

    impl MockStrategy {
        fn new(name: String) -> Self {
            Self {
                name,
                vt_symbols: vec!["BTCUSDT.BINANCE".to_string()],
                state: StrategyState::NotInited,
                positions: std::collections::HashMap::new(),
            }
        }
    }

    impl StrategyTemplate for MockStrategy {
        fn strategy_name(&self) -> &str { &self.name }
        fn vt_symbols(&self) -> &[String] { &self.vt_symbols }
        fn strategy_type(&self) -> StrategyType { StrategyType::Spot }
        fn state(&self) -> StrategyState { self.state }
        fn parameters(&self) -> std::collections::HashMap<String, String> {
            std::collections::HashMap::new()
        }
        fn variables(&self) -> std::collections::HashMap<String, String> {
            std::collections::HashMap::new()
        }
        fn on_init(&mut self, _context: &StrategyContext) {
            self.state = StrategyState::Inited;
        }
        fn on_start(&mut self) {
            self.state = StrategyState::Trading;
        }
        fn on_stop(&mut self) {
            self.state = StrategyState::Stopped;
        }
        fn on_tick(&mut self, _tick: &TickData, _context: &StrategyContext) {}
        fn on_bar(&mut self, _bar: &BarData, _context: &StrategyContext) {}
        fn on_order(&mut self, _order: &OrderData) {}
        fn on_trade(&mut self, _trade: &TradeData) {}
        fn on_stop_order(&mut self, _stop_orderid: &str) {}
        fn update_position(&mut self, vt_symbol: &str, position: f64) {
            self.positions.insert(vt_symbol.to_string(), position);
        }
        fn get_position(&self, vt_symbol: &str) -> f64 {
            *self.positions.get(vt_symbol).unwrap_or(&0.0)
        }
    }

    #[tokio::test]
    async fn test_remove_strategy() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        assert!(engine.get_all_strategy_names().contains(&"TestStrategy".to_string()));

        // Remove the strategy
        let result = engine.remove_strategy("TestStrategy").await;
        assert!(result.is_ok());
        assert!(!engine.get_all_strategy_names().contains(&"TestStrategy".to_string()));

        // Remove nonexistent should fail
        let result = engine.remove_strategy("NonExistent").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_pnl_tracking_initial_state() {
        let engine = create_strategy_engine();
        
        // PnL should be 0 for nonexistent strategy
        let pnl = engine.get_strategy_pnl("NonExistent");
        assert_eq!(pnl, 0.0);
        
        let unrealized = engine.get_strategy_unrealized_pnl("NonExistent");
        assert_eq!(unrealized, 0.0);
        
        let total = engine.get_strategy_total_pnl("NonExistent");
        assert_eq!(total, 0.0);
        
        let count = engine.get_strategy_trade_count("NonExistent");
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_register_bar_synthesizer() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        
        // Register a 5-minute synthesizer
        engine.register_bar_synthesizer("TestStrategy", "BTCUSDT.BINANCE", Interval::Minute5);
        
        // Should not create synthesizer for base intervals
        engine.register_bar_synthesizer("TestStrategy", "ETHUSDT.BINANCE", Interval::Minute);
    }

    #[tokio::test]
    async fn test_get_available_position_no_frozen() {
        let engine = create_strategy_engine();
        let strategy = MockStrategy::new("TestStrategy".to_string());
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();

        // No position and no frozen volume
        let available = engine.get_available_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(available, 0.0);
    }

    #[tokio::test]
    async fn test_get_strategy_position() {
        let engine = create_strategy_engine();
        let mut strategy = MockStrategy::new("TestStrategy".to_string());
        strategy.positions.insert("BTCUSDT.BINANCE".to_string(), 1.5);
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();

        let pos = engine.get_strategy_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(pos, 1.5);

        // Nonexistent symbol returns 0
        let pos = engine.get_strategy_position("TestStrategy", "ETHUSDT.BINANCE");
        assert_eq!(pos, 0.0);

        // Nonexistent strategy returns 0
        let pos = engine.get_strategy_position("NonExistent", "BTCUSDT.BINANCE");
        assert_eq!(pos, 0.0);
    }

    #[tokio::test]
    async fn test_freeze_close_volume() {
        let engine = create_strategy_engine();
        let mut strategy = MockStrategy::new("TestStrategy".to_string());
        strategy.positions.insert("BTCUSDT.BINANCE".to_string(), 2.0);
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();

        // Initially available = position = 2.0
        let available = engine.get_available_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(available, 2.0);

        // Freeze 1.0
        engine.freeze_close_volume("TestStrategy", "BTCUSDT.BINANCE", 1.0);

        // Available should be 2.0 - 1.0 = 1.0
        let available = engine.get_available_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(available, 1.0);

        // Freeze another 0.5
        engine.freeze_close_volume("TestStrategy", "BTCUSDT.BINANCE", 0.5);

        // Available should be 2.0 - 1.5 = 0.5
        let available = engine.get_available_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(available, 0.5);
    }

    #[tokio::test]
    async fn test_frozen_volume_cleaned_on_remove_strategy() {
        let engine = create_strategy_engine();
        let mut strategy = MockStrategy::new("TestStrategy".to_string());
        strategy.positions.insert("BTCUSDT.BINANCE".to_string(), 2.0);
        let setting = StrategySetting::new();

        engine.add_strategy(Box::new(strategy), setting).await.unwrap();
        engine.freeze_close_volume("TestStrategy", "BTCUSDT.BINANCE", 1.0);

        // Verify frozen
        let available = engine.get_available_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(available, 1.0);

        // Remove strategy
        engine.remove_strategy("TestStrategy").await.unwrap();

        // Frozen data should be cleaned up (no panic when accessing nonexistent strategy)
        let available = engine.get_available_position("TestStrategy", "BTCUSDT.BINANCE");
        assert_eq!(available, 0.0);
    }
}
