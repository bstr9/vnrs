//! Binance Spot Gateway implementation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Local, Utc};
use serde_json::json;
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use super::config::{BinanceConfigs, BinanceGatewayConfig};
use super::constants::*;
use super::rest_client::BinanceRestClient;
use super::websocket_client::{BinanceWebSocketClient, WsMessageHandler};

use crate::trader::{
    AccountData, BarData, CancelRequest, ContractData, Exchange,
    GatewayEventSender, GatewaySettings, GatewaySettingValue,
    HistoryRequest, Offset, OrderData, OrderRequest, OrderType,
    Product, Status, SubscribeRequest, TickData,
    TradeData,
};
use crate::trader::gateway::BaseGateway;

fn format_price(value: f64) -> String {
    if value == 0.0 { return "0".to_string(); }
    let s = format!("{:.8}", value);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

/// Binance Spot Gateway
pub struct BinanceSpotGateway {
    /// Gateway name
    gateway_name: String,
    /// REST API client
    rest_client: Arc<BinanceRestClient>,
    /// Market data WebSocket client
    market_ws: Arc<BinanceWebSocketClient>,
    /// Trade WebSocket client
    trade_ws: Arc<BinanceWebSocketClient>,
    /// Event sender
    event_sender: Arc<RwLock<Option<GatewayEventSender>>>,
    /// Order count for generating order IDs
    order_count: AtomicU64,
    /// Connect time for generating order IDs
    connect_time: AtomicI64,
    /// User stream subscription ID (new WebSocket API)
    subscription_id: Arc<RwLock<Option<u64>>>,
    /// Server mode (REAL or TESTNET)
    server: Arc<RwLock<String>>,
    /// Cached orders
    orders: Arc<RwLock<HashMap<String, OrderData>>>,
    /// Cached contracts
    contracts: Arc<RwLock<HashMap<String, ContractData>>>,
    /// Cached ticks
    ticks: Arc<RwLock<HashMap<String, TickData>>>,
}

impl BinanceSpotGateway {
    /// Create a new Binance Spot Gateway
    pub fn new(gateway_name: &str) -> Self {
        Self {
            gateway_name: gateway_name.to_string(),
            rest_client: Arc::new(BinanceRestClient::new()),
            market_ws: Arc::new(BinanceWebSocketClient::new(gateway_name)),
            trade_ws: Arc::new(BinanceWebSocketClient::new(gateway_name)),
            event_sender: Arc::new(RwLock::new(None)),
            order_count: AtomicU64::new(1_000_000),
            connect_time: AtomicI64::new(0),
            subscription_id: Arc::new(RwLock::new(None)),
            server: Arc::new(RwLock::new("REAL".to_string())),
            orders: Arc::new(RwLock::new(HashMap::new())),
            contracts: Arc::new(RwLock::new(HashMap::new())),
            ticks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set event sender
    pub async fn set_event_sender(&self, sender: GatewayEventSender) {
        *self.event_sender.write().await = Some(sender);
    }

    /// Load saved configuration for this gateway
    pub fn load_config(&self) -> Option<BinanceGatewayConfig> {
        let configs = BinanceConfigs::load();
        configs.get(&self.gateway_name).cloned()
    }

    /// Get settings from saved configuration
    pub fn load_settings(&self) -> Option<GatewaySettings> {
        self.load_config().map(|config| config.to_settings())
    }

    /// Get settings with auto-load from saved configuration
    /// If saved configuration exists, return it; otherwise return default settings
    pub fn get_settings(&self) -> GatewaySettings {
        self.load_settings().unwrap_or_else(Self::default_setting)
    }

    /// Write log message
    async fn write_log(&self, msg: &str) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.write_log(msg);
        }
        info!("{}: {}", self.gateway_name, msg);
    }

    /// Push tick event
    #[allow(dead_code)]
    async fn on_tick(&self, tick: TickData) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_tick(tick);
        }
    }

    /// Push order event
    async fn on_order(&self, order: OrderData) {
        self.orders.write().await.insert(order.orderid.clone(), order.clone());
        
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_order(order);
        }
    }

    /// Push trade event
    #[allow(dead_code)]
    async fn on_trade(&self, trade: TradeData) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_trade(trade);
        }
    }

    /// Push account event
    async fn on_account(&self, account: AccountData) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_account(account);
        }
    }

    /// Push contract event
    async fn on_contract(&self, contract: ContractData) {
        self.contracts.write().await.insert(contract.symbol.clone(), contract.clone());
        
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_contract(contract);
        }
    }

    /// Generate new order ID
    fn new_order_id(&self) -> String {
        let count = self.order_count.fetch_add(1, Ordering::SeqCst);
        let connect_time = self.connect_time.load(Ordering::SeqCst);
        format!("{}", connect_time + count as i64)
    }

    /// Query server time and calculate offset
    async fn query_time(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/api/v3/time", &params, Security::None).await?;

        let server_time = data["serverTime"].as_i64().unwrap_or(0);
        let local_time = Utc::now().timestamp_millis();
        let offset = local_time - server_time;
        
        self.rest_client.set_time_offset(offset);
        self.write_log(&format!("时间同步成功，偏移: {}ms", offset)).await;
        
        Ok(())
    }

    /// Query account balance
    async fn query_account_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/api/v3/account", &params, Security::Signed).await?;

        if let Some(balances) = data["balances"].as_array() {
            for balance in balances {
                let free: f64 = balance["free"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                let locked: f64 = balance["locked"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                let total = free + locked;

                if total > 0.0 {
                    let account = AccountData {
                        accountid: balance["asset"].as_str().unwrap_or("").to_string(),
                        balance: total,
                        frozen: locked,
                        gateway_name: self.gateway_name.clone(),
                        extra: None,
                    };
                    self.on_account(account).await;
                }
            }
        }

        self.write_log("账户资金查询成功").await;
        Ok(())
    }

    /// Query historical trades for symbols with open orders
    /// Fetches up to 3 years of trade history with pagination.
    async fn query_trade_impl(&self) -> Result<(), String> {
        // Binance myTrades requires symbol parameter.
        // Collect symbols from cached open orders and positions to query their trade history.
        let symbols: Vec<String> = {
            let orders = self.orders.read().await;
            let mut set = std::collections::HashSet::new();
            for order in orders.values() {
                set.insert(order.symbol.to_uppercase());
            }
            set.into_iter().collect()
        };

        if symbols.is_empty() {
            self.write_log("无持仓委托，跳过历史成交查询").await;
            return Ok(());
        }

        // Query trades from 3 years ago
        let start_time = Utc::now().timestamp_millis() - 3 * 365 * 24 * 60 * 60 * 1000;
        let limit = 1000i64;

        let mut total_count = 0usize;
        for symbol in &symbols {
            let mut cursor = start_time;
            let mut symbol_count = 0usize;

            loop {
                let mut params = HashMap::new();
                params.insert("symbol".to_string(), symbol.clone());
                params.insert("limit".to_string(), limit.to_string());
                params.insert("startTime".to_string(), cursor.to_string());

                match self.rest_client.get("/api/v3/myTrades", &params, Security::Signed).await {
                    Ok(data) => {
                        if let Some(trades) = data.as_array() {
                            if trades.is_empty() {
                                break; // No more trades
                            }

                            let mut last_time = 0i64;
                            for d in trades {
                                let trade_time = d["time"].as_i64().unwrap_or(0);
                                if trade_time > last_time {
                                    last_time = trade_time;
                                }

                                let side = d["side"].as_str().unwrap_or("");
                                let direction = DIRECTION_BINANCE2VT.get(side).copied();

                                let trade = TradeData {
                                    symbol: symbol.to_lowercase(),
                                    exchange: Exchange::Binance,
                                    orderid: d["orderId"].as_i64().unwrap_or(0).to_string(),
                                    tradeid: d["id"].as_i64().unwrap_or(0).to_string(),
                                    direction,
                                    offset: Offset::None,
                                    price: d["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                    volume: d["qty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                    datetime: Some(timestamp_to_datetime(trade_time)),
                                    gateway_name: self.gateway_name.clone(),
                                    extra: None,
                                };
                                self.on_trade(trade).await;
                                symbol_count += 1;
                            }

                            total_count += symbol_count;

                            // If we got less than limit, we've reached the end
                            if trades.len() < limit as usize {
                                break;
                            }

                            // Move cursor past the last trade time
                            cursor = last_time + 1;
                        } else {
                            break;
                        }
                    }
                    Err(e) => {
                        self.write_log(&format!("查询 {} 历史成交失败: {}", symbol, e)).await;
                        break;
                    }
                }
            }

            if symbol_count > 0 {
                self.write_log(&format!("查询 {} 历史成交: {} 条", symbol, symbol_count)).await;
            }
        }

        self.write_log(&format!("历史成交查询成功，共 {} 条", total_count)).await;
        Ok(())
    }

    /// Query open orders
    async fn query_order_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/api/v3/openOrders", &params, Security::Signed).await?;

        if let Some(orders) = data.as_array() {
            for d in orders {
                let order_type_str = d["type"].as_str().unwrap_or("");
                let order_type = ORDERTYPE_BINANCE2VT.get(order_type_str);
                
                if order_type.is_none() {
                    continue;
                }

                let status_str = d["status"].as_str().unwrap_or("");
                let direction_str = d["side"].as_str().unwrap_or("");

                let order = OrderData {
                    symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                    exchange: Exchange::Binance,
                    orderid: d["clientOrderId"].as_str().unwrap_or("").to_string(),
                    order_type: *order_type.expect("order_type verified non-None above"),
                    direction: DIRECTION_BINANCE2VT.get(direction_str).copied(),
                    offset: Offset::None,
                    price: d["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    volume: d["origQty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    traded: d["executedQty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    status: STATUS_BINANCE2VT.get(status_str).copied().unwrap_or(Status::Submitting),
                    datetime: Some(timestamp_to_datetime(d["time"].as_i64().unwrap_or(0))),
                    reference: String::new(),
                    gateway_name: self.gateway_name.clone(),
                    extra: None,
                };
                self.on_order(order).await;
            }
        }

        self.write_log("委托信息查询成功").await;
        Ok(())
    }

    /// Query contracts
    async fn query_contract_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/api/v3/exchangeInfo", &params, Security::None).await?;

        if let Some(symbols) = data["symbols"].as_array() {
            for d in symbols {
                let base_asset = d["baseAsset"].as_str().unwrap_or("");
                let quote_asset = d["quoteAsset"].as_str().unwrap_or("");
                let name = format!("{}/{}", base_asset.to_uppercase(), quote_asset.to_uppercase());

                let mut pricetick: f64 = 1.0;
                let mut min_volume: f64 = 1.0;

                if let Some(filters) = d["filters"].as_array() {
                    for f in filters {
                        let filter_type = f["filterType"].as_str().unwrap_or("");
                        match filter_type {
                            "PRICE_FILTER" => {
                                pricetick = f["tickSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0);
                            }
                            "LOT_SIZE" => {
                                min_volume = f["stepSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0);
                            }
                            _ => {}
                        }
                    }
                }

                let contract = ContractData {
                    symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                    exchange: Exchange::Binance,
                    name,
                    product: Product::Spot,
                    size: 1.0,
                    pricetick,
                    min_volume,
                    max_volume: None,
                    stop_supported: false,
                    net_position: true,
                    history_data: true,
                    option_strike: None,
                    option_underlying: None,
                    option_type: None,
                    option_listed: None,
                    option_expiry: None,
                    option_portfolio: None,
                    option_index: None,
                    gateway_name: self.gateway_name.clone(),
                    extra: None,
                };
                self.on_contract(contract).await;
            }
        }

        self.write_log("合约信息查询成功").await;
        Ok(())
    }

    /// Start user data stream via WebSocket API
    async fn start_user_stream(&self, proxy_host: &str, proxy_port: u16) -> Result<(), String> {
        // Debug log
        tracing::info!("start_user_stream: proxy_host='{}', proxy_port={}", proxy_host, proxy_port);

        // 1. Build WebSocket API URL
        let server = self.server.read().await.clone();
        let url = if server == "REAL" {
            SPOT_WS_API_HOST
        } else {
            SPOT_TESTNET_WS_API_HOST
        };

        // 2. Setup trade WebSocket handler BEFORE connecting
        // The handler must handle THREE types of messages:
        //   a) Subscription responses: {"id": "...", "status": 200, "result": {"subscriptionId": 0}}
        //   b) User data events: {"subscriptionId": 0, "event": {"e": "outboundAccountPosition", ...}}
        //   c) Event stream terminated: {"subscriptionId": 0, "event": {"e": "eventStreamTerminated"}}
        let event_sender = self.event_sender.clone();
        let orders = self.orders.clone();
        let gateway_name = self.gateway_name.clone();
        let subscription_id = self.subscription_id.clone();
        let order_lock = Arc::new(Mutex::new(()));

        let handler: WsMessageHandler = Arc::new(move |packet| {
            let event_sender = event_sender.clone();
            let orders = orders.clone();
            let gateway_name = gateway_name.clone();
            let subscription_id = subscription_id.clone();
            let lock = order_lock.clone();

            tokio::spawn(async move {
                // Check if this is a subscription response (has "status" and "result" fields)
                if packet.get("status").is_some() && packet.get("result").is_some() {
                    let status = packet["status"].as_u64().unwrap_or(0);
                    let req_id = packet.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    if status == 200 {
                        if let Some(result) = packet.get("result") {
                            let sub_id = result["subscriptionId"].as_u64();
                            *subscription_id.write().await = sub_id;
                            tracing::info!("{}: User data stream subscribed, subscriptionId={:?}, reqId={}", gateway_name, sub_id, req_id);
                        }
                    } else {
                        let error_msg = packet.get("error")
                            .and_then(|e| e.get("msg"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown error");
                        tracing::error!("{}: User data stream subscription failed (status={}): {}", gateway_name, status, error_msg);
                    }
                    return;
                }

                let _guard = lock.lock().await;

                // Extract event data - new format nests under "event" key
                let event_data = match packet.get("event") {
                    Some(e) => e.clone(),
                    None => packet.clone(), // fallback for unexpected format
                };

                let event_type = event_data.get("e").and_then(|s| s.as_str()).unwrap_or("");

                match event_type {
                    "outboundAccountPosition" => {
                        if let Some(balances) = event_data.get("B").and_then(|b| b.as_array()) {
                            for b in balances {
                                let free: f64 = b["f"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                                let locked: f64 = b["l"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                                let total = free + locked;

                                if total > 0.0 {
                                    let account = AccountData {
                                        accountid: b["a"].as_str().unwrap_or("").to_string(),
                                        balance: total,
                                        frozen: locked,
                                        gateway_name: gateway_name.clone(),
                                        extra: None,
                                    };
                                    if let Some(sender) = event_sender.read().await.as_ref() {
                                        sender.on_account(account);
                                    }
                                }
                            }
                        }
                    }
                    "executionReport" => {
                        let order_type_str = event_data["o"].as_str().unwrap_or("");
                        let order_type = ORDERTYPE_BINANCE2VT.get(order_type_str);

                        if order_type.is_none() {
                            return;
                        }

                        let orderid = if event_data["C"].as_str().unwrap_or("").is_empty() {
                            event_data["c"].as_str().unwrap_or("").to_string()
                        } else {
                            event_data["C"].as_str().unwrap_or("").to_string()
                        };

                        let status_str = event_data["X"].as_str().unwrap_or("");
                        let direction_str = event_data["S"].as_str().unwrap_or("");

                        let order = OrderData {
                            symbol: event_data["s"].as_str().unwrap_or("").to_lowercase(),
                            exchange: Exchange::Binance,
                            orderid: orderid.clone(),
                            order_type: *order_type.expect("order_type verified non-None above"),
                            direction: DIRECTION_BINANCE2VT.get(direction_str).copied(),
                            offset: Offset::None,
                            price: event_data["p"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                            volume: event_data["q"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                            traded: event_data["z"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                            status: STATUS_BINANCE2VT.get(status_str).copied().unwrap_or(Status::Submitting),
                            datetime: Some(timestamp_to_datetime(event_data["O"].as_i64().unwrap_or(0))),
                            reference: String::new(),
                            gateway_name: gateway_name.clone(),
                            extra: None,
                        };

                        orders.write().await.insert(orderid.clone(), order.clone());
                        if let Some(sender) = event_sender.read().await.as_ref() {
                            sender.on_order(order.clone());
                        }

                        // Check for trade
                        let trade_volume: f64 = event_data["l"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        if trade_volume > 0.0 {
                            let trade = TradeData {
                                symbol: order.symbol.clone(),
                                exchange: Exchange::Binance,
                                orderid,
                                tradeid: event_data["t"].as_i64().unwrap_or(0).to_string(),
                                direction: order.direction,
                                offset: Offset::None,
                                price: event_data["L"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                volume: trade_volume,
                                datetime: Some(timestamp_to_datetime(event_data["T"].as_i64().unwrap_or(0))),
                                gateway_name: gateway_name.clone(),
                                extra: None,
                            };
                            if let Some(sender) = event_sender.read().await.as_ref() {
                                sender.on_trade(trade);
                            }
                        }
                    }
                    "eventStreamTerminated" => {
                        warn!("{}: Event stream terminated, will need to reconnect", gateway_name);
                    }
                    "listenKeyExpired" => {
                        warn!("{}: Listen key expired (legacy event), reconnecting via WebSocket API", gateway_name);
                    }
                    _ => {}
                }
            });
        });

        self.trade_ws.set_handler(handler).await;
        self.trade_ws.connect(url, proxy_host, proxy_port).await?;

        // 3. Send subscription request after connecting
        let api_key = self.rest_client.get_api_key().await;
        let timestamp = self.rest_client.get_timestamp_ms();
        let query = format!("apiKey={}&timestamp={}", api_key, timestamp);
        let signature = self.rest_client.sign_query(&query).await;

        let subscribe_msg = json!({
            "id": format!("sub_{}", timestamp),
            "method": "userDataStream.subscribe.signature",
            "params": {
                "apiKey": api_key,
                "timestamp": timestamp,
                "signature": signature
            }
        });

        self.trade_ws.send(subscribe_msg).await?;
        self.write_log("交易Websocket API连接成功，已发送订阅请求").await;

        Ok(())
    }

    /// Reconnect user data stream (disconnect, reconnect, re-subscribe)
    pub async fn reconnect_user_stream(&self) -> Result<(), String> {
        self.trade_ws.disconnect().await;
        *self.subscription_id.write().await = None;

        let proxy_host = self.rest_client.get_proxy_host().await;
        let proxy_port = self.rest_client.get_proxy_port().await;
        self.start_user_stream(&proxy_host, proxy_port).await?;
        self.write_log("User data stream reconnected via WebSocket API").await;
        Ok(())
    }
}

#[async_trait]
impl BaseGateway for BinanceSpotGateway {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn default_exchange(&self) -> Exchange {
        Exchange::Binance
    }

    fn default_name() -> &'static str {
        "BINANCE_SPOT"
    }

    fn default_setting() -> GatewaySettings {
        // 尝试加载已保存的配置
        let configs = BinanceConfigs::load();
        if let Some(config) = configs.get(Self::default_name()) {
            info!("加载已保存的配置: {}", Self::default_name());
            return config.to_settings();
        }
        
        // 如果没有保存的配置，返回空的默认配置
        let mut settings = GatewaySettings::new();
        settings.insert("key".to_string(), GatewaySettingValue::String(String::new()));
        settings.insert("secret".to_string(), GatewaySettingValue::String(String::new()));
        settings.insert("server".to_string(), GatewaySettingValue::String("REAL".to_string()));
        settings.insert("proxy_host".to_string(), GatewaySettingValue::String(String::new()));
        settings.insert("proxy_port".to_string(), GatewaySettingValue::Int(0));
        settings
    }

    fn exchanges() -> Vec<Exchange> {
        vec![Exchange::Binance]
    }

    async fn connect(&self, setting: GatewaySettings) -> Result<(), String> {
        // Load existing config or use provided settings
        let mut configs = BinanceConfigs::load();
        let config = BinanceGatewayConfig::from_settings(&setting);
        
        // Save the configuration
        configs.set(self.gateway_name.clone(), config.clone());
        if let Err(e) = configs.save() {
            self.write_log(&format!("警告: 保存配置失败: {}", e)).await;
        } else {
            self.write_log("配置已保存到 .rstrader/binance/gateway_configs.json").await;
        }
        
        let key = config.key;
        let secret = config.secret;
        let server = config.server;
        let proxy_host = config.proxy_host;
        let proxy_port = config.proxy_port;

        *self.server.write().await = server.clone();

        let now = Local::now();
        let connect_time = now.format("%y%m%d%H%M%S").to_string().parse::<i64>().unwrap_or(0);
        self.connect_time.store(connect_time * 1_000_000, Ordering::SeqCst);

        let host = if server == "REAL" { SPOT_REST_HOST } else { SPOT_TESTNET_REST_HOST };
        self.rest_client.init(&key, &secret, host, &proxy_host, proxy_port).await;
        self.write_log("REST API启动成功").await;

        self.query_time().await?;
        self.query_account_impl().await?;
        self.query_order_impl().await?;
        self.query_trade_impl().await?;
        self.query_contract_impl().await?;
        self.start_user_stream(&proxy_host, proxy_port).await?;

        let market_url = if server == "REAL" { SPOT_WS_DATA_HOST } else { SPOT_TESTNET_WS_DATA_HOST };
        let ticks = self.ticks.clone();
        let event_sender = self.event_sender.clone();
        let market_lock = Arc::new(Mutex::new(()));
        
        let handler: WsMessageHandler = Arc::new(move |packet| {
            let ticks = ticks.clone();
            let event_sender = event_sender.clone();
            let lock = market_lock.clone();
            
            tokio::spawn(async move {
                let _guard = lock.lock().await;
                let stream = match packet.get("stream").and_then(|s| s.as_str()) {
                    Some(s) => s,
                    None => return,
                };
                let data = match packet.get("data") {
                    Some(d) => d,
                    None => return,
                };

                let parts: Vec<&str> = stream.split('@').collect();
                if parts.len() != 2 { return; }

                let symbol = parts[0];
                let channel = parts[1];

                let mut ticks_guard = ticks.write().await;
                let tick = match ticks_guard.get_mut(symbol) {
                    Some(t) => t,
                    None => return,
                };

                match channel {
                    "ticker" => {
                        tick.volume = data["v"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        tick.turnover = data["q"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        tick.open_price = data["o"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        tick.high_price = data["h"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        tick.low_price = data["l"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        tick.last_price = data["c"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        tick.datetime = timestamp_to_datetime(data["E"].as_i64().unwrap_or(0));
                    }
                    "depth5" => {
                        if let Some(bids) = data["b"].as_array() {
                            for (i, bid) in bids.iter().take(5).enumerate() {
                                if let Some(arr) = bid.as_array() {
                                    let price: f64 = arr.first().and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0.0);
                                    let vol: f64 = arr.get(1).and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0.0);
                                    match i {
                                        0 => { tick.bid_price_1 = price; tick.bid_volume_1 = vol; }
                                        1 => { tick.bid_price_2 = price; tick.bid_volume_2 = vol; }
                                        2 => { tick.bid_price_3 = price; tick.bid_volume_3 = vol; }
                                        3 => { tick.bid_price_4 = price; tick.bid_volume_4 = vol; }
                                        4 => { tick.bid_price_5 = price; tick.bid_volume_5 = vol; }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if let Some(asks) = data["a"].as_array() {
                            for (i, ask) in asks.iter().take(5).enumerate() {
                                if let Some(arr) = ask.as_array() {
                                    let price: f64 = arr.first().and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0.0);
                                    let vol: f64 = arr.get(1).and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0.0);
                                    match i {
                                        0 => { tick.ask_price_1 = price; tick.ask_volume_1 = vol; }
                                        1 => { tick.ask_price_2 = price; tick.ask_volume_2 = vol; }
                                        2 => { tick.ask_price_3 = price; tick.ask_volume_3 = vol; }
                                        3 => { tick.ask_price_4 = price; tick.ask_volume_4 = vol; }
                                        4 => { tick.ask_price_5 = price; tick.ask_volume_5 = vol; }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                if tick.last_price > 0.0 {
                    tick.localtime = Some(Utc::now());
                    if let Some(sender) = event_sender.read().await.as_ref() {
                        sender.on_tick(tick.clone());
                    }
                }
            });
        });

        self.market_ws.set_handler(handler).await;
        self.market_ws.connect(market_url, &proxy_host, proxy_port).await?;
        self.write_log("行情Websocket API连接成功").await;

        Ok(())
    }

    async fn close(&self) {
        self.market_ws.disconnect().await;
        self.trade_ws.disconnect().await;
        self.write_log("Gateway已关闭").await;
    }

    async fn subscribe(&self, req: SubscribeRequest) -> Result<(), String> {
        let symbol = req.symbol.to_lowercase();
        
        // 检查是否已订阅
        if self.ticks.read().await.contains_key(&symbol) {
            return Ok(()); // 已订阅
        }

        // 检查合约是否在已知列表中
        let contracts = self.contracts.read().await;
        if !contracts.contains_key(&symbol) {
            // 合约不在预加载列表中，发出警告但允许继续订阅（动态订阅）
            self.write_log(&format!("⚠️ 警告: 合约 {} 不在已知合约列表中，可能是无效的交易对", symbol.to_uppercase())).await;
        }
        drop(contracts);

        // 创建 tick 数据
        let tick = TickData::new(self.gateway_name.clone(), symbol.clone(), Exchange::Binance, Utc::now());
        self.ticks.write().await.insert(symbol.clone(), tick);

        // 订阅 ticker 和 depth5 数据流
        let channels = vec![format!("{}@ticker", symbol), format!("{}@depth5", symbol)];
        self.market_ws.subscribe(channels).await?;
        self.write_log(&format!("订阅行情: {}", symbol)).await;
        Ok(())
    }

    async fn send_order(&self, req: OrderRequest) -> Result<String, String> {
        let orderid = self.new_order_id();
        let order = req.create_order_data(orderid.clone(), self.gateway_name.clone());
        self.on_order(order.clone()).await;

        let mut params = HashMap::new();
        params.insert("symbol".to_string(), req.symbol.to_uppercase());
        params.insert("side".to_string(), DIRECTION_VT2BINANCE.get(&req.direction).unwrap_or(&"BUY").to_string());
        params.insert("type".to_string(), ORDERTYPE_VT2BINANCE.get(&req.order_type).unwrap_or(&"LIMIT").to_string());
        params.insert("quantity".to_string(), format_price(req.volume));
        params.insert("newClientOrderId".to_string(), orderid.clone());
        params.insert("newOrderRespType".to_string(), "ACK".to_string());

        if req.order_type == OrderType::Limit {
            params.insert("timeInForce".to_string(), "GTC".to_string());
            params.insert("price".to_string(), format_price(req.price));
        }

        match self.rest_client.post("/api/v3/order", &params, Security::Signed).await {
            Ok(_) => Ok(format!("{}.{}", self.gateway_name, orderid)),
            Err(e) => {
                let mut rejected_order = order;
                rejected_order.status = Status::Rejected;
                self.on_order(rejected_order).await;
                self.write_log(&format!("委托失败: {}", e)).await;
                Err(e)
            }
        }
    }

    async fn cancel_order(&self, req: CancelRequest) -> Result<(), String> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), req.symbol.to_uppercase());
        params.insert("origClientOrderId".to_string(), req.orderid.clone());

        match self.rest_client.delete("/api/v3/order", &params, Security::Signed).await {
            Ok(_) => { self.write_log(&format!("撤单成功: {}", req.orderid)).await; Ok(()) }
            Err(e) => { self.write_log(&format!("撤单失败: {}", e)).await; Err(e) }
        }
    }

    async fn query_account(&self) -> Result<(), String> { self.query_account_impl().await }
    async fn query_position(&self) -> Result<(), String> { Ok(()) }

    async fn query_history(&self, req: HistoryRequest) -> Result<Vec<BarData>, String> {
        let mut history = Vec::new();
        let limit = 1000;
        let mut start_time = req.start.timestamp() * 1000;
        let interval = req.interval.unwrap_or(crate::trader::Interval::Minute);
        let interval_str = INTERVAL_VT2BINANCE.get(&interval).unwrap_or(&"1m");
        let interval_ms = get_interval_seconds(interval) * 1000;

        loop {
            let mut params = HashMap::new();
            params.insert("symbol".to_string(), req.symbol.to_uppercase());
            params.insert("interval".to_string(), interval_str.to_string());
            params.insert("limit".to_string(), limit.to_string());
            params.insert("startTime".to_string(), start_time.to_string());
            if let Some(end) = req.end {
                params.insert("endTime".to_string(), (end.timestamp() * 1000).to_string());
            }

            let data = self.rest_client.get("/api/v3/klines", &params, Security::None).await?;
            let rows = match data.as_array() {
                Some(r) if !r.is_empty() => r,
                _ => break,
            };

            for row in rows {
                if let Some(arr) = row.as_array() {
                    history.push(BarData {
                        symbol: req.symbol.clone(),
                        exchange: Exchange::Binance,
                        datetime: timestamp_to_datetime(arr[0].as_i64().unwrap_or(0)),
                        interval: Some(interval),
                        volume: arr[5].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        turnover: arr[7].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        open_interest: 0.0,
                        open_price: arr[1].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        high_price: arr[2].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        low_price: arr[3].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        close_price: arr[4].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        gateway_name: self.gateway_name.clone(),
                        extra: None,
                    });
                }
            }

            if rows.len() < limit { break; }
            if let Some(last) = rows.last().and_then(|r| r.as_array()) {
                start_time = last[0].as_i64().unwrap_or(0) + interval_ms;
            }
        }

        self.write_log(&format!("获取历史数据成功: {} 条", history.len())).await;
        Ok(history)
    }
}
