//! Mock Gateway for testing - Records all calls and can be configured with responses

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use trade_engine::trader::{
    AccountData, BaseGateway, CancelRequest, Exchange, GatewayEventSender,
    GatewaySettings, OrderData, OrderRequest, PositionData, Status, SubscribeRequest,
};
use trade_engine::trader::constant::Direction;

/// Record of a gateway call for test assertions
#[derive(Debug, Clone)]
pub enum GatewayCall {
    /// Connect was called
    Connect,
    /// Close was called
    Close,
    /// Subscribe was called with symbol and exchange
    Subscribe { symbol: String, exchange: Exchange },
    /// SendOrder was called with details
    SendOrder {
        symbol: String,
        direction: Direction,
        price: f64,
        volume: f64,
    },
    /// CancelOrder was called with orderid
    CancelOrder { orderid: String },
    /// QueryAccount was called
    QueryAccount,
    /// QueryPosition was called
    QueryPosition,
    /// QueryHistory was called
    QueryHistory { symbol: String },
}

/// Mock gateway that records all calls for testing purposes
///
/// This implementation tracks all method calls and can be configured
/// to return specific results for testing different scenarios.
pub struct MockGateway {
    /// Gateway name
    name: String,
    /// Recorded calls
    calls: Arc<Mutex<Vec<GatewayCall>>>,
    /// Event sender for pushing events
    event_sender: Arc<Mutex<Option<GatewayEventSender>>>,
    /// Configurable connect result
    connect_result: Arc<Mutex<Result<(), String>>>,
    /// Configurable send_order result (returns orderid)
    send_order_result: Arc<Mutex<Result<String, String>>>,
    /// Order counter for generating unique order IDs
    order_counter: Arc<Mutex<u64>>,
}

impl MockGateway {
    /// Create a new MockGateway with the given name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            calls: Arc::new(Mutex::new(Vec::new())),
            event_sender: Arc::new(Mutex::new(None)),
            connect_result: Arc::new(Mutex::new(Ok(()))),
            send_order_result: Arc::new(Mutex::new(Ok("MOCK_ORDER_1".to_string()))),
            order_counter: Arc::new(Mutex::new(1)),
        }
    }

    /// Get all recorded calls
    #[allow(clippy::unwrap_used)]
    pub fn calls(&self) -> Vec<GatewayCall> {
        self.calls.lock().unwrap().clone()
    }

    /// Clear all recorded calls
    #[allow(clippy::unwrap_used)]
    pub fn clear_calls(&self) {
        self.calls.lock().unwrap().clear();
    }

    /// Set the result to return from connect()
    #[allow(clippy::unwrap_used)]
    pub fn set_connect_result(&self, result: Result<(), String>) {
        *self.connect_result.lock().unwrap() = result;
    }

    /// Set the result to return from send_order()
    #[allow(clippy::unwrap_used)]
    pub fn set_send_order_result(&self, result: Result<String, String>) {
        *self.send_order_result.lock().unwrap() = result;
    }

    /// Set the event sender for pushing gateway events
    #[allow(clippy::unwrap_used)]
    pub fn set_event_sender(&self, sender: GatewayEventSender) {
        *self.event_sender.lock().unwrap() = Some(sender);
    }

    /// Check if a specific call was made
    #[allow(clippy::unwrap_used)]
    pub fn was_called(&self, expected: &GatewayCall) -> bool {
        self.calls.lock().unwrap().iter().any(|c| {
            match (c, expected) {
                (GatewayCall::Connect, GatewayCall::Connect) => true,
                (GatewayCall::Close, GatewayCall::Close) => true,
                (
                    GatewayCall::Subscribe { symbol: s1, exchange: e1 },
                    GatewayCall::Subscribe { symbol: s2, exchange: e2 },
                ) => s1 == s2 && e1 == e2,
                (
                    GatewayCall::SendOrder { symbol: s1, direction: d1, price: p1, volume: v1 },
                    GatewayCall::SendOrder { symbol: s2, direction: d2, price: p2, volume: v2 },
                ) => s1 == s2 && d1 == d2 && (p1 - p2).abs() < f64::EPSILON && (v1 - v2).abs() < f64::EPSILON,
                (GatewayCall::CancelOrder { orderid: o1 }, GatewayCall::CancelOrder { orderid: o2 }) => o1 == o2,
                (GatewayCall::QueryAccount, GatewayCall::QueryAccount) => true,
                (GatewayCall::QueryPosition, GatewayCall::QueryPosition) => true,
                (
                    GatewayCall::QueryHistory { symbol: s1 },
                    GatewayCall::QueryHistory { symbol: s2 },
                ) => s1 == s2,
                _ => false,
            }
        })
    }

    /// Count the number of times a specific call type was made
    #[allow(clippy::unwrap_used)]
    pub fn count_calls(&self, call_type: &str) -> usize {
        self.calls.lock().unwrap().iter().filter(|c| {
            match (call_type, c) {
                ("connect", GatewayCall::Connect) => true,
                ("close", GatewayCall::Close) => true,
                ("subscribe", GatewayCall::Subscribe { .. }) => true,
                ("send_order", GatewayCall::SendOrder { .. }) => true,
                ("cancel_order", GatewayCall::CancelOrder { .. }) => true,
                ("query_account", GatewayCall::QueryAccount) => true,
                ("query_position", GatewayCall::QueryPosition) => true,
                ("query_history", GatewayCall::QueryHistory { .. }) => true,
                _ => false,
            }
        }).count()
    }

    /// Generate a unique order ID
    #[allow(clippy::unwrap_used)]
    fn generate_order_id(&self) -> String {
        let mut counter = self.order_counter.lock().unwrap();
        let id = format!("MOCK_ORDER_{}", *counter);
        *counter += 1;
        id
    }
}

#[async_trait]
impl BaseGateway for MockGateway {
    fn gateway_name(&self) -> &str {
        &self.name
    }

    fn default_exchange(&self) -> Exchange {
        Exchange::Binance
    }

    fn default_name() -> &'static str {
        "MOCK"
    }

    fn default_setting() -> GatewaySettings {
        HashMap::new()
    }

    fn exchanges() -> Vec<Exchange> {
        vec![Exchange::Binance, Exchange::BinanceUsdm]
    }

    #[allow(clippy::unwrap_used)]
    async fn connect(&self, _setting: GatewaySettings) -> Result<(), String> {
        self.calls.lock().unwrap().push(GatewayCall::Connect);
        self.connect_result.lock().unwrap().clone()
    }

    #[allow(clippy::unwrap_used)]
    async fn close(&self) {
        self.calls.lock().unwrap().push(GatewayCall::Close);
    }

    #[allow(clippy::unwrap_used)]
    async fn subscribe(&self, req: SubscribeRequest) -> Result<(), String> {
        self.calls.lock().unwrap().push(GatewayCall::Subscribe {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
        });
        Ok(())
    }

    #[allow(clippy::unwrap_used)]
    async fn send_order(&self, req: OrderRequest) -> Result<String, String> {
        let direction = req.direction;
        self.calls.lock().unwrap().push(GatewayCall::SendOrder {
            symbol: req.symbol.clone(),
            direction,
            price: req.price,
            volume: req.volume,
        });

        let result = self.send_order_result.lock().unwrap().clone();
        
        // If success, send an order event
        if result.is_ok() {
            let orderid = self.generate_order_id();
            if let Some(sender) = self.event_sender.lock().unwrap().as_ref() {
                let order = OrderData {
                            gateway_name: self.name.clone(),
                            symbol: req.symbol.clone(),
                            exchange: req.exchange,
                            orderid: orderid.clone(),
                            order_type: req.order_type,
                            direction: Some(direction),
                            offset: req.offset,
                            price: req.price,
                            volume: req.volume,
                            traded: 0.0,
                            status: Status::NotTraded,
                            datetime: Some(chrono::Utc::now()),
                            reference: req.reference.clone(),
                            post_only: false,
                            reduce_only: false,
                            extra: None,
                        };
                sender.on_order(order);
            }
            return Ok(format!("{}.{}", self.name, orderid));
        }
        
        result
    }

    #[allow(clippy::unwrap_used)]
    async fn cancel_order(&self, req: CancelRequest) -> Result<(), String> {
        self.calls.lock().unwrap().push(GatewayCall::CancelOrder {
            orderid: req.orderid.clone(),
        });
        Ok(())
    }

    #[allow(clippy::unwrap_used)]
    async fn query_account(&self) -> Result<(), String> {
        self.calls.lock().unwrap().push(GatewayCall::QueryAccount);
        
        // Optionally send account data
        if let Some(sender) = self.event_sender.lock().unwrap().as_ref() {
            let account = AccountData::new(self.name.clone(), "DEFAULT".to_string());
            sender.on_account(account);
        }
        
        Ok(())
    }

    #[allow(clippy::unwrap_used)]
    async fn query_position(&self) -> Result<(), String> {
        self.calls.lock().unwrap().push(GatewayCall::QueryPosition);
        
        // Optionally send position data
        if let Some(sender) = self.event_sender.lock().unwrap().as_ref() {
            let position = PositionData::new(
                self.name.clone(),
                "BTCUSDT".to_string(),
                Exchange::Binance,
                Direction::Long,
            );
            sender.on_position(position);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_gateway_connect() {
        let gateway = MockGateway::new("TEST");
        
        let result = gateway.connect(HashMap::new()).await;
        assert!(result.is_ok());
        assert!(gateway.was_called(&GatewayCall::Connect));
        assert_eq!(gateway.count_calls("connect"), 1);
    }

    #[tokio::test]
    async fn test_mock_gateway_connect_failure() {
        let gateway = MockGateway::new("TEST");
        gateway.set_connect_result(Err("Connection failed".to_string()));
        
        let result = gateway.connect(HashMap::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_gateway_send_order() {
        let gateway = MockGateway::new("TEST");
        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            trade_engine::trader::constant::OrderType::Limit,
            1.0,
        );
        
        let result = gateway.send_order(req).await;
        assert!(result.is_ok());
        assert_eq!(gateway.count_calls("send_order"), 1);
    }

    #[tokio::test]
    async fn test_mock_gateway_subscribe() {
        let gateway = MockGateway::new("TEST");
        let req = SubscribeRequest::new("BTCUSDT".to_string(), Exchange::Binance);
        
        let result = gateway.subscribe(req).await;
        assert!(result.is_ok());
        assert!(gateway.was_called(&GatewayCall::Subscribe {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
        }));
    }
}
