//! Strategy Engine
//! 
//! Core engine for managing and executing trading strategies

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use chrono::{Utc, Duration};

use crate::trader::{
    MainEngine, TickData, OrderData, TradeData,
    SubscribeRequest, CancelRequest, HistoryRequest,
    Direction, Interval, Exchange,
    EVENT_TICK, EVENT_ORDER, EVENT_TRADE,
};
use crate::event::EventEngine;
use super::template::{StrategyTemplate, StrategyContext};
use super::base::{
    StrategyState, StopOrder, StopOrderStatus, 
    StrategySetting
};

// Event type constants for strategy
pub const EVENT_STRATEGY_TICK: &str = "eStrategyTick";
pub const EVENT_STRATEGY_BAR: &str = "eStrategyBar";
pub const EVENT_STRATEGY_ORDER: &str = "eStrategyOrder";
pub const EVENT_STRATEGY_TRADE: &str = "eStrategyTrade";

/// Strategy engine managing all strategies
pub struct StrategyEngine {
    /// Main trading engine
    main_engine: Arc<MainEngine>,
    /// Event engine
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
}

impl StrategyEngine {
    pub fn new(main_engine: Arc<MainEngine>, event_engine: Arc<EventEngine>) -> Self {
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
        }
    }

    /// Initialize the engine
    pub async fn init(&self) {
        // Register event handlers
        self.register_event_handlers();
        
        // Load strategy settings
        self.load_strategy_settings().await;
        
        tracing::info!("Strategy engine initialized successfully");
    }

    /// Register event handlers
    fn register_event_handlers(&self) {
        // Clone Arc for use in closures
        let strategies = Arc::clone(&self.strategies);
        let contexts = Arc::clone(&self.contexts);
        let symbol_strategy_map = Arc::clone(&self.symbol_strategy_map);
        let orderid_strategy_map = Arc::clone(&self.orderid_strategy_map);
        let processed_tradeids = Arc::clone(&self.processed_tradeids);

        // Register tick event - listen to all ticks
        let tick_strategies = Arc::clone(&strategies);
        let tick_contexts = Arc::clone(&contexts);
        let tick_symbol_map = Arc::clone(&symbol_strategy_map);
        let tick_stop_orders = Arc::clone(&self.stop_orders);
        
        self.event_engine.register(EVENT_TICK, Arc::new(move |event| {
            tracing::debug!("Tick event received");
            let _ = (&tick_strategies, &tick_contexts, &tick_symbol_map, &tick_stop_orders);
            if let Some(ref data) = event.data {
                if let Ok(tick) = data.clone().downcast::<TickData>() {
                    let strategies = tick_strategies.clone();
                    let contexts = tick_contexts.clone();
                    let symbol_map = tick_symbol_map.clone();
                    let stop_orders = tick_stop_orders.clone();
                    
                    let vt_symbol = tick.vt_symbol();
                    let strategy_names: Vec<String> = symbol_map.blocking_read()
                        .get(&vt_symbol).cloned()
                        .unwrap_or_default();
                    
                    for strategy_name in &strategy_names {
                        let contexts_guard = contexts.blocking_read();
                        if let Some(context) = contexts_guard.get(strategy_name) {
                            context.update_tick((*tick).clone());
                        }
                        let strategies_guard = strategies.blocking_write();
                        if let Some(_strategy) = strategies_guard.get(strategy_name) {
                            // strategy.on_tick(&tick, context);
                        }
                    }
                    
                    {
                        let mut stop_orders_guard = stop_orders.blocking_write();
                        let mut triggered = Vec::new();
                        for (stop_orderid, stop_order) in stop_orders_guard.iter() {
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
                            if let Some(stop_order) = stop_orders_guard.get_mut(&stop_orderid) {
                                stop_order.status = StopOrderStatus::Triggered;
                            }
                        }
                    }
                }
            }
        }));

        // Register order event
        let order_strategies = Arc::clone(&strategies);
        let order_orderid_map = Arc::clone(&orderid_strategy_map);
        
        self.event_engine.register(EVENT_ORDER, Arc::new(move |event| {
            tracing::debug!("Order event received");
            if let Some(ref data) = event.data {
                if let Ok(order) = data.clone().downcast::<OrderData>() {
                    let orderid_map = order_orderid_map.clone();
                    let strategies = order_strategies.clone();
                    
                    let strategy_name = orderid_map.blocking_read()
                        .get(&order.vt_orderid())
                        .cloned();
                    
                    if let Some(strategy_name) = strategy_name {
                        let mut strategies_guard = strategies.blocking_write();
                        if let Some(strategy) = strategies_guard.get_mut(&strategy_name) {
                            strategy.on_order(&order);
                        }
                    }
                }
            }
        }));

        // Register trade event
        let trade_strategies = Arc::clone(&strategies);
        let trade_orderid_map = Arc::clone(&orderid_strategy_map);
        let trade_processed = Arc::clone(&processed_tradeids);
        
        self.event_engine.register(EVENT_TRADE, Arc::new(move |event| {
            tracing::debug!("Trade event received");
            if let Some(ref data) = event.data {
                if let Ok(trade) = data.clone().downcast::<TradeData>() {
                    let processed = trade_processed.clone();
                    let orderid_map = trade_orderid_map.clone();
                    let strategies = trade_strategies.clone();
                    
                    let vt_tradeid = trade.vt_tradeid();
                    {
                        let mut processed_guard = processed.blocking_write();
                        if processed_guard.contains(&vt_tradeid) {
                            return;
                        }
                        processed_guard.insert(vt_tradeid);
                        if processed_guard.len() > 10000 {
                            processed_guard.clear();
                        }
                    }
                    
                    let strategy_name = orderid_map.blocking_read()
                        .get(&trade.vt_orderid())
                        .cloned();
                    
                    if let Some(strategy_name) = strategy_name {
                        let mut strategies_guard = strategies.blocking_write();
                        if let Some(strategy) = strategies_guard.get_mut(&strategy_name) {
                            let volume_change = match trade.direction {
                                Some(Direction::Long) => trade.volume,
                                Some(Direction::Short) => -trade.volume,
                                Some(Direction::Net) => 0.0,
                                None => 0.0,
                            };
                            let current_pos = strategy.get_position(&trade.vt_symbol());
                            strategy.update_position(&trade.vt_symbol(), current_pos + volume_change);
                            strategy.on_trade(&trade);
                        }
                    }
                }
            }
        }));
    }

    /// Add a strategy
    pub async fn add_strategy(
        &self,
        strategy: Box<dyn StrategyTemplate>,
        setting: StrategySetting,
    ) -> Result<(), String> {
        let strategy_name = strategy.strategy_name().to_string();
        
        // Check if strategy already exists
        if self.strategies.read().await.contains_key(&strategy_name) {
            return Err(format!("Strategy {} already exists", strategy_name));
        }

        // Create context for this strategy
        let context = StrategyContext::new();
        
        // Subscribe to symbols
        for vt_symbol in strategy.vt_symbols() {
            self.subscribe_symbol(&strategy_name, vt_symbol).await;
        }

        // Store strategy
        self.strategies.write().await.insert(strategy_name.clone(), strategy);
        self.strategy_settings.write().await.insert(strategy_name.clone(), setting);
        self.contexts.write().await.insert(strategy_name.clone(), context);

        tracing::info!("Strategy {} added successfully", strategy_name);
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
        let mut map = self.symbol_strategy_map.write().await;
        map.entry(vt_symbol.to_string())
            .or_insert_with(Vec::new)
            .push(strategy_name.to_string());
    }

    /// Initialize a strategy
    pub async fn init_strategy(&self, strategy_name: &str) -> Result<(), String> {
        let mut strategies = self.strategies.write().await;
        let contexts = self.contexts.read().await;

        if let Some(strategy) = strategies.get_mut(strategy_name) {
            if let Some(context) = contexts.get(strategy_name) {
                // Load historical data
                self.load_historical_data(strategy_name, context).await?;

                // Initialize strategy
                strategy.on_init(context);

                tracing::info!("Strategy {} initialized", strategy_name);
                Ok(())
            } else {
                Err(format!("Context not found for strategy {}", strategy_name))
            }
        } else {
            Err(format!("Strategy {} not found", strategy_name))
        }
    }

    /// Start a strategy
    pub async fn start_strategy(&self, strategy_name: &str) -> Result<(), String> {
        let mut strategies = self.strategies.write().await;

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
            let mut strategies = self.strategies.write().await;

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
    async fn load_historical_data(
        &self,
        strategy_name: &str,
        _context: &StrategyContext,
    ) -> Result<(), String> {
        let strategies = self.strategies.read().await;
        
        if let Some(strategy) = strategies.get(strategy_name) {
            for vt_symbol in strategy.vt_symbols() {
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
                            tracing::info!("Loaded {} historical bars for {}", bars.len(), vt_symbol);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load history for {}: {}", vt_symbol, e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Cancel all orders for a strategy
    async fn cancel_all_orders(&self, strategy_name: &str) {
        let orderid_map = self.strategy_orderid_map.read().await;
        
        if let Some(orderids) = orderid_map.get(strategy_name) {
            for vt_orderid in orderids {
                if let Some(order) = self.main_engine.get_order(vt_orderid) {
                    let req = CancelRequest::new(
                        order.orderid.clone(),
                        order.symbol.clone(),
                        order.exchange,
                    );
                    if let Some(gw_name) = self.main_engine.find_gateway_name_for_exchange(order.exchange) {
                        if let Err(e) = self.main_engine.cancel_order(req, &gw_name).await {
                            tracing::warn!("Failed to cancel order {}: {}", vt_orderid, e);
                        }
                    }
                }
            }
        }
    }

    /// Load strategy settings from file
    async fn load_strategy_settings(&self) {
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
                        let mut settings = self.strategy_settings.write().await;
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
    pub async fn save_strategy_settings(&self) {
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

        let settings = self.strategy_settings.read().await;
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
    pub async fn get_all_strategy_names(&self) -> Vec<String> {
        self.strategies.read().await.keys().cloned().collect()
    }

    /// Get strategy information
    pub async fn get_strategy_info(&self, strategy_name: &str) -> Option<HashMap<String, String>> {
        let strategies = self.strategies.read().await;
        
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
        }
    }
}
