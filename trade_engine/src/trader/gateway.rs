//! Abstract gateway class for creating gateway connections to different trading systems.

use async_trait::async_trait;
use std::collections::HashMap;

use tokio::sync::mpsc;

use super::constant::Exchange;
use super::event::*;
use super::object::{
    AccountData, BarData, CancelRequest, ContractData, HistoryRequest, LogData,
    OrderData, OrderRequest, PositionData, QuoteData, QuoteRequest, SubscribeRequest,
    TickData, TradeData,
};

/// Event data that can be sent from gateway
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    Tick(TickData),
    Trade(TradeData),
    Order(OrderData),
    Position(PositionData),
    Account(AccountData),
    Quote(QuoteData),
    Contract(ContractData),
    Log(LogData),
}

/// Gateway setting value types
#[derive(Debug, Clone)]
pub enum GatewaySettingValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl From<String> for GatewaySettingValue {
    fn from(s: String) -> Self {
        GatewaySettingValue::String(s)
    }
}

impl From<&str> for GatewaySettingValue {
    fn from(s: &str) -> Self {
        GatewaySettingValue::String(s.to_string())
    }
}

impl From<i64> for GatewaySettingValue {
    fn from(i: i64) -> Self {
        GatewaySettingValue::Int(i)
    }
}

impl From<f64> for GatewaySettingValue {
    fn from(f: f64) -> Self {
        GatewaySettingValue::Float(f)
    }
}

impl From<bool> for GatewaySettingValue {
    fn from(b: bool) -> Self {
        GatewaySettingValue::Bool(b)
    }
}

/// Gateway settings type
pub type GatewaySettings = HashMap<String, GatewaySettingValue>;

/// Abstract gateway trait for creating gateway connections to different trading systems.
///
/// # How to implement a gateway:
///
/// ## Basics
/// A gateway should satisfy:
/// - This trait should be thread-safe
/// - All methods should be non-blocking
/// - Satisfies all requirements written in docstrings
/// - Automatically reconnect if connection lost
///
/// ## Callbacks that must respond manually:
/// - on_tick
/// - on_trade  
/// - on_order
/// - on_position
/// - on_account
/// - on_contract
///
/// All data passed to callbacks should be constant (immutable).
#[async_trait]
pub trait BaseGateway: Send + Sync {
    /// Get the gateway name
    fn gateway_name(&self) -> &str;

    /// Get the default name for this gateway type
    fn default_name() -> &'static str
    where
        Self: Sized;

    /// Get the default settings for connection
    fn default_setting() -> GatewaySettings
    where
        Self: Sized;

    /// Get exchanges supported by this gateway
    fn exchanges() -> Vec<Exchange>
    where
        Self: Sized;

    /// Start gateway connection.
    ///
    /// Implementation must:
    /// - Connect to server if necessary
    /// - Log connected if all necessary connections established
    /// - Query and respond with:
    ///   - contracts: on_contract
    ///   - account asset: on_account
    ///   - account holding: on_position
    ///   - orders of account: on_order
    ///   - trades of account: on_trade
    /// - Write log if any query fails
    async fn connect(&self, setting: GatewaySettings) -> Result<(), String>;

    /// Close gateway connection.
    async fn close(&self);

    /// Subscribe tick data update.
    async fn subscribe(&self, req: SubscribeRequest) -> Result<(), String>;

    /// Send a new order to server.
    ///
    /// Implementation should:
    /// - Create an OrderData from req using OrderRequest.create_order_data
    /// - Assign a unique (gateway instance scope) id to OrderData.orderid
    /// - Send request to server
    ///   - If request is sent, OrderData.status should be set to Status::Submitting
    ///   - If request failed, OrderData.status should be set to Status::Rejected
    /// - Response on_order
    /// - Return vt_orderid
    async fn send_order(&self, req: OrderRequest) -> Result<String, String>;

    /// Cancel an existing order.
    async fn cancel_order(&self, req: CancelRequest) -> Result<(), String>;

    /// Send a new two-sided quote to server.
    async fn send_quote(&self, _req: QuoteRequest) -> Result<String, String> {
        Ok(String::new())
    }

    /// Cancel an existing quote.
    async fn cancel_quote(&self, _req: CancelRequest) -> Result<(), String> {
        Ok(())
    }

    /// Query account balance.
    async fn query_account(&self) -> Result<(), String>;

    /// Query holding positions.
    async fn query_position(&self) -> Result<(), String>;

    /// Query bar history data.
    async fn query_history(&self, _req: HistoryRequest) -> Result<Vec<BarData>, String> {
        Ok(Vec::new())
    }

    /// Get default setting dict
    fn get_default_setting(&self) -> GatewaySettings
    where
        Self: Sized,
    {
        Self::default_setting()
    }
}

/// Gateway event sender for pushing events
pub struct GatewayEventSender {
    gateway_name: String,
    sender: mpsc::UnboundedSender<(String, GatewayEvent)>,
}

impl GatewayEventSender {
    /// Create a new event sender
    pub fn new(gateway_name: String, sender: mpsc::UnboundedSender<(String, GatewayEvent)>) -> Self {
        Self {
            gateway_name,
            sender,
        }
    }

    /// Push a tick event
    pub fn on_tick(&self, tick: TickData) {
        let event_type = format!("{}{}", EVENT_TICK, tick.vt_symbol());
        let _ = self.sender.send((event_type, GatewayEvent::Tick(tick.clone())));
        let _ = self.sender.send((EVENT_TICK.to_string(), GatewayEvent::Tick(tick)));
    }

    /// Push a trade event
    pub fn on_trade(&self, trade: TradeData) {
        let event_type = format!("{}{}", EVENT_TRADE, trade.vt_symbol());
        let _ = self.sender.send((event_type, GatewayEvent::Trade(trade.clone())));
        let _ = self.sender.send((EVENT_TRADE.to_string(), GatewayEvent::Trade(trade)));
    }

    /// Push an order event
    pub fn on_order(&self, order: OrderData) {
        let event_type = format!("{}{}", EVENT_ORDER, order.vt_orderid());
        let _ = self.sender.send((event_type, GatewayEvent::Order(order.clone())));
        let _ = self.sender.send((EVENT_ORDER.to_string(), GatewayEvent::Order(order)));
    }

    /// Push a position event
    pub fn on_position(&self, position: PositionData) {
        let event_type = format!("{}{}", EVENT_POSITION, position.vt_symbol());
        let _ = self.sender.send((event_type, GatewayEvent::Position(position.clone())));
        let _ = self.sender.send((EVENT_POSITION.to_string(), GatewayEvent::Position(position)));
    }

    /// Push an account event
    pub fn on_account(&self, account: AccountData) {
        let event_type = format!("{}{}", EVENT_ACCOUNT, account.vt_accountid());
        let _ = self.sender.send((event_type, GatewayEvent::Account(account.clone())));
        let _ = self.sender.send((EVENT_ACCOUNT.to_string(), GatewayEvent::Account(account)));
    }

    /// Push a quote event
    pub fn on_quote(&self, quote: QuoteData) {
        let event_type = format!("{}{}", EVENT_QUOTE, quote.vt_symbol());
        let _ = self.sender.send((event_type, GatewayEvent::Quote(quote.clone())));
        let _ = self.sender.send((EVENT_QUOTE.to_string(), GatewayEvent::Quote(quote)));
    }

    /// Push a log event
    pub fn on_log(&self, log: LogData) {
        let _ = self.sender.send((EVENT_LOG.to_string(), GatewayEvent::Log(log)));
    }

    /// Push a contract event
    pub fn on_contract(&self, contract: ContractData) {
        let _ = self.sender.send((EVENT_CONTRACT.to_string(), GatewayEvent::Contract(contract)));
    }

    /// Write a log message from gateway
    pub fn write_log(&self, msg: impl Into<String>) {
        let log = LogData::new(self.gateway_name.clone(), msg.into());
        self.on_log(log);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_setting_value() {
        let s: GatewaySettingValue = "test".into();
        assert!(matches!(s, GatewaySettingValue::String(_)));

        let i: GatewaySettingValue = 42i64.into();
        assert!(matches!(i, GatewaySettingValue::Int(42)));

        let f: GatewaySettingValue = 3.14f64.into();
        assert!(matches!(f, GatewaySettingValue::Float(_)));

        let b: GatewaySettingValue = true.into();
        assert!(matches!(b, GatewaySettingValue::Bool(true)));
    }
}
