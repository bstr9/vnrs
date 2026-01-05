//! Binance USDT-M Futures Gateway implementation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Local, Utc};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::config::{BinanceConfigs, BinanceGatewayConfig};
use super::constants::*;
use super::rest_client::BinanceRestClient;
use super::websocket_client::{BinanceWebSocketClient, WsMessageHandler};

use crate::trader::{
    AccountData, BarData, CancelRequest, ContractData, Direction, Exchange,
    GatewayEventSender, GatewaySettings, GatewaySettingValue,
    HistoryRequest, Offset, OrderData, OrderRequest, OrderType,
    PositionData, Product, Status, SubscribeRequest, TickData, TradeData,
};
use crate::trader::gateway::BaseGateway;

/// Binance USDT-M Futures Gateway
pub struct BinanceUsdtGateway {
    gateway_name: String,
    rest_client: Arc<BinanceRestClient>,
    market_ws: Arc<BinanceWebSocketClient>,
    trade_ws: Arc<BinanceWebSocketClient>,
    event_sender: Arc<RwLock<Option<GatewayEventSender>>>,
    order_count: AtomicU64,
    connect_time: AtomicI64,
    listen_key: Arc<RwLock<String>>,
    keep_alive_count: AtomicU64,
    server: Arc<RwLock<String>>,
    orders: Arc<RwLock<HashMap<String, OrderData>>>,
    contracts: Arc<RwLock<HashMap<String, ContractData>>>,
    ticks: Arc<RwLock<HashMap<String, TickData>>>,
    positions: Arc<RwLock<HashMap<String, PositionData>>>,
}

impl BinanceUsdtGateway {
    pub fn new(gateway_name: &str) -> Self {
        Self {
            gateway_name: gateway_name.to_string(),
            rest_client: Arc::new(BinanceRestClient::new()),
            market_ws: Arc::new(BinanceWebSocketClient::new(gateway_name)),
            trade_ws: Arc::new(BinanceWebSocketClient::new(gateway_name)),
            event_sender: Arc::new(RwLock::new(None)),
            order_count: AtomicU64::new(1_000_000),
            connect_time: AtomicI64::new(0),
            listen_key: Arc::new(RwLock::new(String::new())),
            keep_alive_count: AtomicU64::new(0),
            server: Arc::new(RwLock::new("REAL".to_string())),
            orders: Arc::new(RwLock::new(HashMap::new())),
            contracts: Arc::new(RwLock::new(HashMap::new())),
            ticks: Arc::new(RwLock::new(HashMap::new())),
            positions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

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
        self.load_settings().unwrap_or_else(|| Self::default_setting())
    }

    async fn write_log(&self, msg: &str) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.write_log(msg);
        }
        info!("{}: {}", self.gateway_name, msg);
    }

    async fn on_tick(&self, tick: TickData) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_tick(tick);
        }
    }

    async fn on_order(&self, order: OrderData) {
        self.orders.write().await.insert(order.orderid.clone(), order.clone());
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_order(order);
        }
    }

    async fn on_trade(&self, trade: TradeData) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_trade(trade);
        }
    }

    async fn on_account(&self, account: AccountData) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_account(account);
        }
    }

    async fn on_position(&self, position: PositionData) {
        let key = format!("{}_{}", position.symbol, position.direction);
        self.positions.write().await.insert(key, position.clone());
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_position(position);
        }
    }

    async fn on_contract(&self, contract: ContractData) {
        self.contracts.write().await.insert(contract.symbol.clone(), contract.clone());
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.on_contract(contract);
        }
    }

    fn new_order_id(&self) -> String {
        let count = self.order_count.fetch_add(1, Ordering::SeqCst);
        let connect_time = self.connect_time.load(Ordering::SeqCst);
        format!("{}", connect_time + count as i64)
    }

    async fn query_time(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/fapi/v1/time", &params, Security::None).await?;
        let server_time = data["serverTime"].as_i64().unwrap_or(0);
        let local_time = Utc::now().timestamp_millis();
        let offset = local_time - server_time;
        self.rest_client.set_time_offset(offset);
        self.write_log(&format!("时间同步成功，偏移: {}ms", offset)).await;
        Ok(())
    }

    async fn query_account_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/fapi/v2/account", &params, Security::Signed).await?;

        if let Some(assets) = data["assets"].as_array() {
            for asset in assets {
                let wallet_balance: f64 = asset["walletBalance"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                if wallet_balance > 0.0 {
                    let account = AccountData {
                        accountid: asset["asset"].as_str().unwrap_or("").to_string(),
                        balance: wallet_balance,
                        frozen: 0.0,
                        gateway_name: self.gateway_name.clone(),
                        extra: None,
                    };
                    self.on_account(account).await;
                }
            }
        }

        if let Some(positions) = data["positions"].as_array() {
            for pos in positions {
                let pos_amt: f64 = pos["positionAmt"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                if pos_amt.abs() > 0.0 {
                    let direction = if pos_amt > 0.0 { Direction::Long } else { Direction::Short };
                    let position = PositionData {
                        symbol: pos["symbol"].as_str().unwrap_or("").to_lowercase(),
                        exchange: Exchange::Binance,
                        direction,
                        volume: pos_amt.abs(),
                        frozen: 0.0,
                        price: pos["entryPrice"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        pnl: pos["unrealizedProfit"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        yd_volume: 0.0,
                        gateway_name: self.gateway_name.clone(),
                        extra: None,
                    };
                    self.on_position(position).await;
                }
            }
        }

        self.write_log("账户资金查询成功").await;
        Ok(())
    }

    async fn query_order_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/fapi/v1/openOrders", &params, Security::Signed).await?;

        if let Some(orders) = data.as_array() {
            for d in orders {
                let order_type_str = d["type"].as_str().unwrap_or("");
                let order_type = match ORDERTYPE_BINANCE2VT.get(order_type_str) {
                    Some(t) => *t,
                    None => continue,
                };

                let status_str = d["status"].as_str().unwrap_or("");
                let direction_str = d["side"].as_str().unwrap_or("");

                let order = OrderData {
                    symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                    exchange: Exchange::Binance,
                    orderid: d["clientOrderId"].as_str().unwrap_or("").to_string(),
                    order_type,
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

    async fn query_position_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/fapi/v2/positionRisk", &params, Security::Signed).await?;

        if let Some(positions) = data.as_array() {
            for pos in positions {
                let pos_amt: f64 = pos["positionAmt"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                if pos_amt.abs() > 0.0 {
                    let direction = if pos_amt > 0.0 { Direction::Long } else { Direction::Short };
                    let position = PositionData {
                        symbol: pos["symbol"].as_str().unwrap_or("").to_lowercase(),
                        exchange: Exchange::Binance,
                        direction,
                        volume: pos_amt.abs(),
                        frozen: 0.0,
                        price: pos["entryPrice"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        pnl: pos["unRealizedProfit"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        yd_volume: 0.0,
                        gateway_name: self.gateway_name.clone(),
                        extra: None,
                    };
                    self.on_position(position).await;
                }
            }
        }

        self.write_log("持仓信息查询成功").await;
        Ok(())
    }

    async fn query_contract_impl(&self) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.get("/fapi/v1/exchangeInfo", &params, Security::None).await?;

        if let Some(symbols) = data["symbols"].as_array() {
            for d in symbols {
                let base_asset = d["baseAsset"].as_str().unwrap_or("");
                let quote_asset = d["quoteAsset"].as_str().unwrap_or("");
                let name = format!("{}/{}", base_asset.to_uppercase(), quote_asset.to_uppercase());

                let mut pricetick: f64 = 1.0;
                let mut min_volume: f64 = 1.0;

                if let Some(filters) = d["filters"].as_array() {
                    for f in filters {
                        match f["filterType"].as_str().unwrap_or("") {
                            "PRICE_FILTER" => pricetick = f["tickSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0),
                            "LOT_SIZE" => min_volume = f["stepSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0),
                            _ => {}
                        }
                    }
                }

                let contract = ContractData {
                    symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                    exchange: Exchange::Binance,
                    name,
                    product: Product::Futures,
                    size: 1.0,
                    pricetick,
                    min_volume,
                    max_volume: None,
                    stop_supported: true,
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

    async fn start_user_stream(&self, proxy_host: &str, proxy_port: u16) -> Result<(), String> {
        let params = HashMap::new();
        let data = self.rest_client.post("/fapi/v1/listenKey", &params, Security::ApiKey).await?;

        let listen_key = data["listenKey"].as_str().unwrap_or("").to_string();
        *self.listen_key.write().await = listen_key.clone();
        self.keep_alive_count.store(0, Ordering::SeqCst);

        let server = self.server.read().await.clone();
        let url = if server == "REAL" {
            format!("{}{}", USDT_WS_TRADE_HOST, listen_key)
        } else {
            format!("{}{}", USDT_TESTNET_WS_TRADE_HOST, listen_key)
        };

        let event_sender = self.event_sender.clone();
        let orders = self.orders.clone();
        let positions = self.positions.clone();
        let gateway_name = self.gateway_name.clone();

        let handler: WsMessageHandler = Arc::new(move |packet| {
            let event_sender = event_sender.clone();
            let orders = orders.clone();
            let positions = positions.clone();
            let gateway_name = gateway_name.clone();

            tokio::spawn(async move {
                let event_type = packet.get("e").and_then(|s| s.as_str()).unwrap_or("");

                match event_type {
                    "ACCOUNT_UPDATE" => {
                        if let Some(account_data) = packet.get("a") {
                            if let Some(balances) = account_data.get("B").and_then(|b| b.as_array()) {
                                for b in balances {
                                    let balance: f64 = b["wb"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                                    if balance > 0.0 {
                                        let account = AccountData {
                                            accountid: b["a"].as_str().unwrap_or("").to_string(),
                                            balance,
                                            frozen: 0.0,
                                            gateway_name: gateway_name.clone(),
                                            extra: None,
                                        };
                                        if let Some(sender) = event_sender.read().await.as_ref() {
                                            sender.on_account(account);
                                        }
                                    }
                                }
                            }

                            if let Some(pos_updates) = account_data.get("P").and_then(|p| p.as_array()) {
                                for pos in pos_updates {
                                    let pos_amt: f64 = pos["pa"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                                    let direction = if pos_amt > 0.0 {
                                        Direction::Long
                                    } else if pos_amt < 0.0 {
                                        Direction::Short
                                    } else {
                                        Direction::Net
                                    };

                                    let position = PositionData {
                                        symbol: pos["s"].as_str().unwrap_or("").to_lowercase(),
                                        exchange: Exchange::Binance,
                                        direction,
                                        volume: pos_amt.abs(),
                                        frozen: 0.0,
                                        price: pos["ep"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                        pnl: pos["up"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                        yd_volume: 0.0,
                                        gateway_name: gateway_name.clone(),
                                        extra: None,
                                    };

                                    let key = format!("{}_{}", position.symbol, position.direction);
                                    positions.write().await.insert(key, position.clone());
                                    if let Some(sender) = event_sender.read().await.as_ref() {
                                        sender.on_position(position);
                                    }
                                }
                            }
                        }
                    }
                    "ORDER_TRADE_UPDATE" => {
                        let order_data = match packet.get("o") {
                            Some(d) => d,
                            None => return,
                        };

                        let order_type_str = order_data["o"].as_str().unwrap_or("");
                        let order_type = match ORDERTYPE_BINANCE2VT.get(order_type_str) {
                            Some(t) => *t,
                            None => return,
                        };

                        let orderid = if order_data["C"].as_str().unwrap_or("").is_empty() {
                            order_data["c"].as_str().unwrap_or("").to_string()
                        } else {
                            order_data["C"].as_str().unwrap_or("").to_string()
                        };

                        let status_str = order_data["X"].as_str().unwrap_or("");
                        let direction_str = order_data["S"].as_str().unwrap_or("");

                        let order = OrderData {
                            symbol: order_data["s"].as_str().unwrap_or("").to_lowercase(),
                            exchange: Exchange::Binance,
                            orderid: orderid.clone(),
                            order_type,
                            direction: DIRECTION_BINANCE2VT.get(direction_str).copied(),
                            offset: Offset::None,
                            price: order_data["p"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                            volume: order_data["q"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                            traded: order_data["z"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                            status: STATUS_BINANCE2VT.get(status_str).copied().unwrap_or(Status::Submitting),
                            datetime: Some(timestamp_to_datetime(order_data["T"].as_i64().unwrap_or(0))),
                            reference: String::new(),
                            gateway_name: gateway_name.clone(),
                            extra: None,
                        };

                        orders.write().await.insert(orderid.clone(), order.clone());
                        if let Some(sender) = event_sender.read().await.as_ref() {
                            sender.on_order(order.clone());
                        }

                        let trade_volume: f64 = order_data["l"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        if trade_volume > 0.0 {
                            let trade = TradeData {
                                symbol: order.symbol.clone(),
                                exchange: Exchange::Binance,
                                orderid: orderid,
                                tradeid: order_data["t"].as_i64().unwrap_or(0).to_string(),
                                direction: order.direction,
                                offset: Offset::None,
                                price: order_data["L"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                volume: trade_volume,
                                datetime: Some(timestamp_to_datetime(order_data["T"].as_i64().unwrap_or(0))),
                                gateway_name: gateway_name.clone(),
                                extra: None,
                            };
                            if let Some(sender) = event_sender.read().await.as_ref() {
                                sender.on_trade(trade);
                            }
                        }
                    }
                    "listenKeyExpired" => {
                        warn!("{}: Listen key expired", gateway_name);
                    }
                    _ => {}
                }
            });
        });

        self.trade_ws.set_handler(handler).await;
        self.trade_ws.connect(&url, proxy_host, proxy_port).await?;
        self.write_log("交易Websocket API连接成功").await;
        Ok(())
    }

    pub async fn keep_user_stream(&self) {
        let count = self.keep_alive_count.fetch_add(1, Ordering::SeqCst);
        if count < 1800 { return; }
        self.keep_alive_count.store(0, Ordering::SeqCst);

        let listen_key = self.listen_key.read().await.clone();
        let mut params = HashMap::new();
        params.insert("listenKey".to_string(), listen_key);

        if let Err(e) = self.rest_client.put("/fapi/v1/listenKey", &params, Security::ApiKey).await {
            warn!("{}: Keep user stream failed: {}", self.gateway_name, e);
        }
    }
}

#[async_trait]
impl BaseGateway for BinanceUsdtGateway {
    fn gateway_name(&self) -> &str { &self.gateway_name }
    fn default_name() -> &'static str { "BINANCE_USDT" }

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

    fn exchanges() -> Vec<Exchange> { vec![Exchange::Binance] }

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

        let host = if server == "REAL" { USDT_REST_HOST } else { USDT_TESTNET_REST_HOST };
        self.rest_client.init(&key, &secret, host, &proxy_host, proxy_port).await;
        self.write_log("REST API启动成功").await;

        self.query_time().await?;
        self.query_account_impl().await?;
        self.query_position_impl().await?;
        self.query_order_impl().await?;
        self.query_contract_impl().await?;
        self.start_user_stream(&proxy_host, proxy_port).await?;

        let market_url = if server == "REAL" { USDT_WS_DATA_HOST } else { USDT_TESTNET_WS_DATA_HOST };
        let ticks = self.ticks.clone();
        let event_sender = self.event_sender.clone();

        let handler: WsMessageHandler = Arc::new(move |packet| {
            let ticks = ticks.clone();
            let event_sender = event_sender.clone();

            tokio::spawn(async move {
                let stream = match packet.get("stream").and_then(|s| s.as_str()) { Some(s) => s, None => return };
                let data = match packet.get("data") { Some(d) => d, None => return };

                let parts: Vec<&str> = stream.split('@').collect();
                if parts.len() != 2 { return; }

                let symbol = parts[0];
                let channel = parts[1];

                let mut ticks_guard = ticks.write().await;
                let tick = match ticks_guard.get_mut(symbol) { Some(t) => t, None => return };

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
                    s if s.starts_with("depth5") => {
                        if let Some(bids) = data["b"].as_array() {
                            for (i, bid) in bids.iter().take(5).enumerate() {
                                if let Some(arr) = bid.as_array() {
                                    let price: f64 = arr.get(0).and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0.0);
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
                                    let price: f64 = arr.get(0).and_then(|v| v.as_str()).unwrap_or("0").parse().unwrap_or(0.0);
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
        if !self.contracts.read().await.contains_key(&symbol) {
            return Err(format!("找不到该合约代码: {}", symbol));
        }
        if self.ticks.read().await.contains_key(&symbol) { return Ok(()); }

        let tick = TickData::new(self.gateway_name.clone(), symbol.clone(), Exchange::Binance, Utc::now());
        self.ticks.write().await.insert(symbol.clone(), tick);

        let channels = vec![format!("{}@ticker", symbol), format!("{}@depth5@100ms", symbol)];
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
        params.insert("quantity".to_string(), format!("{}", req.volume));
        params.insert("newClientOrderId".to_string(), orderid.clone());
        params.insert("newOrderRespType".to_string(), "ACK".to_string());

        if req.order_type == OrderType::Limit {
            params.insert("timeInForce".to_string(), "GTC".to_string());
            params.insert("price".to_string(), format!("{}", req.price));
        }

        match self.rest_client.post("/fapi/v1/order", &params, Security::Signed).await {
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

        match self.rest_client.delete("/fapi/v1/order", &params, Security::Signed).await {
            Ok(_) => { self.write_log(&format!("撤单成功: {}", req.orderid)).await; Ok(()) }
            Err(e) => { self.write_log(&format!("撤单失败: {}", e)).await; Err(e) }
        }
    }

    async fn query_account(&self) -> Result<(), String> { self.query_account_impl().await }
    async fn query_position(&self) -> Result<(), String> { self.query_position_impl().await }

    async fn query_history(&self, req: HistoryRequest) -> Result<Vec<BarData>, String> {
        let mut history = Vec::new();
        let limit = 1500;
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

            let data = self.rest_client.get("/fapi/v1/klines", &params, Security::None).await?;
            let rows = match data.as_array() { Some(r) if !r.is_empty() => r, _ => break };

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
