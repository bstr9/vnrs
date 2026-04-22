//! Binance USDT-M Futures Gateway implementation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use chrono::{Local, Utc};
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info, warn};

use super::config::{BinanceConfigs, BinanceGatewayConfig};
use super::constants::*;
use super::rest_client::BinanceRestClient;
use super::websocket_client::{BinanceWebSocketClient, WsMessageHandler};

use crate::trader::{
    AccountData, BarData, CancelRequest, ContractData, DepthData, Direction, Exchange,
    GatewayEventSender, GatewaySettings, GatewaySettingValue,
    HistoryRequest, Offset, OrderData, OrderRequest, OrderType,
    PositionData, Product, Status, SubscribeRequest, TickData, TradeData,
};
use crate::trader::gateway::BaseGateway;
use crate::error::GatewayError;

fn format_price(value: f64) -> String {
    if value == 0.0 { return "0".to_string(); }
    let s = format!("{:.8}", value);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.to_string()
}

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
    /// Order submission times for stale order detection
    order_submit_times: Arc<RwLock<HashMap<String, Instant>>>,
    contracts: Arc<RwLock<HashMap<String, ContractData>>>,
    ticks: Arc<RwLock<HashMap<String, TickData>>>,
    positions: Arc<RwLock<HashMap<String, PositionData>>>,
}

impl BinanceUsdtGateway {
    pub fn new(gateway_name: &str) -> Self {
        Self {
            gateway_name: gateway_name.to_string(),
            rest_client: Arc::new(BinanceRestClient::new().unwrap_or_default()),
            market_ws: Arc::new(BinanceWebSocketClient::new(gateway_name)),
            trade_ws: Arc::new(BinanceWebSocketClient::new(gateway_name)),
            event_sender: Arc::new(RwLock::new(None)),
            order_count: AtomicU64::new(1_000_000),
            connect_time: AtomicI64::new(0),
            listen_key: Arc::new(RwLock::new(String::new())),
            keep_alive_count: AtomicU64::new(0),
            server: Arc::new(RwLock::new("REAL".to_string())),
            orders: Arc::new(RwLock::new(HashMap::new())),
            order_submit_times: Arc::new(RwLock::new(HashMap::new())),
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
        self.load_settings().unwrap_or_else(Self::default_setting)
    }

    async fn write_log(&self, msg: &str) {
        if let Some(sender) = self.event_sender.read().await.as_ref() {
            sender.write_log(msg);
        }
        info!("{}: {}", self.gateway_name, msg);
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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
                        exchange: Exchange::BinanceUsdm,
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
                let tif_str = d["timeInForce"].as_str().unwrap_or("");
                let order_type = match ORDERTYPE_BINANCE2VT_FUTURES.get(&(order_type_str, tif_str)) {
                    Some(t) => *t,
                    None => match ORDERTYPE_BINANCE2VT.get(order_type_str) {
                        Some(t) => *t,
                        None => continue,
                    },
                };

                let status_str = d["status"].as_str().unwrap_or("");
                let direction_str = d["side"].as_str().unwrap_or("");

                let order = OrderData {
                    symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                    exchange: Exchange::BinanceUsdm,
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
                    post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
                };
                self.on_order(order).await;
            }
        }

        self.write_log("委托信息查询成功").await;
        Ok(())
    }

    /// Query historical trades for symbols with positions
    /// Fetches up to 3 years of trade history with pagination.
    async fn query_trade_impl(&self) -> Result<(), String> {
        // Binance futures userTrades requires symbol parameter.
        // Collect symbols from cached positions (already queried by query_position_impl).
        let symbols: Vec<String> = {
            let positions = self.positions.read().await;
            let mut set = std::collections::HashSet::new();
            for pos in positions.values() {
                set.insert(pos.symbol.to_uppercase());
            }
            set.into_iter().collect()
        };

        if symbols.is_empty() {
            self.write_log("无持仓，跳过历史成交查询").await;
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

                match self.rest_client.get("/fapi/v1/userTrades", &params, Security::Signed).await {
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
                                    exchange: Exchange::BinanceUsdm,
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
                        exchange: Exchange::BinanceUsdm,
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
                let mut min_notional: f64 = 0.0;

                if let Some(filters) = d["filters"].as_array() {
                    for f in filters {
                        match f["filterType"].as_str().unwrap_or("") {
                            "PRICE_FILTER" => pricetick = f["tickSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0),
                            "LOT_SIZE" => min_volume = f["stepSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0),
                            "MIN_NOTIONAL" => {
                                min_notional = f["notional"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                            }
                            _ => {}
                        }
                    }
                }

                // Build extra HashMap for min_notional
                let mut extra = std::collections::HashMap::new();
                if min_notional > 0.0 {
                    extra.insert("min_notional".to_string(), min_notional.to_string());
                }

                let contract = ContractData {
                    symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                    exchange: Exchange::BinanceUsdm,
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
                    extra: if extra.is_empty() { None } else { Some(extra) },
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
        let order_submit_times = self.order_submit_times.clone();
        let positions = self.positions.clone();
        let gateway_name = self.gateway_name.clone();
        let order_lock = Arc::new(Mutex::new(()));

        let handler: WsMessageHandler = Arc::new(move |packet| {
            let event_sender = event_sender.clone();
            let orders = orders.clone();
            let order_submit_times = order_submit_times.clone();
            let positions = positions.clone();
            let gateway_name = gateway_name.clone();
            let lock = order_lock.clone();

            tokio::spawn(async move {
                let _guard = lock.lock().await;
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
                                        exchange: Exchange::BinanceUsdm,
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
                        let tif_str = order_data["f"].as_str().unwrap_or("");
                        let order_type = match ORDERTYPE_BINANCE2VT_FUTURES.get(&(order_type_str, tif_str)) {
                            Some(t) => *t,
                            None => match ORDERTYPE_BINANCE2VT.get(order_type_str) {
                                Some(t) => *t,
                                None => return,
                            },
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
                            exchange: Exchange::BinanceUsdm,
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
                            post_only: false,
                            reduce_only: false,
                            expire_time: None,
                            extra: None,
                        };

                        orders.write().await.insert(orderid.clone(), order.clone());
                        if order.status != Status::Submitting {
                            order_submit_times.write().await.remove(&orderid);
                        }
                        if let Some(sender) = event_sender.read().await.as_ref() {
                            sender.on_order(order.clone());
                        }

                        let trade_volume: f64 = order_data["l"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                        if trade_volume > 0.0 {
                            let trade = TradeData {
                                symbol: order.symbol.clone(),
                                exchange: Exchange::BinanceUsdm,
                                orderid,
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
                        warn!("{}: Listen key expired, attempting to recreate and reconnect trade stream", gateway_name);
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

    /// Recreate listen key and reconnect trade stream
    pub async fn recreate_listen_key(&self) -> Result<(), String> {
        self.trade_ws.disconnect().await;

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

        let proxy_host = self.rest_client.get_proxy_host().await;
        let proxy_port = self.rest_client.get_proxy_port().await;
        self.trade_ws.connect(&url, &proxy_host, proxy_port).await?;
        self.write_log("ListenKey recreated, trade stream reconnected").await;
        Ok(())
    }
}

#[async_trait]
impl BaseGateway for BinanceUsdtGateway {
    fn gateway_name(&self) -> &str { &self.gateway_name }
    fn default_exchange(&self) -> Exchange { Exchange::BinanceUsdm }
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

    async fn connect(&self, setting: GatewaySettings) -> Result<(), GatewayError> {
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
        self.query_trade_impl().await?;
        self.query_contract_impl().await?;
        self.start_user_stream(&proxy_host, proxy_port).await?;

        let market_url = if server == "REAL" { USDT_WS_DATA_HOST } else { USDT_TESTNET_WS_DATA_HOST };
        let ticks = self.ticks.clone();
        let event_sender = self.event_sender.clone();
        let market_lock = Arc::new(Mutex::new(()));
        let gateway_name = self.gateway_name.clone();

        let handler: WsMessageHandler = Arc::new(move |packet| {
            let ticks = ticks.clone();
            let event_sender = event_sender.clone();
            let lock = market_lock.clone();
            let gateway_name = gateway_name.clone();

            tokio::spawn(async move {
                let _guard = lock.lock().await;
                let stream = match packet.get("stream").and_then(|s| s.as_str()) { Some(s) => s, None => return };
                let data = match packet.get("data") { Some(d) => d, None => return };

                let parts: Vec<&str> = stream.split('@').collect();
                if parts.len() < 2 { return; }

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

                if tick.last_price > 0.0 || tick.bid_price_1 > 0.0 || tick.ask_price_1 > 0.0 {
                    tick.localtime = Some(Utc::now());
                    if let Some(sender) = event_sender.read().await.as_ref() {
                        sender.on_tick(tick.clone());
                    } else {
                        warn!("{}: event_sender为空，跳过tick数据发送", gateway_name);
                    }
                    // Emit depth event from tick's 5-level book
                    let depth = DepthData::from_tick(tick);
                    if !depth.bids.is_empty() || !depth.asks.is_empty() {
                        if let Some(sender) = event_sender.read().await.as_ref() {
                            sender.on_depth(depth);
                        }
                    }
                }
            });
        });

        self.market_ws.set_handler(handler).await;
        self.market_ws.connect(market_url, &proxy_host, proxy_port).await?;
        self.write_log("行情Websocket API连接成功").await;

        // Set up on_disconnect callback for market_ws to auto-reconnect with exponential backoff
        let market_ws_reconnect = self.market_ws.clone();
        let gateway_name_market = self.gateway_name.clone();
        let on_disconnect_market: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let market_ws = market_ws_reconnect.clone();
            let gateway_name = gateway_name_market.clone();
            tokio::spawn(async move {
                warn!("{}: market_ws连接断开，开始自动重连...", gateway_name);

                // Retry loop with exponential backoff
                let mut attempt = 0u32;
                loop {
                    attempt += 1;
                    let delay = BinanceWebSocketClient::calculate_backoff_delay(attempt - 1);
                    info!(
                        "{}: market_ws重连尝试 {}, 等待 {:?}",
                        gateway_name, attempt, delay
                    );
                    tokio::time::sleep(delay).await;

                    match market_ws.reconnect().await {
                        Ok(()) => {
                            info!("{}: market_ws重连成功，重新订阅...", gateway_name);
                            match market_ws.resubscribe().await {
                                Ok(()) => {
                                    info!("{}: market_ws重新订阅成功", gateway_name);
                                    return; // Success — exit retry loop
                                }
                                Err(e) => {
                                    error!("{}: market_ws重新订阅失败: {}, 将继续重试", gateway_name, e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("{}: market_ws重连失败 (尝试 {}): {}", gateway_name, attempt, e);
                        }
                    }

                    if attempt > 10 {
                        warn!("{}: market_ws已尝试{}次，继续尝试...", gateway_name, attempt);
                    }
                }
            });
        });
        self.market_ws.set_on_disconnect(on_disconnect_market).await;

        // Set up on_disconnect callback for trade_ws to auto-reconnect with exponential backoff
        let trade_ws_reconnect = self.trade_ws.clone();
        let rest_client_reconnect = self.rest_client.clone();
        let listen_key_reconnect = self.listen_key.clone();
        let positions_reconnect = self.positions.clone();
        let orders_reconnect = self.orders.clone();
        let event_sender_trade = self.event_sender.clone();
        let gateway_name_trade = self.gateway_name.clone();
        let server_trade = self.server.clone();
        let on_disconnect_trade: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            let trade_ws = trade_ws_reconnect.clone();
            let rest_client = rest_client_reconnect.clone();
            let listen_key_arc = listen_key_reconnect.clone();
            let positions = positions_reconnect.clone();
            let orders = orders_reconnect.clone();
            let event_sender = event_sender_trade.clone();
            let gateway_name = gateway_name_trade.clone();
            let server = server_trade.clone();
            tokio::spawn(async move {
                warn!("{}: trade_ws连接断开，开始自动重连...", gateway_name);

                // Retry loop with exponential backoff
                let mut attempt = 0u32;
                loop {
                    // Disconnect existing
                    trade_ws.disconnect().await;

                    attempt += 1;
                    let delay = BinanceWebSocketClient::calculate_backoff_delay(attempt - 1);
                    info!(
                        "{}: trade_ws重连尝试 {}, 等待 {:?}",
                        gateway_name, attempt, delay
                    );
                    tokio::time::sleep(delay).await;

                    // Create new listen key
                    let params = std::collections::HashMap::new();
                    let data = match rest_client.post("/fapi/v1/listenKey", &params, super::constants::Security::ApiKey).await {
                        Ok(d) => d,
                        Err(e) => {
                            warn!("{}: 创建listenKey失败 (尝试 {}): {}, 将继续重试", gateway_name, attempt, e);
                            continue;
                        }
                    };

                    let new_listen_key = data["listenKey"].as_str().unwrap_or("").to_string();
                    *listen_key_arc.write().await = new_listen_key.clone();

                    // Build URL
                    let server_val = server.read().await.clone();
                    let url = if server_val == "REAL" {
                        format!("{}{}", super::constants::USDT_WS_TRADE_HOST, new_listen_key)
                    } else {
                        format!("{}{}", super::constants::USDT_TESTNET_WS_TRADE_HOST, new_listen_key)
                    };

                    // Get proxy settings
                    let proxy_host = rest_client.get_proxy_host().await;
                    let proxy_port = rest_client.get_proxy_port().await;

                    // Reconnect trade_ws
                    match trade_ws.connect(&url, &proxy_host, proxy_port).await {
                        Ok(()) => {
                            info!("{}: trade_ws重连成功", gateway_name);

                            // Re-query positions
                            match rest_client.get("/fapi/v2/positionRisk", &params, super::constants::Security::Signed).await {
                                Ok(data) => {
                                    if let Some(pos_arr) = data.as_array() {
                                        for pos in pos_arr {
                                            let pos_amt: f64 = pos["positionAmt"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                                            if pos_amt.abs() > 0.0 {
                                                let direction = if pos_amt > 0.0 { crate::trader::Direction::Long } else { crate::trader::Direction::Short };
                                                let position = crate::trader::PositionData {
                                                    symbol: pos["symbol"].as_str().unwrap_or("").to_lowercase(),
                                                    exchange: crate::trader::Exchange::BinanceUsdm,
                                                    direction,
                                                    volume: pos_amt.abs(),
                                                    frozen: 0.0,
                                                    price: pos["entryPrice"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                                    pnl: pos["unRealizedProfit"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
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
                                    info!("{}: 重连后持仓重新查询成功", gateway_name);
                                }
                                Err(e) => {
                                    error!("{}: 重连后持仓查询失败: {}", gateway_name, e);
                                }
                            }

                            // Re-query open orders
                            match rest_client.get("/fapi/v1/openOrders", &params, super::constants::Security::Signed).await {
                                Ok(data) => {
                                    if let Some(orders_arr) = data.as_array() {
                                        for d in orders_arr {
                                            let order_type_str = d["type"].as_str().unwrap_or("");
                                            let tif_str = d["timeInForce"].as_str().unwrap_or("");
                                            let order_type = match super::constants::ORDERTYPE_BINANCE2VT_FUTURES.get(&(order_type_str, tif_str)) {
                                                Some(t) => *t,
                                                None => match super::constants::ORDERTYPE_BINANCE2VT.get(order_type_str) {
                                                    Some(t) => *t,
                                                    None => continue,
                                                },
                                            };

                                            let status_str = d["status"].as_str().unwrap_or("");
                                            let direction_str = d["side"].as_str().unwrap_or("");

                                            let order = crate::trader::OrderData {
                                                symbol: d["symbol"].as_str().unwrap_or("").to_lowercase(),
                                                exchange: crate::trader::Exchange::BinanceUsdm,
                                                orderid: d["clientOrderId"].as_str().unwrap_or("").to_string(),
                                                order_type,
                                                direction: super::constants::DIRECTION_BINANCE2VT.get(direction_str).copied(),
                                                offset: crate::trader::Offset::None,
                                                price: d["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                                volume: d["origQty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                                traded: d["executedQty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                                status: super::constants::STATUS_BINANCE2VT.get(status_str).copied().unwrap_or(crate::trader::Status::Submitting),
                                                datetime: Some(super::constants::timestamp_to_datetime(d["time"].as_i64().unwrap_or(0))),
                                                reference: String::new(),
                                                gateway_name: gateway_name.clone(),
                                                post_only: false,
                                                reduce_only: false,
                                                expire_time: None,
                                                extra: None,
                                            };
                                            orders.write().await.insert(order.orderid.clone(), order.clone());
                                            if let Some(sender) = event_sender.read().await.as_ref() {
                                                sender.on_order(order);
                                            }
                                        }
                                    }
                                    info!("{}: 重连后活跃委托重新查询成功", gateway_name);
                                }
                                Err(e) => {
                                    error!("{}: 重连后活跃委托查询失败: {}", gateway_name, e);
                                }
                            }

                            return; // Successfully reconnected — exit retry loop
                        }
                        Err(e) => {
                            warn!("{}: trade_ws重连失败 (尝试 {}): {}", gateway_name, attempt, e);
                            // Continue loop to try again
                        }
                    }
                }
            });
        });
        self.trade_ws.set_on_disconnect(on_disconnect_trade).await;

        // Spawn background stale order checker
        let orders_checker = self.orders.clone();
        let order_submit_times_checker = self.order_submit_times.clone();
        let rest_client_checker = self.rest_client.clone();
        let event_sender_checker = self.event_sender.clone();
        let gateway_name_checker = self.gateway_name.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;

                // Check for stale orders (Submitting for > 60 seconds)
                let stale_orderids: Vec<String> = {
                    let times = order_submit_times_checker.read().await;
                    let now = Instant::now();
                    times.iter()
                        .filter(|(_, time)| now.duration_since(**time) > std::time::Duration::from_secs(60))
                        .map(|(id, _)| id.clone())
                        .collect()
                };

                for orderid in stale_orderids {
                    let mut params = HashMap::new();
                    params.insert("origClientOrderId".to_string(), orderid.clone());

                    match rest_client_checker.get("/fapi/v1/order", &params, Security::Signed).await {
                        Ok(data) => {
                            let status_str = data["status"].as_str().unwrap_or("");
                            let new_status = STATUS_BINANCE2VT.get(status_str).copied().unwrap_or(Status::Submitting);

                            if new_status != Status::Submitting {
                                let order_type_str = data["type"].as_str().unwrap_or("");
                                let tif_str = data["timeInForce"].as_str().unwrap_or("");
                                let order_type = match ORDERTYPE_BINANCE2VT_FUTURES.get(&(order_type_str, tif_str)) {
                                    Some(t) => *t,
                                    None => match ORDERTYPE_BINANCE2VT.get(order_type_str) {
                                        Some(t) => *t,
                                        None => {
                                            order_submit_times_checker.write().await.remove(&orderid);
                                            continue;
                                        }
                                    }
                                };

                                let direction_str = data["side"].as_str().unwrap_or("");

                                let corrected_order = OrderData {
                                    symbol: data["symbol"].as_str().unwrap_or("").to_lowercase(),
                                    exchange: Exchange::BinanceUsdm,
                                    orderid: orderid.clone(),
                                    order_type,
                                    direction: DIRECTION_BINANCE2VT.get(direction_str).copied(),
                                    offset: Offset::None,
                                    price: data["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                    volume: data["origQty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                    traded: data["executedQty"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                                    status: new_status,
                                    datetime: Some(timestamp_to_datetime(data["time"].as_i64().unwrap_or(0))),
                                    reference: String::new(),
                                    gateway_name: gateway_name_checker.clone(),
                                    post_only: false,
                                    reduce_only: false,
                                    expire_time: None,
                                    extra: None,
                                };

                                // Update local cache
                                orders_checker.write().await.insert(orderid.clone(), corrected_order.clone());
                                // Emit event
                                if let Some(sender) = event_sender_checker.read().await.as_ref() {
                                    sender.on_order(corrected_order);
                                }
                                // Remove from tracking
                                order_submit_times_checker.write().await.remove(&orderid);

                                info!("{}: Stale order {} resolved via REST query, new status: {:?}", gateway_name_checker, orderid, new_status);
                            }
                        }
                        Err(_) => {
                            // Order not found on exchange — mark as cancelled
                            if let Some(order) = orders_checker.read().await.get(&orderid).cloned() {
                                let mut cancelled = order;
                                cancelled.status = Status::Cancelled;
                                orders_checker.write().await.insert(orderid.clone(), cancelled.clone());
                                if let Some(sender) = event_sender_checker.read().await.as_ref() {
                                    sender.on_order(cancelled);
                                }
                            }
                            order_submit_times_checker.write().await.remove(&orderid);
                            warn!("{}: Stale order {} not found on exchange, marking cancelled", gateway_name_checker, orderid);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    async fn close(&self) {
        self.market_ws.disconnect().await;
        self.trade_ws.disconnect().await;
        // Clear event sender so in-flight spawned tasks can no longer send events
        *self.event_sender.write().await = None;
        self.write_log("Gateway已关闭").await;
    }

    async fn subscribe(&self, req: SubscribeRequest) -> Result<(), GatewayError> {
        let symbol = req.symbol.to_lowercase();
        if !self.contracts.read().await.contains_key(&symbol) {
            return Err(format!("找不到该合约代码: {}", symbol).into());
        }
        if self.ticks.read().await.contains_key(&symbol) { return Ok(()); }

        let tick = TickData::new(self.gateway_name.clone(), symbol.clone(), Exchange::BinanceUsdm, Utc::now());
        self.ticks.write().await.insert(symbol.clone(), tick);

        let channels = vec![format!("{}@ticker", symbol), format!("{}@depth5@100ms", symbol)];
        self.market_ws.subscribe(channels).await?;
        self.write_log(&format!("订阅行情: {}", symbol)).await;
        Ok(())
    }

    async fn unsubscribe(&self, req: SubscribeRequest) -> Result<(), GatewayError> {
        let symbol = req.symbol.to_lowercase();

        // Remove from ticks cache
        if self.ticks.write().await.remove(&symbol).is_none() {
            return Ok(()); // Not subscribed, no-op
        }

        // Unsubscribe from WebSocket streams
        let channels = vec![format!("{}@ticker", symbol), format!("{}@depth5@100ms", symbol)];
        self.market_ws.unsubscribe(channels).await?;
        self.write_log(&format!("退订行情: {}", symbol)).await;
        Ok(())
    }

    async fn send_order(&self, req: OrderRequest) -> Result<String, GatewayError> {
        let orderid = self.new_order_id();
        let order = req.create_order_data(orderid.clone(), self.gateway_name.clone());
        self.on_order(order.clone()).await;

        // Track submission time for stale order detection
        self.order_submit_times.write().await.insert(orderid.clone(), Instant::now());

        let mut params = HashMap::new();
        params.insert("symbol".to_string(), req.symbol.to_uppercase());
        params.insert("side".to_string(), DIRECTION_VT2BINANCE.get(&req.direction).unwrap_or(&"BUY").to_string());
        params.insert("quantity".to_string(), format_price(req.volume));
        params.insert("newClientOrderId".to_string(), orderid.clone());
        params.insert("newOrderRespType".to_string(), "ACK".to_string());

        if let Some((order_type_str, time_in_force)) = ORDERTYPE_VT2BINANCE_FUTURES.get(&req.order_type) {
            params.insert("type".to_string(), order_type_str.to_string());
            // Post-Only on Binance Futures: Override timeInForce to GTX (Good-Till-Crossing)
            if req.post_only && *order_type_str == "LIMIT" {
                params.insert("timeInForce".to_string(), "GTX".to_string());
            } else {
                params.insert("timeInForce".to_string(), time_in_force.to_string());
            }
            if *order_type_str == "LIMIT" || *order_type_str == "STOP" {
                params.insert("price".to_string(), format_price(req.price));
            }
        } else {
            params.insert("type".to_string(), "LIMIT".to_string());
            params.insert("timeInForce".to_string(), "GTC".to_string());
            params.insert("price".to_string(), format_price(req.price));
        }

        // Good-Till-Date: add goodTillDate parameter for Gtd orders
        if req.order_type == OrderType::Gtd {
            if let Some(expire_time) = req.expire_time {
                params.insert("goodTillDate".to_string(), expire_time.timestamp_millis().to_string());
            } else {
                self.write_log("GTD订单缺少expire_time，回退为GTC限价单").await;
                params.insert("timeInForce".to_string(), "GTC".to_string());
            }
        }

        // Reduce-Only on Binance Futures: `reduceOnly=true`
        if req.reduce_only {
            params.insert("reduceOnly".to_string(), "true".to_string());
        }

        match self.rest_client.post("/fapi/v1/order", &params, Security::Signed).await {
            Ok(_) => Ok(format!("{}.{}", self.gateway_name, orderid)),
            Err(e) => {
                let mut rejected_order = order;
                rejected_order.status = Status::Rejected;
                self.on_order(rejected_order).await;
                // Remove from tracking on rejection
                self.order_submit_times.write().await.remove(&orderid);
                self.write_log(&format!("委托失败: {}", e)).await;
                Err(e.into())
            }
        }
    }

    async fn cancel_order(&self, req: CancelRequest) -> Result<(), GatewayError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), req.symbol.to_uppercase());
        params.insert("origClientOrderId".to_string(), req.orderid.clone());

        match self.rest_client.delete("/fapi/v1/order", &params, Security::Signed).await {
            Ok(_) => { self.write_log(&format!("撤单成功: {}", req.orderid)).await; Ok(()) }
            Err(e) => { self.write_log(&format!("撤单失败: {}", e)).await; Err(e.into()) }
        }
    }

    async fn query_account(&self) -> Result<(), GatewayError> { self.query_account_impl().await.map_err(Into::into) }
    async fn query_position(&self) -> Result<(), GatewayError> { self.query_position_impl().await.map_err(Into::into) }

    async fn query_history(&self, req: HistoryRequest) -> Result<Vec<BarData>, GatewayError> {
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
                        exchange: Exchange::BinanceUsdm,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Direction, Exchange, Offset, OrderType, Product, Status};

    /// Test ContractData construction from Binance USDT-M Futures exchangeInfo response.
    /// This mirrors the parsing logic in `query_contract_impl`.
    #[test]
    fn test_usdt_contract_data_from_exchange_info() {
        let json = serde_json::json!({
            "symbol": "BTCUSDT",
            "baseAsset": "BTC",
            "quoteAsset": "USDT",
            "filters": [
                { "filterType": "PRICE_FILTER", "tickSize": "0.10" },
                { "filterType": "LOT_SIZE", "stepSize": "0.001" },
                { "filterType": "MIN_NOTIONAL", "notional": "5" }
            ]
        });

        let base_asset = json["baseAsset"].as_str().unwrap_or("");
        let quote_asset = json["quoteAsset"].as_str().unwrap_or("");
        let name = format!("{}/{}", base_asset.to_uppercase(), quote_asset.to_uppercase());

        let mut pricetick: f64 = 1.0;
        let mut min_volume: f64 = 1.0;
        let mut min_notional: f64 = 0.0;

        if let Some(filters) = json["filters"].as_array() {
            for f in filters {
                match f["filterType"].as_str().unwrap_or("") {
                    "PRICE_FILTER" => pricetick = f["tickSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0),
                    "LOT_SIZE" => min_volume = f["stepSize"].as_str().unwrap_or("1").parse().unwrap_or(1.0),
                    "MIN_NOTIONAL" => {
                        min_notional = f["notional"].as_str().unwrap_or("0").parse().unwrap_or(0.0);
                    }
                    _ => {}
                }
            }
        }

        let mut extra = std::collections::HashMap::new();
        if min_notional > 0.0 {
            extra.insert("min_notional".to_string(), min_notional.to_string());
        }

        let contract = ContractData {
            symbol: json["symbol"].as_str().unwrap_or("").to_lowercase(),
            exchange: Exchange::BinanceUsdm,
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
            gateway_name: "BINANCE_USDT".to_string(),
            extra: if extra.is_empty() { None } else { Some(extra) },
        };

        assert_eq!(contract.symbol, "btcusdt");
        assert_eq!(contract.exchange, Exchange::BinanceUsdm);
        assert_eq!(contract.name, "BTC/USDT");
        assert_eq!(contract.product, Product::Futures);
        assert_eq!(contract.size, 1.0);
        assert_eq!(contract.pricetick, 0.1);
        assert_eq!(contract.min_volume, 0.001);
        assert!(contract.stop_supported);
        assert!(contract.net_position);
        assert!(contract.history_data);
        assert!(contract.extra.is_some());
        let extra_map = contract.extra.unwrap();
        assert_eq!(extra_map.get("min_notional").unwrap(), "5");
    }

    /// Test OrderData construction from Binance USDT-M Futures ORDER_TRADE_UPDATE event.
    /// This mirrors the parsing logic in the trade_ws handler for "ORDER_TRADE_UPDATE".
    #[test]
    fn test_usdt_order_data_from_ws_event() {
        let packet = serde_json::json!({
            "e": "ORDER_TRADE_UPDATE",
            "o": {
                "s": "ETHUSDT",
                "c": "260422120000000001",
                "C": "",
                "S": "SELL",
                "o": "LIMIT",
                "X": "PARTIALLY_FILLED",
                "p": "3000.00",
                "q": "0.500",
                "z": "0.200",
                "T": 1672531200000_i64
            }
        });

        let order_data = packet.get("o").unwrap();
        let order_type_str = order_data["o"].as_str().unwrap_or("");
        let order_type = ORDERTYPE_BINANCE2VT.get(order_type_str).copied().unwrap();
        let orderid = if order_data["C"].as_str().unwrap_or("").is_empty() {
            order_data["c"].as_str().unwrap_or("").to_string()
        } else {
            order_data["C"].as_str().unwrap_or("").to_string()
        };
        let status_str = order_data["X"].as_str().unwrap_or("");
        let direction_str = order_data["S"].as_str().unwrap_or("");

        let order = OrderData {
            symbol: order_data["s"].as_str().unwrap_or("").to_lowercase(),
            exchange: Exchange::BinanceUsdm,
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
            gateway_name: "BINANCE_USDT".to_string(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        assert_eq!(order.symbol, "ethusdt");
        assert_eq!(order.exchange, Exchange::BinanceUsdm);
        assert_eq!(order.orderid, "260422120000000001");
        assert_eq!(order.order_type, OrderType::Limit);
        assert_eq!(order.direction, Some(Direction::Short));
        assert_eq!(order.price, 3000.0);
        assert_eq!(order.volume, 0.5);
        assert_eq!(order.traded, 0.2);
        assert_eq!(order.status, Status::PartTraded);
    }

    /// Test that the event type "e" field correctly identifies the message type.
    #[test]
    fn test_usdt_ws_event_type_detection() {
        // ACCOUNT_UPDATE event
        let account_packet = serde_json::json!({"e": "ACCOUNT_UPDATE", "a": {}});
        assert_eq!(
            account_packet.get("e").and_then(|s| s.as_str()).unwrap_or(""),
            "ACCOUNT_UPDATE"
        );

        // ORDER_TRADE_UPDATE event
        let order_packet = serde_json::json!({"e": "ORDER_TRADE_UPDATE", "o": {}});
        assert_eq!(
            order_packet.get("e").and_then(|s| s.as_str()).unwrap_or(""),
            "ORDER_TRADE_UPDATE"
        );

        // listenKeyExpired event
        let expired_packet = serde_json::json!({"e": "listenKeyExpired"});
        assert_eq!(
            expired_packet.get("e").and_then(|s| s.as_str()).unwrap_or(""),
            "listenKeyExpired"
        );

        // Unknown event type
        let unknown_packet = serde_json::json!({"e": "SOME_NEW_EVENT"});
        assert_eq!(
            unknown_packet.get("e").and_then(|s| s.as_str()).unwrap_or(""),
            "SOME_NEW_EVENT"
        );

        // Missing "e" field
        let no_event_packet = serde_json::json!({"result": null});
        assert_eq!(
            no_event_packet.get("e").and_then(|s| s.as_str()).unwrap_or(""),
            ""
        );
    }
}
