//! Basic data structures used for general trading function in the trading platform.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use super::constant::{Direction, Exchange, Interval, Offset, OptionType, OrderType, Product, Status};

/// Log level constants
pub const DEBUG: i32 = 10;
pub const INFO: i32 = 20;
pub const WARNING: i32 = 30;
pub const ERROR: i32 = 40;
pub const CRITICAL: i32 = 50;

/// Active order statuses
pub fn is_active_status(status: Status) -> bool {
    matches!(status, Status::Submitting | Status::NotTraded | Status::PartTraded)
}

/// Base trait for all data objects
pub trait BaseData {
    /// Get the gateway name
    fn gateway_name(&self) -> &str;
    
    /// Get extra data
    fn extra(&self) -> Option<&HashMap<String, String>>;
    
    /// Set extra data
    fn set_extra(&mut self, extra: HashMap<String, String>);
}

/// Tick data contains information about:
/// - last trade in market
/// - orderbook snapshot
/// - intraday market statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub datetime: DateTime<Utc>,

    pub name: String,
    pub volume: f64,
    pub turnover: f64,
    pub open_interest: f64,
    pub last_price: f64,
    pub last_volume: f64,
    pub limit_up: f64,
    pub limit_down: f64,

    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub pre_close: f64,

    pub bid_price_1: f64,
    pub bid_price_2: f64,
    pub bid_price_3: f64,
    pub bid_price_4: f64,
    pub bid_price_5: f64,

    pub ask_price_1: f64,
    pub ask_price_2: f64,
    pub ask_price_3: f64,
    pub ask_price_4: f64,
    pub ask_price_5: f64,

    pub bid_volume_1: f64,
    pub bid_volume_2: f64,
    pub bid_volume_3: f64,
    pub bid_volume_4: f64,
    pub bid_volume_5: f64,

    pub ask_volume_1: f64,
    pub ask_volume_2: f64,
    pub ask_volume_3: f64,
    pub ask_volume_4: f64,
    pub ask_volume_5: f64,

    pub localtime: Option<DateTime<Utc>>,
    
    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl TickData {
    /// Create a new TickData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        datetime: DateTime<Utc>,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            datetime,
            name: String::new(),
            volume: 0.0,
            turnover: 0.0,
            open_interest: 0.0,
            last_price: 0.0,
            last_volume: 0.0,
            limit_up: 0.0,
            limit_down: 0.0,
            open_price: 0.0,
            high_price: 0.0,
            low_price: 0.0,
            pre_close: 0.0,
            bid_price_1: 0.0,
            bid_price_2: 0.0,
            bid_price_3: 0.0,
            bid_price_4: 0.0,
            bid_price_5: 0.0,
            ask_price_1: 0.0,
            ask_price_2: 0.0,
            ask_price_3: 0.0,
            ask_price_4: 0.0,
            ask_price_5: 0.0,
            bid_volume_1: 0.0,
            bid_volume_2: 0.0,
            bid_volume_3: 0.0,
            bid_volume_4: 0.0,
            bid_volume_5: 0.0,
            ask_volume_1: 0.0,
            ask_volume_2: 0.0,
            ask_volume_3: 0.0,
            ask_volume_4: 0.0,
            ask_volume_5: 0.0,
            localtime: None,
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

impl BaseData for TickData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Depth data contains a full order book snapshot with multiple price levels.
/// Unlike TickData's fixed 5-level depth, DepthData supports variable-depth
/// order books using BTreeMap for efficient sorted access and updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub datetime: DateTime<Utc>,

    /// Bid side: price -> volume, sorted descending by price
    pub bids: BTreeMap<Decimal, Decimal>,
    /// Ask side: price -> volume, sorted ascending by price
    pub asks: BTreeMap<Decimal, Decimal>,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl DepthData {
    /// Create a new DepthData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        datetime: DateTime<Utc>,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            datetime,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Create DepthData from a TickData's 5-level depth
    pub fn from_tick(tick: &TickData) -> Self {
        let mut depth = Self::new(
            tick.gateway_name.clone(),
            tick.symbol.clone(),
            tick.exchange,
            tick.datetime,
        );

        // Insert bid levels
        let bid_prices = [
            tick.bid_price_1, tick.bid_price_2, tick.bid_price_3,
            tick.bid_price_4, tick.bid_price_5,
        ];
        let bid_volumes = [
            tick.bid_volume_1, tick.bid_volume_2, tick.bid_volume_3,
            tick.bid_volume_4, tick.bid_volume_5,
        ];
        let ask_prices = [
            tick.ask_price_1, tick.ask_price_2, tick.ask_price_3,
            tick.ask_price_4, tick.ask_price_5,
        ];
        let ask_volumes = [
            tick.ask_volume_1, tick.ask_volume_2, tick.ask_volume_3,
            tick.ask_volume_4, tick.ask_volume_5,
        ];

        for i in 0..5 {
            if bid_prices[i] > 0.0 && bid_volumes[i] > 0.0 {
                if let (Some(price), Some(vol)) = (
                    Decimal::from_f64_retain(bid_prices[i]),
                    Decimal::from_f64_retain(bid_volumes[i]),
                ) {
                    depth.bids.insert(price, vol);
                }
            }
            if ask_prices[i] > 0.0 && ask_volumes[i] > 0.0 {
                if let (Some(price), Some(vol)) = (
                    Decimal::from_f64_retain(ask_prices[i]),
                    Decimal::from_f64_retain(ask_volumes[i]),
                ) {
                    depth.asks.insert(price, vol);
                }
            }
        }

        depth
    }

    /// Get best bid price
    pub fn best_bid_price(&self) -> Option<Decimal> {
        self.bids.keys().next_back().copied()
    }

    /// Get best ask price
    pub fn best_ask_price(&self) -> Option<Decimal> {
        self.asks.keys().next().copied()
    }

    /// Get best bid volume
    pub fn best_bid_volume(&self) -> Option<Decimal> {
        self.bids.values().next_back().copied()
    }

    /// Get best ask volume
    pub fn best_ask_volume(&self) -> Option<Decimal> {
        self.asks.values().next().copied()
    }
}

impl BaseData for DepthData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Candlestick bar data of a certain trading period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub datetime: DateTime<Utc>,

    pub interval: Option<Interval>,
    pub volume: f64,
    pub turnover: f64,
    pub open_interest: f64,
    pub open_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub close_price: f64,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl BarData {
    /// Create a new BarData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        datetime: DateTime<Utc>,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            datetime,
            interval: None,
            volume: 0.0,
            turnover: 0.0,
            open_interest: 0.0,
            open_price: 0.0,
            high_price: 0.0,
            low_price: 0.0,
            close_price: 0.0,
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

impl BaseData for BarData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Order data contains information for tracking latest status of a specific order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub orderid: String,

    pub order_type: OrderType,
    pub direction: Option<Direction>,
    pub offset: Offset,
    pub price: f64,
    pub volume: f64,
    pub traded: f64,
    pub status: Status,
    pub datetime: Option<DateTime<Utc>>,
    pub reference: String,
    /// Post-Only: Order will only be added to the order book as a maker order.
    #[serde(default)]
    pub post_only: bool,
    /// Reduce-Only: Order can only reduce an existing position, not open a new one.
    #[serde(default)]
    pub reduce_only: bool,
    /// Expire time for GTD (Good Till Date) orders.
    #[serde(default)]
    pub expire_time: Option<DateTime<Utc>>,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl OrderData {
    /// Create a new OrderData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        orderid: String,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            orderid,
            order_type: OrderType::Limit,
            direction: None,
            offset: Offset::None,
            price: 0.0,
            volume: 0.0,
            traded: 0.0,
            status: Status::Submitting,
            datetime: None,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Get vt_orderid (gateway_name.orderid)
    pub fn vt_orderid(&self) -> String {
        format!("{}.{}", self.gateway_name, self.orderid)
    }

    /// Check if the order is active
    pub fn is_active(&self) -> bool {
        is_active_status(self.status)
    }

    /// Create cancel request object from order
    pub fn create_cancel_request(&self) -> CancelRequest {
        CancelRequest {
            orderid: self.orderid.clone(),
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            gateway_name: String::new(),
        }
    }
}

impl BaseData for OrderData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Trade data contains information of a fill of an order.
/// One order can have several trade fills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub orderid: String,
    pub tradeid: String,
    pub direction: Option<Direction>,

    pub offset: Offset,
    pub price: f64,
    pub volume: f64,
    pub datetime: Option<DateTime<Utc>>,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl TradeData {
    /// Create a new TradeData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        orderid: String,
        tradeid: String,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            orderid,
            tradeid,
            direction: None,
            offset: Offset::None,
            price: 0.0,
            volume: 0.0,
            datetime: None,
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Get vt_orderid (gateway_name.orderid)
    pub fn vt_orderid(&self) -> String {
        format!("{}.{}", self.gateway_name, self.orderid)
    }

    /// Get vt_tradeid (gateway_name.tradeid)
    pub fn vt_tradeid(&self) -> String {
        format!("{}.{}", self.gateway_name, self.tradeid)
    }
}

impl BaseData for TradeData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Position data is used for tracking each individual position holding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,

    pub volume: f64,
    pub frozen: f64,
    pub price: f64,
    pub pnl: f64,
    pub yd_volume: f64,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl PositionData {
    /// Create a new PositionData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        direction: Direction,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            direction,
            volume: 0.0,
            frozen: 0.0,
            price: 0.0,
            pnl: 0.0,
            yd_volume: 0.0,
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Get vt_positionid (gateway_name.vt_symbol.direction)
    pub fn vt_positionid(&self) -> String {
        format!("{}.{}.{}", self.gateway_name, self.vt_symbol(), self.direction)
    }
}

impl BaseData for PositionData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Account data contains information about balance, frozen and available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    pub gateway_name: String,
    pub accountid: String,

    pub balance: f64,
    pub frozen: f64,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl AccountData {
    /// Create a new AccountData
    pub fn new(gateway_name: String, accountid: String) -> Self {
        Self {
            gateway_name,
            accountid,
            balance: 0.0,
            frozen: 0.0,
            extra: None,
        }
    }

    /// Get available balance
    pub fn available(&self) -> f64 {
        self.balance - self.frozen
    }

    /// Get vt_accountid (gateway_name.accountid)
    pub fn vt_accountid(&self) -> String {
        format!("{}.{}", self.gateway_name, self.accountid)
    }
}

impl BaseData for AccountData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Log data is used for recording log messages on GUI or in log files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogData {
    pub gateway_name: String,
    pub msg: String,
    pub level: i32,
    pub time: DateTime<Utc>,
}

impl LogData {
    /// Create a new LogData
    pub fn new(gateway_name: String, msg: String) -> Self {
        Self {
            gateway_name,
            msg,
            level: INFO,
            time: Utc::now(),
        }
    }

    /// Create LogData with specific level
    pub fn with_level(gateway_name: String, msg: String, level: i32) -> Self {
        Self {
            gateway_name,
            msg,
            level,
            time: Utc::now(),
        }
    }
}

/// Contract data contains basic information about each contract traded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub name: String,
    pub product: Product,
    pub size: f64,
    pub pricetick: f64,

    pub min_volume: f64,
    pub max_volume: Option<f64>,
    pub stop_supported: bool,
    pub net_position: bool,
    pub history_data: bool,

    pub option_strike: Option<f64>,
    pub option_underlying: Option<String>,
    pub option_type: Option<OptionType>,
    pub option_listed: Option<DateTime<Utc>>,
    pub option_expiry: Option<DateTime<Utc>>,
    pub option_portfolio: Option<String>,
    pub option_index: Option<String>,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl ContractData {
    /// Create a new ContractData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        name: String,
        product: Product,
        size: f64,
        pricetick: f64,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            name,
            product,
            size,
            pricetick,
            min_volume: 1.0,
            max_volume: None,
            stop_supported: false,
            net_position: false,
            history_data: false,
            option_strike: None,
            option_underlying: None,
            option_type: None,
            option_listed: None,
            option_expiry: None,
            option_portfolio: None,
            option_index: None,
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

impl BaseData for ContractData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Quote data contains information for tracking latest status of a specific quote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteData {
    pub gateway_name: String,
    pub symbol: String,
    pub exchange: Exchange,
    pub quoteid: String,

    pub bid_price: f64,
    pub bid_volume: i64,
    pub ask_price: f64,
    pub ask_volume: i64,
    pub bid_offset: Offset,
    pub ask_offset: Offset,
    pub status: Status,
    pub datetime: Option<DateTime<Utc>>,
    pub reference: String,

    #[serde(skip)]
    pub extra: Option<HashMap<String, String>>,
}

impl QuoteData {
    /// Create a new QuoteData
    pub fn new(
        gateway_name: String,
        symbol: String,
        exchange: Exchange,
        quoteid: String,
    ) -> Self {
        Self {
            gateway_name,
            symbol,
            exchange,
            quoteid,
            bid_price: 0.0,
            bid_volume: 0,
            ask_price: 0.0,
            ask_volume: 0,
            bid_offset: Offset::None,
            ask_offset: Offset::None,
            status: Status::Submitting,
            datetime: None,
            reference: String::new(),
            extra: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Get vt_quoteid (gateway_name.quoteid)
    pub fn vt_quoteid(&self) -> String {
        format!("{}.{}", self.gateway_name, self.quoteid)
    }

    /// Check if the quote is active
    pub fn is_active(&self) -> bool {
        is_active_status(self.status)
    }

    /// Create cancel request object from quote
    pub fn create_cancel_request(&self) -> CancelRequest {
        CancelRequest {
            orderid: self.quoteid.clone(),
            symbol: self.symbol.clone(),
            exchange: self.exchange,
            gateway_name: String::new(),
        }
    }
}

impl BaseData for QuoteData {
    fn gateway_name(&self) -> &str {
        &self.gateway_name
    }

    fn extra(&self) -> Option<&HashMap<String, String>> {
        self.extra.as_ref()
    }

    fn set_extra(&mut self, extra: HashMap<String, String>) {
        self.extra = Some(extra);
    }
}

/// Request sending to specific gateway for subscribing tick data update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequest {
    pub symbol: String,
    pub exchange: Exchange,
}

impl SubscribeRequest {
    /// Create a new SubscribeRequest
    pub fn new(symbol: String, exchange: Exchange) -> Self {
        Self { symbol, exchange }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

/// Request sending to specific gateway for creating a new order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,
    pub order_type: OrderType,
    pub volume: f64,
    pub price: f64,
    pub offset: Offset,
    pub reference: String,
    /// Post-Only: Order will only be added to the order book if it can be placed
    /// as a maker order (no immediate taker fill). Rejected if it would immediately match.
    /// Binance Spot: `newOrderRespType=EXPIRED_TAKER` + `type=LIMIT` + `timeInForce=GTC`
    /// Binance Futures: `timeInForce=GTX`
    #[serde(default)]
    pub post_only: bool,
    /// Reduce-Only: Order can only reduce an existing position, not open a new one.
    /// If the order would increase position size, the excess quantity is cancelled.
    /// Binance Spot: Not supported (field ignored).
    /// Binance Futures: `reduceOnly=true`
    #[serde(default)]
    pub reduce_only: bool,
    /// Expire time for GTD (Good Till Date) orders.
    /// Specifies the date/time when the order should be automatically cancelled if not filled.
    #[serde(default)]
    pub expire_time: Option<DateTime<Utc>>,
    /// Gateway name for routing the order to the correct exchange connection.
    /// When set, callbacks (StopOrder/Emulator/Bracket) use this instead of
    /// looking up the exchange-to-gateway mapping.
    #[serde(default)]
    pub gateway_name: String,
}

impl OrderRequest {
    /// Create a new OrderRequest
    pub fn new(
        symbol: String,
        exchange: Exchange,
        direction: Direction,
        order_type: OrderType,
        volume: f64,
    ) -> Self {
        Self {
            symbol,
            exchange,
            direction,
            order_type,
            volume,
            price: 0.0,
            offset: Offset::None,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Create order data from request
    pub fn create_order_data(&self, orderid: String, gateway_name: String) -> OrderData {
        let mut order = OrderData::new(gateway_name, self.symbol.clone(), self.exchange, orderid);
        order.order_type = self.order_type;
        order.direction = Some(self.direction);
        order.offset = self.offset;
        order.price = self.price;
        order.volume = self.volume;
        order.reference = self.reference.clone();
        order.post_only = self.post_only;
        order.reduce_only = self.reduce_only;
        order.expire_time = self.expire_time;
        order.expire_time = self.expire_time;
        order
    }
}

/// Request sending to specific gateway for canceling an existing order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRequest {
    pub orderid: String,
    pub symbol: String,
    pub exchange: Exchange,
    /// Gateway name for routing the cancel to the correct exchange connection.
    #[serde(default)]
    pub gateway_name: String,
}

impl CancelRequest {
    /// Create a new CancelRequest
    pub fn new(orderid: String, symbol: String, exchange: Exchange) -> Self {
        Self {
            orderid,
            symbol,
            exchange,
            gateway_name: String::new(),
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

/// Request sending to specific gateway for querying history data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    pub interval: Option<Interval>,
}

impl HistoryRequest {
    /// Create a new HistoryRequest
    pub fn new(symbol: String, exchange: Exchange, start: DateTime<Utc>) -> Self {
        Self {
            symbol,
            exchange,
            start,
            end: None,
            interval: None,
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }
}

/// Request sending to specific gateway for creating a new quote.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuoteRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub bid_price: f64,
    pub bid_volume: i64,
    pub ask_price: f64,
    pub ask_volume: i64,
    pub bid_offset: Offset,
    pub ask_offset: Offset,
    pub reference: String,
}

impl QuoteRequest {
    /// Create a new QuoteRequest
    pub fn new(
        symbol: String,
        exchange: Exchange,
        bid_price: f64,
        bid_volume: i64,
        ask_price: f64,
        ask_volume: i64,
    ) -> Self {
        Self {
            symbol,
            exchange,
            bid_price,
            bid_volume,
            ask_price,
            ask_volume,
            bid_offset: Offset::None,
            ask_offset: Offset::None,
            reference: String::new(),
        }
    }

    /// Get vt_symbol (symbol.exchange)
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange.value())
    }

    /// Create quote data from request
    pub fn create_quote_data(&self, quoteid: String, gateway_name: String) -> QuoteData {
        let mut quote = QuoteData::new(gateway_name, self.symbol.clone(), self.exchange, quoteid);
        quote.bid_price = self.bid_price;
        quote.bid_volume = self.bid_volume;
        quote.ask_price = self.ask_price;
        quote.ask_volume = self.ask_volume;
        quote.bid_offset = self.bid_offset;
        quote.ask_offset = self.ask_offset;
        quote.reference = self.reference.clone();
        quote
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_data_vt_symbol() {
        let tick = TickData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        assert_eq!(tick.vt_symbol(), "BTCUSDT.BINANCE");
    }

    #[test]
    fn test_order_data_is_active() {
        let mut order = OrderData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "12345".to_string(),
        );
        
        order.status = Status::Submitting;
        assert!(order.is_active());
        
        order.status = Status::AllTraded;
        assert!(!order.is_active());
    }

    #[test]
    fn test_account_data_available() {
        let mut account = AccountData::new("test".to_string(), "acc1".to_string());
        account.balance = 10000.0;
        account.frozen = 2000.0;
        assert_eq!(account.available(), 8000.0);
    }

    // --- TickData tests ---

    #[test]
    fn test_tick_data_new_initializes_zeros() {
        let tick = TickData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        assert_eq!(tick.volume, 0.0);
        assert_eq!(tick.last_price, 0.0);
        assert_eq!(tick.bid_price_1, 0.0);
        assert_eq!(tick.ask_price_1, 0.0);
        assert_eq!(tick.bid_volume_1, 0.0);
        assert_eq!(tick.ask_volume_1, 0.0);
        assert_eq!(tick.bid_price_5, 0.0);
        assert_eq!(tick.ask_volume_5, 0.0);
        assert!(tick.localtime.is_none());
        assert!(tick.extra.is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_tick_data_serde_roundtrip() {
        let tick = TickData::new(
            "gw".to_string(),
            "ETHUSDT".to_string(),
            Exchange::Okx,
            Utc::now(),
        );
        let json = serde_json::to_string(&tick).unwrap();
        let parsed: TickData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.gateway_name, tick.gateway_name);
        assert_eq!(parsed.symbol, tick.symbol);
        assert_eq!(parsed.exchange, tick.exchange);
        // extra is #[serde(skip)] so it will be None after roundtrip
        assert!(parsed.extra.is_none());
    }

    #[test]
    fn test_tick_data_clone_equal() {
        let tick = TickData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        let cloned = tick.clone();
        assert_eq!(cloned.symbol, tick.symbol);
        assert_eq!(cloned.exchange, tick.exchange);
        assert_eq!(cloned.last_price, tick.last_price);
        assert_eq!(cloned.bid_price_1, tick.bid_price_1);
    }

    // --- BarData tests ---

    #[test]
    fn test_bar_data_vt_symbol() {
        let bar = BarData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::BinanceUsdm,
            Utc::now(),
        );
        assert_eq!(bar.vt_symbol(), "BTCUSDT.BINANCE_USDM");
    }

    #[test]
    fn test_bar_data_new_initializes_ohlcv() {
        let bar = BarData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        assert_eq!(bar.open_price, 0.0);
        assert_eq!(bar.high_price, 0.0);
        assert_eq!(bar.low_price, 0.0);
        assert_eq!(bar.close_price, 0.0);
        assert_eq!(bar.volume, 0.0);
        assert!(bar.interval.is_none());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_bar_data_serde_roundtrip() {
        let bar = BarData::new(
            "gw".to_string(),
            "ETHUSDT".to_string(),
            Exchange::Bybit,
            Utc::now(),
        );
        let json = serde_json::to_string(&bar).unwrap();
        let parsed: BarData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, bar.symbol);
        assert_eq!(parsed.exchange, bar.exchange);
        assert_eq!(parsed.volume, bar.volume);
    }

    // --- OrderData tests ---

    #[test]
    fn test_order_data_vt_orderid() {
        let order = OrderData::new(
            "binance".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "ORD123".to_string(),
        );
        assert_eq!(order.vt_orderid(), "binance.ORD123");
    }

    #[test]
    fn test_order_data_new_defaults() {
        let order = OrderData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "1".to_string(),
        );
        assert_eq!(order.status, Status::Submitting);
        assert_eq!(order.order_type, OrderType::Limit);
        assert_eq!(order.offset, Offset::None);
        assert!(order.direction.is_none());
        assert_eq!(order.price, 0.0);
        assert_eq!(order.volume, 0.0);
        assert_eq!(order.traded, 0.0);
    }

    #[test]
    fn test_order_data_is_active_all_statuses() {
        let mut order = OrderData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "1".to_string(),
        );
        for (status, expected) in [
            (Status::Submitting, true),
            (Status::NotTraded, true),
            (Status::PartTraded, true),
            (Status::AllTraded, false),
            (Status::Cancelled, false),
            (Status::Rejected, false),
        ] {
            order.status = status;
            assert_eq!(order.is_active(), expected, "is_active mismatch for {status:?}");
        }
    }

    #[test]
    fn test_order_data_create_cancel_request() {
        let order = OrderData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "ORD999".to_string(),
        );
        let cancel = order.create_cancel_request();
        assert_eq!(cancel.orderid, "ORD999");
        assert_eq!(cancel.symbol, "BTCUSDT");
        assert_eq!(cancel.exchange, Exchange::Binance);
    }

    #[test]
    fn test_order_request_create_order_data() {
        let req = OrderRequest {
                    symbol: "BTCUSDT".to_string(),
                    exchange: Exchange::Binance,
                    direction: Direction::Long,
                    order_type: OrderType::Market,
                    volume: 1.5,
                    price: 42000.0,
                    offset: Offset::Open,
                    reference: "test_ref".to_string(),
                    post_only: false,
                    reduce_only: false,
                    expire_time: None,
                    gateway_name: String::new(),
                };
        let order = req.create_order_data("OID1".to_string(), "binance".to_string());
        assert_eq!(order.gateway_name, "binance");
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.exchange, Exchange::Binance);
        assert_eq!(order.orderid, "OID1");
        assert_eq!(order.direction, Some(Direction::Long));
        assert_eq!(order.order_type, OrderType::Market);
        assert_eq!(order.price, 42000.0);
        assert_eq!(order.volume, 1.5);
        assert_eq!(order.offset, Offset::Open);
        assert_eq!(order.reference, "test_ref");
        // Default status from OrderData::new
        assert_eq!(order.status, Status::Submitting);
    }

    // --- TradeData tests ---

    #[test]
    fn test_trade_data_vt_tradeid() {
        let trade = TradeData::new(
            "binance".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "ORD1".to_string(),
            "TRD42".to_string(),
        );
        assert_eq!(trade.vt_tradeid(), "binance.TRD42");
    }

    // --- PositionData tests ---

    #[test]
    fn test_position_data_vt_positionid() {
        let pos = PositionData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
        );
        assert_eq!(pos.vt_positionid(), "gw.BTCUSDT.BINANCE.多");
    }

    #[test]
    fn test_position_data_directions() {
        let pos_long = PositionData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
        );
        assert!(pos_long.vt_positionid().ends_with("多"));

        let pos_short = PositionData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Short,
        );
        assert!(pos_short.vt_positionid().ends_with("空"));

        let pos_net = PositionData::new(
            "gw".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Net,
        );
        assert!(pos_net.vt_positionid().ends_with("净"));
    }

    // --- AccountData tests ---

    #[test]
    fn test_account_data_vt_accountid() {
        let account = AccountData::new("gw".to_string(), "SPOT".to_string());
        assert_eq!(account.vt_accountid(), "gw.SPOT");
    }

    #[test]
    fn test_account_data_available_zero() {
        let account = AccountData::new("gw".to_string(), "acc1".to_string());
        assert_eq!(account.available(), 0.0);
    }

    #[test]
    fn test_account_data_available_negative() {
        let mut account = AccountData::new("gw".to_string(), "acc1".to_string());
        account.balance = 100.0;
        account.frozen = 200.0;
        assert_eq!(account.available(), -100.0);
    }

    // --- SubscribeRequest / CancelRequest tests ---

    #[test]
    fn test_subscribe_request_vt_symbol() {
        let req = SubscribeRequest::new("BTCUSDT".to_string(), Exchange::Binance);
        assert_eq!(req.vt_symbol(), "BTCUSDT.BINANCE");
    }

    #[test]
    fn test_cancel_request_new() {
        let req = CancelRequest::new("OID1".to_string(), "BTCUSDT".to_string(), Exchange::Okx);
        assert_eq!(req.orderid, "OID1");
        assert_eq!(req.symbol, "BTCUSDT");
        assert_eq!(req.exchange, Exchange::Okx);
        assert_eq!(req.vt_symbol(), "BTCUSDT.OKX");
    }
}
