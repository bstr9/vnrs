//! Engine module for the trading platform core functionality.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::app::BaseApp;
use super::constant::Exchange;

use super::event::*;
use super::gateway::{BaseGateway, GatewayEvent, GatewaySettings};
use super::object::{
    AccountData, BarData, CancelRequest, ContractData, HistoryRequest, LogData,
    OrderData, OrderRequest, PositionData, QuoteData, QuoteRequest, SubscribeRequest,
    TickData, TradeData,
};
use super::setting::SETTINGS;


/// Event handler type
pub type EventHandler = Box<dyn Fn(&GatewayEvent) + Send + Sync>;

/// Base engine trait for implementing function engines
pub trait BaseEngine: Send + Sync {
    /// Get the engine name
    fn engine_name(&self) -> &str;

    /// Close the engine
    fn close(&self) {}
}

/// OMS (Order Management System) Engine data container
pub struct OmsData {
    pub ticks: HashMap<String, TickData>,
    pub orders: HashMap<String, OrderData>,
    pub trades: HashMap<String, TradeData>,
    pub positions: HashMap<String, PositionData>,
    pub accounts: HashMap<String, AccountData>,
    pub contracts: HashMap<String, ContractData>,
    pub quotes: HashMap<String, QuoteData>,
    pub active_orders: HashMap<String, OrderData>,
    pub active_quotes: HashMap<String, QuoteData>,
    pub logs: Vec<LogData>,
}

impl OmsData {
    pub fn new() -> Self {
        Self {
            ticks: HashMap::new(),
            orders: HashMap::new(),
            trades: HashMap::new(),
            positions: HashMap::new(),
            accounts: HashMap::new(),
            contracts: HashMap::new(),
            quotes: HashMap::new(),
            active_orders: HashMap::new(),
            active_quotes: HashMap::new(),
            logs: Vec::new(),
        }
    }
}

impl Default for OmsData {
    fn default() -> Self {
        Self::new()
    }
}

/// OMS Engine for order management
pub struct OmsEngine {
    data: RwLock<OmsData>,
}

impl OmsEngine {
    /// Create a new OmsEngine
    pub fn new() -> Self {
        Self {
            data: RwLock::new(OmsData::new()),
        }
    }

    /// Process tick event
    pub fn process_tick(&self, tick: TickData) {
        if let Ok(mut data) = self.data.write() {
            data.ticks.insert(tick.vt_symbol(), tick);
        }
    }

    /// Process order event
    pub fn process_order(&self, order: OrderData) {
        if let Ok(mut data) = self.data.write() {
            let vt_orderid = order.vt_orderid();
            
            // If order is active, update data in dict
            if order.is_active() {
                data.active_orders.insert(vt_orderid.clone(), order.clone());
            } else {
                // Otherwise, pop inactive order from dict
                data.active_orders.remove(&vt_orderid);
            }
            
            data.orders.insert(vt_orderid, order);
        }
    }

    /// Process trade event
    pub fn process_trade(&self, trade: TradeData) {
        if let Ok(mut data) = self.data.write() {
            data.trades.insert(trade.vt_tradeid(), trade);
        }
    }

    /// Process position event
    pub fn process_position(&self, position: PositionData) {
        if let Ok(mut data) = self.data.write() {
            data.positions.insert(position.vt_positionid(), position);
        }
    }

    /// Process account event
    pub fn process_account(&self, account: AccountData) {
        if let Ok(mut data) = self.data.write() {
            data.accounts.insert(account.vt_accountid(), account);
        }
    }

    /// Process contract event
    pub fn process_contract(&self, contract: ContractData) {
        if let Ok(mut data) = self.data.write() {
            data.contracts.insert(contract.vt_symbol(), contract);
        }
    }

    /// Process quote event
    pub fn process_quote(&self, quote: QuoteData) {
        if let Ok(mut data) = self.data.write() {
            let vt_quoteid = quote.vt_quoteid();
            
            // If quote is active, update data in dict
            if quote.is_active() {
                data.active_quotes.insert(vt_quoteid.clone(), quote.clone());
            } else {
                // Otherwise, pop inactive quote from dict
                data.active_quotes.remove(&vt_quoteid);
            }
            
            data.quotes.insert(vt_quoteid, quote);
        }
    }

    /// Get latest tick data by vt_symbol
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        self.data.read().ok()?.ticks.get(vt_symbol).cloned()
    }

    /// Get latest order data by vt_orderid
    pub fn get_order(&self, vt_orderid: &str) -> Option<OrderData> {
        self.data.read().ok()?.orders.get(vt_orderid).cloned()
    }

    /// Get trade data by vt_tradeid
    pub fn get_trade(&self, vt_tradeid: &str) -> Option<TradeData> {
        self.data.read().ok()?.trades.get(vt_tradeid).cloned()
    }

    /// Get latest position data by vt_positionid
    pub fn get_position(&self, vt_positionid: &str) -> Option<PositionData> {
        self.data.read().ok()?.positions.get(vt_positionid).cloned()
    }

    /// Get latest account data by vt_accountid
    pub fn get_account(&self, vt_accountid: &str) -> Option<AccountData> {
        self.data.read().ok()?.accounts.get(vt_accountid).cloned()
    }

    /// Get contract data by vt_symbol
    pub fn get_contract(&self, vt_symbol: &str) -> Option<ContractData> {
        self.data.read().ok()?.contracts.get(vt_symbol).cloned()
    }

    /// Get latest quote data by vt_quoteid
    pub fn get_quote(&self, vt_quoteid: &str) -> Option<QuoteData> {
        self.data.read().ok()?.quotes.get(vt_quoteid).cloned()
    }

    /// Get all tick data
    pub fn get_all_ticks(&self) -> Vec<TickData> {
        self.data.read().map(|d| d.ticks.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all order data
    pub fn get_all_orders(&self) -> Vec<OrderData> {
        self.data.read().map(|d| d.orders.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all trade data
    pub fn get_all_trades(&self) -> Vec<TradeData> {
        self.data.read().map(|d| d.trades.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all position data
    pub fn get_all_positions(&self) -> Vec<PositionData> {
        self.data.read().map(|d| d.positions.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all account data
    pub fn get_all_accounts(&self) -> Vec<AccountData> {
        self.data.read().map(|d| d.accounts.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all contract data
    pub fn get_all_contracts(&self) -> Vec<ContractData> {
        self.data.read().map(|d| d.contracts.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all quote data
    pub fn get_all_quotes(&self) -> Vec<QuoteData> {
        self.data.read().map(|d| d.quotes.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all active orders
    pub fn get_all_active_orders(&self) -> Vec<OrderData> {
        self.data.read().map(|d| d.active_orders.values().cloned().collect()).unwrap_or_default()
    }

    /// Get all active quotes
    pub fn get_all_active_quotes(&self) -> Vec<QuoteData> {
        self.data.read().map(|d| d.active_quotes.values().cloned().collect()).unwrap_or_default()
    }

    /// Process log event
    pub fn process_log(&self, log: LogData) {
        if let Ok(mut data) = self.data.write() {
            data.logs.insert(0, log);
            // Keep only last 1000 logs
            if data.logs.len() > 1000 {
                data.logs.truncate(1000);
            }
        }
    }

    /// Get all log data
    pub fn get_all_logs(&self) -> Vec<LogData> {
        self.data.read().map(|d| d.logs.clone()).unwrap_or_default()
    }
}

impl Default for OmsEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for OmsEngine {
    fn engine_name(&self) -> &str {
        "oms"
    }
}

/// Log engine for handling log events
pub struct LogEngine {
    active: bool,
}

impl LogEngine {
    /// Create a new LogEngine
    pub fn new() -> Self {
        let active = SETTINGS.get_bool("log.active").unwrap_or(true);
        Self { active }
    }

    /// Process log event
    pub fn process_log(&self, log: &LogData) {
        if !self.active {
            return;
        }

        let level = log.level;
        let msg = &log.msg;
        let gateway = &log.gateway_name;

        match level {
            10 => debug!(gateway = gateway, "{}", msg),
            20 => info!(gateway = gateway, "{}", msg),
            30 => warn!(gateway = gateway, "{}", msg),
            40 | 50 => error!(gateway = gateway, "{}", msg),
            _ => info!(gateway = gateway, "{}", msg),
        }
    }
}

impl Default for LogEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for LogEngine {
    fn engine_name(&self) -> &str {
        "log"
    }
}

/// Main engine acts as the core of the trading platform
pub struct MainEngine {
    gateways: RwLock<HashMap<String, Arc<dyn BaseGateway>>>,
    engines: RwLock<HashMap<String, Arc<dyn BaseEngine>>>,
    #[allow(dead_code)]
    apps: RwLock<HashMap<String, Arc<dyn BaseApp>>>,
    exchanges: RwLock<Vec<Exchange>>,
    
    oms_engine: Arc<OmsEngine>,
    log_engine: Arc<LogEngine>,
    
    event_tx: mpsc::UnboundedSender<(String, GatewayEvent)>,
    event_rx: RwLock<Option<mpsc::UnboundedReceiver<(String, GatewayEvent)>>>,
    
    handlers: RwLock<HashMap<String, Vec<EventHandler>>>,
    running: RwLock<bool>,
}

impl MainEngine {
    /// Create a new MainEngine
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        
        let oms_engine = Arc::new(OmsEngine::new());
        let log_engine = Arc::new(LogEngine::new());
        
        let engine = Self {
            gateways: RwLock::new(HashMap::new()),
            engines: RwLock::new(HashMap::new()),
            apps: RwLock::new(HashMap::new()),
            exchanges: RwLock::new(Vec::new()),
            oms_engine,
            log_engine,
            event_tx,
            event_rx: RwLock::new(Some(event_rx)),
            handlers: RwLock::new(HashMap::new()),
            running: RwLock::new(false),
        };
        
        // Register OMS engine
        if let Ok(mut engines) = engine.engines.write() {
            engines.insert("oms".to_string(), engine.oms_engine.clone());
            engines.insert("log".to_string(), engine.log_engine.clone());
        }
        
        engine
    }

    /// Start the main engine event loop
    pub async fn start(&self) {
        if let Ok(mut running) = self.running.write() {
            *running = true;
        }

        // Take the receiver from the RwLock
        let rx = {
            let mut rx_lock = self.event_rx.write().unwrap();
            rx_lock.take()
        };

        if let Some(mut rx) = rx {
            while *self.running.read().unwrap() {
                tokio::select! {
                    Some((event_type, event)) = rx.recv() => {
                        self.process_event(&event_type, &event);
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                        // Timer tick
                    }
                }
            }
        }
    }

    /// Process an event
    fn process_event(&self, event_type: &str, event: &GatewayEvent) {
        // Process in OMS engine
        match event {
            GatewayEvent::Tick(tick) => self.oms_engine.process_tick(tick.clone()),
            GatewayEvent::Order(order) => self.oms_engine.process_order(order.clone()),
            GatewayEvent::Trade(trade) => self.oms_engine.process_trade(trade.clone()),
            GatewayEvent::Position(position) => self.oms_engine.process_position(position.clone()),
            GatewayEvent::Account(account) => self.oms_engine.process_account(account.clone()),
            GatewayEvent::Contract(contract) => self.oms_engine.process_contract(contract.clone()),
            GatewayEvent::Quote(quote) => self.oms_engine.process_quote(quote.clone()),
            GatewayEvent::Log(log) => {
                self.log_engine.process_log(log);
                self.oms_engine.process_log(log.clone());
            }
        }

        // Call registered handlers
        if let Ok(handlers) = self.handlers.read() {
            if let Some(handler_list) = handlers.get(event_type) {
                for handler in handler_list {
                    handler(event);
                }
            }
            
            // Also call handlers for base event type (without suffix)
            let base_type = event_type.split('.').next().unwrap_or(event_type);
            if base_type != event_type {
                if let Some(handler_list) = handlers.get(base_type) {
                    for handler in handler_list {
                        handler(event);
                    }
                }
            }
        }
    }

    /// Add a gateway
    pub fn add_gateway(&self, gateway: Arc<dyn BaseGateway>) -> Arc<dyn BaseGateway> {
        let gateway_name = gateway.gateway_name().to_string();
        
        if let Ok(mut gateways) = self.gateways.write() {
            gateways.insert(gateway_name, gateway.clone());
        }
        
        gateway
    }

    /// Get a gateway by name
    pub fn get_gateway(&self, gateway_name: &str) -> Option<Arc<dyn BaseGateway>> {
        self.gateways.read().ok()?.get(gateway_name).cloned()
    }

    /// Get all gateway names
    pub fn get_all_gateway_names(&self) -> Vec<String> {
        self.gateways.read().map(|g| g.keys().cloned().collect()).unwrap_or_default()
    }

    /// Get all exchanges
    pub fn get_all_exchanges(&self) -> Vec<Exchange> {
        self.exchanges.read().map(|e| e.clone()).unwrap_or_default()
    }

    /// Write a log message
    pub fn write_log(&self, msg: impl Into<String>, source: &str) {
        let log = LogData::new(source.to_string(), msg.into());
        let _ = self.event_tx.send((EVENT_LOG.to_string(), GatewayEvent::Log(log)));
    }

    /// Connect to a gateway
    pub async fn connect(&self, setting: GatewaySettings, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("连接登录 -> {}", gateway_name), "MainEngine");
            gateway.connect(setting).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Subscribe to tick data
    pub async fn subscribe(&self, req: SubscribeRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("订阅行情 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.subscribe(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Send an order
    pub async fn send_order(&self, req: OrderRequest, gateway_name: &str) -> Result<String, String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("委托下单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.send_order(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Cancel an order
    pub async fn cancel_order(&self, req: CancelRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("委托撤单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.cancel_order(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Send a quote
    pub async fn send_quote(&self, req: QuoteRequest, gateway_name: &str) -> Result<String, String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("报价下单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.send_quote(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Cancel a quote
    pub async fn cancel_quote(&self, req: CancelRequest, gateway_name: &str) -> Result<(), String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("报价撤单 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.cancel_quote(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Query history data
    pub async fn query_history(&self, req: HistoryRequest, gateway_name: &str) -> Result<Vec<BarData>, String> {
        if let Some(gateway) = self.get_gateway(gateway_name) {
            self.write_log(format!("查询K线 -> {}：{:?}", gateway_name, req), "MainEngine");
            gateway.query_history(req).await
        } else {
            Err(format!("找不到底层接口：{}", gateway_name))
        }
    }

    /// Get OMS engine
    pub fn oms(&self) -> &Arc<OmsEngine> {
        &self.oms_engine
    }

    /// Get tick data
    pub fn get_tick(&self, vt_symbol: &str) -> Option<TickData> {
        self.oms_engine.get_tick(vt_symbol)
    }

    /// Get order data
    pub fn get_order(&self, vt_orderid: &str) -> Option<OrderData> {
        self.oms_engine.get_order(vt_orderid)
    }

    /// Get trade data
    pub fn get_trade(&self, vt_tradeid: &str) -> Option<TradeData> {
        self.oms_engine.get_trade(vt_tradeid)
    }

    /// Get position data
    pub fn get_position(&self, vt_positionid: &str) -> Option<PositionData> {
        self.oms_engine.get_position(vt_positionid)
    }

    /// Get account data
    pub fn get_account(&self, vt_accountid: &str) -> Option<AccountData> {
        self.oms_engine.get_account(vt_accountid)
    }

    /// Get contract data
    pub fn get_contract(&self, vt_symbol: &str) -> Option<ContractData> {
        self.oms_engine.get_contract(vt_symbol)
    }

    /// Get quote data
    pub fn get_quote(&self, vt_quoteid: &str) -> Option<QuoteData> {
        self.oms_engine.get_quote(vt_quoteid)
    }

    /// Get all ticks
    pub fn get_all_ticks(&self) -> Vec<TickData> {
        self.oms_engine.get_all_ticks()
    }

    /// Get all orders
    pub fn get_all_orders(&self) -> Vec<OrderData> {
        self.oms_engine.get_all_orders()
    }

    /// Get all trades
    pub fn get_all_trades(&self) -> Vec<TradeData> {
        self.oms_engine.get_all_trades()
    }

    /// Get all positions
    pub fn get_all_positions(&self) -> Vec<PositionData> {
        self.oms_engine.get_all_positions()
    }

    /// Get all accounts
    pub fn get_all_accounts(&self) -> Vec<AccountData> {
        self.oms_engine.get_all_accounts()
    }

    /// Get all contracts
    pub fn get_all_contracts(&self) -> Vec<ContractData> {
        self.oms_engine.get_all_contracts()
    }

    /// Get all quotes
    pub fn get_all_quotes(&self) -> Vec<QuoteData> {
        self.oms_engine.get_all_quotes()
    }

    /// Get all active orders
    pub fn get_all_active_orders(&self) -> Vec<OrderData> {
        self.oms_engine.get_all_active_orders()
    }

    /// Get all active quotes
    pub fn get_all_active_quotes(&self) -> Vec<QuoteData> {
        self.oms_engine.get_all_active_quotes()
    }

    /// Get all logs
    pub fn get_all_logs(&self) -> Vec<LogData> {
        self.oms_engine.get_all_logs()
    }

    /// Register an event handler
    pub fn register_handler(&self, event_type: &str, handler: EventHandler) {
        if let Ok(mut handlers) = self.handlers.write() {
            handlers.entry(event_type.to_string())
                .or_insert_with(Vec::new)
                .push(handler);
        }
    }

    /// Get event sender for gateways
    pub fn get_event_sender(&self) -> mpsc::UnboundedSender<(String, GatewayEvent)> {
        self.event_tx.clone()
    }

    /// Close the main engine
    pub async fn close(&self) {
        // Stop event loop
        if let Ok(mut running) = self.running.write() {
            *running = false;
        }

        // Close all engines
        if let Ok(engines) = self.engines.read() {
            for engine in engines.values() {
                engine.close();
            }
        }

        // Close all gateways
        if let Ok(gateways) = self.gateways.read() {
            for gateway in gateways.values() {
                gateway.close().await;
            }
        }
    }
}

impl Default for MainEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oms_engine() {
        let oms = OmsEngine::new();
        
        // Test empty state
        assert!(oms.get_tick("TEST.LOCAL").is_none());
        assert!(oms.get_all_ticks().is_empty());
        
        // Test adding tick
        let tick = TickData::new(
            "test".to_string(),
            "TEST".to_string(),
            Exchange::Local,
            chrono::Utc::now(),
        );
        oms.process_tick(tick);
        
        assert!(oms.get_tick("TEST.LOCAL").is_some());
        assert_eq!(oms.get_all_ticks().len(), 1);
    }

    #[test]
    fn test_main_engine() {
        let engine = MainEngine::new();
        
        assert!(engine.get_all_gateway_names().is_empty());
        assert!(engine.get_tick("TEST.LOCAL").is_none());
    }
}
