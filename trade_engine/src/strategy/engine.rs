//! Strategy Engine
//! 
//! Core engine for managing and executing trading strategies

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use chrono::{Utc, Duration};

use crate::trader::{
    MainEngine, TickData, OrderData, TradeData,
    SubscribeRequest, OrderRequest, HistoryRequest,
    Direction, Offset, Interval, Exchange,
    EVENT_TICK, EVENT_ORDER, EVENT_TRADE,
};
use crate::event::{EventEngine, EVENT_TIMER};
use super::template::{StrategyTemplate, StrategyContext};
use super::base::{
    StrategyState, StopOrder, StopOrderStatus, 
    StrategySetting, STOPORDER_PREFIX
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

        // Register tick event - listen to all ticks
        let tick_strategies = Arc::clone(&strategies);
        let tick_contexts = Arc::clone(&contexts);
        let tick_symbol_map = Arc::clone(&symbol_strategy_map);
        
        self.event_engine.register(EVENT_TICK, Arc::new(move |event| {
            // Extract tick from event data
            // Note: This is a simplified version - in production you'd need proper type extraction
            tracing::debug!("Tick event received");
        }));

        // Register order event
        let order_strategies = Arc::clone(&strategies);
        
        self.event_engine.register(EVENT_ORDER, Arc::new(move |event| {
            tracing::debug!("Order event received");
        }));

        // Register trade event
        let trade_strategies = Arc::clone(&strategies);
        
        self.event_engine.register(EVENT_TRADE, Arc::new(move |event| {
            tracing::debug!("Trade event received");
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
        // self.main_engine.subscribe(req, gateway_name).await;

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
        let mut strategies = self.strategies.write().await;

        if let Some(strategy) = strategies.get_mut(strategy_name) {
            if strategy.state() != StrategyState::Trading {
                return Err(format!(
                    "Strategy {} not trading, current state: {:?}",
                    strategy_name, strategy.state()
                ));
            }

            strategy.on_stop();
            
            // Cancel all active orders
            self.cancel_all_orders(strategy_name).await;

            tracing::info!("Strategy {} stopped", strategy_name);
            Ok(())
        } else {
            Err(format!("Strategy {} not found", strategy_name))
        }
    }

    /// Load historical data for strategy initialization
    async fn load_historical_data(
        &self,
        strategy_name: &str,
        context: &StrategyContext,
    ) -> Result<(), String> {
        let strategies = self.strategies.read().await;
        
        if let Some(strategy) = strategies.get(strategy_name) {
            for vt_symbol in strategy.vt_symbols() {
                // Parse symbol
                let parts: Vec<&str> = vt_symbol.split('.').collect();
                if parts.len() != 2 {
                    continue;
                }

                let symbol = parts[0].to_string();
                let exchange = Exchange::Binance; // Simplified

                // Query historical data (last 30 days)
                let end = Utc::now();
                let start = end - Duration::days(30);

                let req = HistoryRequest {
                    symbol,
                    exchange,
                    start,
                    end: Some(end),
                    interval: Some(Interval::Minute),
                };

                // TODO: Query from main engine
                // let bars = self.main_engine.query_history(req, gateway_name).await;
                
                // Update context with historical bars
                // for bar in bars {
                //     context.update_bar(bar);
                // }
            }
        }

        Ok(())
    }

    /// Process tick event
    async fn process_tick_event(&self, tick: TickData) {
        let vt_symbol = tick.vt_symbol();
        
        // Update context
        if let Some(strategy_names) = self.symbol_strategy_map.read().await.get(&vt_symbol) {
            let strategies = self.strategies.write().await;
            let contexts = self.contexts.read().await;

            for strategy_name in strategy_names {
                if let Some(context) = contexts.get(strategy_name) {
                    // Update tick in context
                    context.update_tick(tick.clone());

                    // Call strategy callback
                    if let Some(strategy) = strategies.get(strategy_name) {
                        // strategy.on_tick(&tick, context);
                    }
                }
            }
        }

        // Check stop orders
        self.check_stop_orders(&tick).await;
    }

    /// Process order event
    async fn process_order_event(&self, order: OrderData) {
        let orderid_map = self.orderid_strategy_map.read().await;
        
        if let Some(strategy_name) = orderid_map.get(&order.vt_orderid()) {
            let mut strategies = self.strategies.write().await;
            
            if let Some(strategy) = strategies.get_mut(strategy_name) {
                strategy.on_order(&order);
            }
        }
    }

    /// Process trade event
    async fn process_trade_event(&self, trade: TradeData) {
        // Check for duplicate trades
        let vt_tradeid = trade.vt_tradeid();
        {
            let mut processed = self.processed_tradeids.write().await;
            if processed.contains(&vt_tradeid) {
                return;
            }
            processed.insert(vt_tradeid);
        }

        let orderid_map = self.orderid_strategy_map.read().await;
        
        if let Some(strategy_name) = orderid_map.get(&trade.vt_orderid()) {
            let mut strategies = self.strategies.write().await;
            
            if let Some(strategy) = strategies.get_mut(strategy_name) {
                // Update position
                let volume_change = match trade.direction {
                    Some(Direction::Long) => trade.volume,
                    Some(Direction::Short) => -trade.volume,
                    Some(Direction::Net) => 0.0, // Net position doesn't affect total position
                    None => 0.0,
                };
                
                let current_pos = strategy.get_position(&trade.vt_symbol());
                strategy.update_position(&trade.vt_symbol(), current_pos + volume_change);

                // Call strategy callback
                strategy.on_trade(&trade);
            }
        }
    }

    /// Check and trigger stop orders
    async fn check_stop_orders(&self, tick: &TickData) {
        let mut triggered_orders = Vec::new();
        
        {
            let stop_orders = self.stop_orders.read().await;
            
            for (stop_orderid, stop_order) in stop_orders.iter() {
                if stop_order.vt_symbol != tick.vt_symbol() {
                    continue;
                }

                if stop_order.status != StopOrderStatus::Waiting {
                    continue;
                }

                // Check if should trigger
                let should_trigger = match stop_order.direction {
                    Direction::Long => tick.last_price >= stop_order.price,
                    Direction::Short => tick.last_price <= stop_order.price,
                    Direction::Net => false, // Net direction doesn't trigger
                };

                if should_trigger {
                    triggered_orders.push(stop_orderid.clone());
                }
            }
        }

        // Trigger orders
        for stop_orderid in triggered_orders {
            self.trigger_stop_order(&stop_orderid).await;
        }
    }

    /// Trigger a stop order
    async fn trigger_stop_order(&self, stop_orderid: &str) {
        let mut stop_orders = self.stop_orders.write().await;
        
        if let Some(stop_order) = stop_orders.get_mut(stop_orderid) {
            // Create order request
            let req = OrderRequest {
                symbol: stop_order.vt_symbol.split('.').next().unwrap().to_string(),
                exchange: Exchange::Binance, // Simplified
                direction: stop_order.direction,
                order_type: stop_order.order_type,
                volume: stop_order.volume,
                price: stop_order.price,
                offset: stop_order.offset.unwrap_or(Offset::Open), // Default to Open for spot
                reference: format!("STOP_{}", stop_order.strategy_name),
            };

            // Send order through main engine
            // let vt_orderid = self.main_engine.send_order(req, gateway_name).await;

            // Update stop order
            // stop_order.vt_orderid = Some(vt_orderid.clone());
            stop_order.status = StopOrderStatus::Triggered;

            // Notify strategy
            let strategies = self.strategies.read().await;
            if let Some(strategy) = strategies.get(&stop_order.strategy_name) {
                // strategy.on_stop_order(stop_orderid);
            }
        }
    }

    /// Cancel all orders for a strategy
    async fn cancel_all_orders(&self, strategy_name: &str) {
        let orderid_map = self.strategy_orderid_map.read().await;
        
        if let Some(orderids) = orderid_map.get(strategy_name) {
            for vt_orderid in orderids {
                // Send cancel request
                // self.main_engine.cancel_order(req, gateway_name).await;
            }
        }
    }

    /// Load strategy settings from file
    async fn load_strategy_settings(&self) {
        // TODO: Load from JSON file
        tracing::info!("Loading strategy settings...");
    }

    /// Save strategy settings to file
    pub async fn save_strategy_settings(&self) {
        // TODO: Save to JSON file
        tracing::info!("Saving strategy settings...");
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
