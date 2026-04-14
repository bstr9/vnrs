//! Basic data structures used for general trading function in the trading platform.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        order
    }
}

/// Request sending to specific gateway for canceling an existing order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRequest {
    pub orderid: String,
    pub symbol: String,
    pub exchange: Exchange,
}

impl CancelRequest {
    /// Create a new CancelRequest
    pub fn new(orderid: String, symbol: String, exchange: Exchange) -> Self {
        Self {
            orderid,
            symbol,
            exchange,
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
}
