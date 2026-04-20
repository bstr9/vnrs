//! Integration tests for Gateway + MainEngine event flow and order submission

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use trade_engine::trader::{
    AccountData, BaseGateway, CancelRequest, ContractData, DepthData, Direction, Exchange,
    GatewayEvent, GatewayEventSender, LogData, MainEngine, OmsEngine, OrderData, OrderRequest,
    OrderType, PositionData, Product, TradeData,
};
use common::mock_gateway::{GatewayCall, MockGateway};
use common::fixtures::{make_test_order_request, make_test_tick};

// ============================================================================
// Test 1: MainEngine creation
// ============================================================================

#[test]
fn test_main_engine_creation() {
    let engine = MainEngine::new();
    // Verify OMS engine is accessible
    let _oms: &Arc<OmsEngine> = engine.oms();
    // Verify engine starts with no gateways
    let gateway_names = engine.get_all_gateway_names();
    assert!(gateway_names.is_empty());
}

// ============================================================================
// Test 2: Add and find gateway
// ============================================================================

#[test]
fn test_add_and_find_gateway() {
    let engine = MainEngine::new();
    
    // Create MockGateway
    let gateway = Arc::new(MockGateway::new("MOCK_BINANCE"));
    
    // Add gateway to engine
    engine.add_gateway(gateway.clone());
    
    // Find gateway by name
    let found = engine.get_gateway("MOCK_BINANCE");
    assert!(found.is_some());
    assert_eq!(found.unwrap().gateway_name(), "MOCK_BINANCE");
    
    // Find gateway name by exchange
    let gateway_name = engine.find_gateway_name_for_exchange(Exchange::Binance);
    assert!(gateway_name.is_some());
    assert_eq!(gateway_name.unwrap(), "MOCK_BINANCE");
}

// ============================================================================
// Test 3: Event sender produces events
// ============================================================================

#[test]
fn test_gateway_event_sender() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = GatewayEventSender::new("BINANCE".to_string(), tx);
    
    // Create test tick
    let tick = make_test_tick("BTCUSDT", Exchange::Binance, 50000.0);
    
    // Send tick event
    sender.on_tick(tick.clone());
    
    // Receive and verify tick events (two events: specific + base type)
    let (event_type, event) = rx.blocking_recv().expect("Should receive event");
    assert!(event_type.starts_with("eTick."));
    assert!(matches!(event, GatewayEvent::Tick(_)));
    
    let (event_type2, _) = rx.blocking_recv().expect("Should receive base event");
    assert_eq!(event_type2, "eTick.");
    
    // Create test order
    let order = OrderData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        "ORDER_123".to_string(),
    );
    
    // Send order event (sends 2 events: specific + base)
    sender.on_order(order);
    
    // Receive order events
    let (event_type, event) = rx.blocking_recv().expect("Should receive order event");
    assert!(event_type.starts_with("eOrder."));
    assert!(matches!(event, GatewayEvent::Order(_)));
    
    let (event_type2, event2) = rx.blocking_recv().expect("Should receive base order event");
    assert_eq!(event_type2, "eOrder.");
    assert!(matches!(event2, GatewayEvent::Order(_)));
    
    // Create test trade
    let trade = TradeData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        "ORDER_123".to_string(),
        "TRADE_1".to_string(),
    );
    
    // Send trade event (sends 2 events: specific + base)
    sender.on_trade(trade);
    
    // Receive trade events
    let (event_type, event) = rx.blocking_recv().expect("Should receive trade event");
    assert!(event_type.starts_with("eTrade."));
    assert!(matches!(event, GatewayEvent::Trade(_)));
    
    let (event_type2, event2) = rx.blocking_recv().expect("Should receive base trade event");
    assert_eq!(event_type2, "eTrade.");
    assert!(matches!(event2, GatewayEvent::Trade(_)));
    
    // Create test position
    let position = PositionData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        Direction::Long,
    );
    
    // Send position event (sends 2 events: specific + base)
    sender.on_position(position);
    
    // Receive position events
    let (event_type, event) = rx.blocking_recv().expect("Should receive position event");
    assert!(event_type.starts_with("ePosition."));
    assert!(matches!(event, GatewayEvent::Position(_)));
    
    let (event_type2, event2) = rx.blocking_recv().expect("Should receive base position event");
    assert_eq!(event_type2, "ePosition.");
    assert!(matches!(event2, GatewayEvent::Position(_)));
}

// ============================================================================
// Test 4: Gateway connect recorded
// ============================================================================

#[tokio::test]
async fn test_mock_gateway_connect() {
    let gateway = MockGateway::new("TEST_GATEWAY");
    
    // Connect with empty settings
    let result = gateway.connect(HashMap::new()).await;
    
    assert!(result.is_ok());
    assert!(gateway.was_called(&GatewayCall::Connect));
    assert_eq!(gateway.count_calls("connect"), 1);
}

// ============================================================================
// Test 5: Gateway send_order recorded
// ============================================================================

#[tokio::test]
async fn test_mock_gateway_send_order() {
    let gateway = MockGateway::new("TEST_GATEWAY");
    
    // Create order request
    let req = OrderRequest::new(
        "BTCUSDT".to_string(),
        Exchange::Binance,
        Direction::Long,
        OrderType::Limit,
        1.0,
    );
    
    // Send order
    let result = gateway.send_order(req.clone()).await;
    
    assert!(result.is_ok());
    assert!(gateway.was_called(&GatewayCall::SendOrder {
        symbol: "BTCUSDT".to_string(),
        direction: Direction::Long,
        price: 0.0, // Default price from OrderRequest::new
        volume: 1.0,
    }));
    assert_eq!(gateway.count_calls("send_order"), 1);
}

// ============================================================================
// Test 6: Gateway cancel_order recorded
// ============================================================================

#[tokio::test]
async fn test_mock_gateway_cancel_order() {
    let gateway = MockGateway::new("TEST_GATEWAY");
    
    // Create cancel request
    let req = CancelRequest::new(
        "ORDER_123".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
    );
    
    // Cancel order
    let result = gateway.cancel_order(req).await;
    
    assert!(result.is_ok());
    assert!(gateway.was_called(&GatewayCall::CancelOrder {
        orderid: "ORDER_123".to_string(),
    }));
    assert_eq!(gateway.count_calls("cancel_order"), 1);
}

// ============================================================================
// Test 7: MainEngine event processing
// ============================================================================

#[tokio::test]
async fn test_main_engine_event_processing() {
    let engine = Arc::new(MainEngine::new());
    
    // Start engine in background
    let engine_clone = engine.clone();
    let handle = tokio::spawn(async move {
        engine_clone.start().await;
    });
    
    // Give the engine a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    
    // Get event sender
    let sender = engine.get_event_sender();
    
    // Create and send tick event
    let tick = make_test_tick("BTCUSDT", Exchange::Binance, 50000.0);
    let event_type = format!("eTick.{}", tick.vt_symbol());
    let _ = sender.send((event_type, GatewayEvent::Tick(tick.clone())));
    
    // Wait briefly for event processing
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    
    // Verify tick is available via engine
    let retrieved_tick = engine.get_tick(&tick.vt_symbol());
    assert!(retrieved_tick.is_some());
    assert_eq!(retrieved_tick.unwrap().last_price, 50000.0);
    
    // Stop engine
    engine.close().await;
    handle.abort();
}

// ============================================================================
// Test 8: MainEngine order submission through gateway
// ============================================================================

#[tokio::test]
async fn test_main_engine_order_submission() {
    let engine = MainEngine::new();
    
    // Create MockGateway
    let gateway = Arc::new(MockGateway::new("MOCK_BINANCE"));
    
    // Add gateway to engine
    engine.add_gateway(gateway.clone());
    
    // Create order request
    let req = make_test_order_request(
        "BTCUSDT",
        Exchange::Binance,
        Direction::Long,
        50000.0,
        1.0,
    );
    
    // Send order through engine
    let result = engine.send_order(req.clone(), "MOCK_BINANCE").await;
    
    assert!(result.is_ok());
    let vt_orderid = result.unwrap();
    assert!(vt_orderid.starts_with("MOCK_BINANCE."));
    
    // Verify gateway recorded the send_order call
    assert_eq!(gateway.count_calls("send_order"), 1);
}

// ============================================================================
// Test 9: GatewayEvent variant coverage
// ============================================================================

#[test]
fn test_gateway_event_variants() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = GatewayEventSender::new("BINANCE".to_string(), tx);
    
    // 1. Tick
    let tick = make_test_tick("BTCUSDT", Exchange::Binance, 50000.0);
    sender.on_tick(tick);
    
    // 2. Bar
    let bar = trade_engine::trader::BarData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        chrono::Utc::now(),
    );
    sender.on_bar(bar);
    
    // 3. Trade
    let trade = TradeData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        "ORDER_1".to_string(),
        "TRADE_1".to_string(),
    );
    sender.on_trade(trade);
    
    // 4. Order
    let order = OrderData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        "ORDER_1".to_string(),
    );
    sender.on_order(order);
    
    // 5. Position
    let position = PositionData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        Direction::Long,
    );
    sender.on_position(position);
    
    // 6. Account
    let account = AccountData::new("BINANCE".to_string(), "DEFAULT".to_string());
    sender.on_account(account);
    
    // 7. Contract
    let contract = ContractData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        "BTCUSDT".to_string(),
        Product::Futures,
        1.0,
        0.01,
    );
    sender.on_contract(contract);
    
    // 8. Log
    let log = LogData::new("BINANCE".to_string(), "Test log".to_string());
    sender.on_log(log);
    
    // 9. DepthBook
    let depth = DepthData::new(
        "BINANCE".to_string(),
        "BTCUSDT".to_string(),
        Exchange::Binance,
        chrono::Utc::now(),
    );
    sender.on_depth(depth);
    
    // Collect all events
    let mut event_count = 0;
    let mut variants_found: Vec<&str> = Vec::new();
    
    while let Ok((_, event)) = rx.try_recv() {
        event_count += 1;
        let variant = match event {
            GatewayEvent::Tick(_) => "Tick",
            GatewayEvent::Bar(_) => "Bar",
            GatewayEvent::Trade(_) => "Trade",
            GatewayEvent::Order(_) => "Order",
            GatewayEvent::Position(_) => "Position",
            GatewayEvent::Account(_) => "Account",
            GatewayEvent::Contract(_) => "Contract",
            GatewayEvent::Log(_) => "Log",
            GatewayEvent::Quote(_) => "Quote",
            GatewayEvent::DepthBook(_) => "DepthBook",
        };
        variants_found.push(variant);
    }
    
    // Verify we received events (note: some methods send 2 events)
    assert!(event_count >= 9, "Expected at least 9 events, got {}", event_count);
    
    // Verify all 9 variants are present
    assert!(variants_found.contains(&"Tick"), "Missing Tick variant");
    assert!(variants_found.contains(&"Bar"), "Missing Bar variant");
    assert!(variants_found.contains(&"Trade"), "Missing Trade variant");
    assert!(variants_found.contains(&"Order"), "Missing Order variant");
    assert!(variants_found.contains(&"Position"), "Missing Position variant");
    assert!(variants_found.contains(&"Account"), "Missing Account variant");
    assert!(variants_found.contains(&"Contract"), "Missing Contract variant");
    assert!(variants_found.contains(&"Log"), "Missing Log variant");
    assert!(variants_found.contains(&"DepthBook"), "Missing DepthBook variant");
}

// ============================================================================
// Test 10: MockGateway configurable responses
// ============================================================================

#[tokio::test]
async fn test_mock_gateway_configurable_responses() {
    let gateway = MockGateway::new("TEST_GATEWAY");
    
    // Test configurable connect failure
    gateway.set_connect_result(Err("connection failed".to_string()));
    let result = gateway.connect(HashMap::new()).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "connection failed");
    
    // Reset connect result
    gateway.set_connect_result(Ok(()));
    let result = gateway.connect(HashMap::new()).await;
    assert!(result.is_ok());
    
    // Test configurable send_order result
    gateway.set_send_order_result(Ok("ORDER_123".to_string()));
    let req = OrderRequest::new(
        "BTCUSDT".to_string(),
        Exchange::Binance,
        Direction::Long,
        OrderType::Limit,
        1.0,
    );
    let result = gateway.send_order(req).await;
    assert!(result.is_ok());
    // Note: MockGateway prefixes with gateway name, so we get TEST_GATEWAY.MOCK_ORDER_X
    assert!(result.unwrap().starts_with("TEST_GATEWAY."));
    
    // Test send_order failure
    gateway.set_send_order_result(Err("insufficient balance".to_string()));
    let req = OrderRequest::new(
        "BTCUSDT".to_string(),
        Exchange::Binance,
        Direction::Long,
        OrderType::Limit,
        1.0,
    );
    let result = gateway.send_order(req).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "insufficient balance");
}
