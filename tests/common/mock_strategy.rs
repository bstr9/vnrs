//! Test Strategy for integration testing
//!
//! A simple strategy implementation that records all callback invocations
//! and can be configured to place orders automatically.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use trade_engine::strategy::{
    CancelRequestType, StrategyContext, StrategyState, StrategyTemplate,
    StrategyType, StopOrderRequest,
};
use trade_engine::trader::{
    BarData, DepthData, Direction, Exchange, Offset, OrderData, OrderRequest, OrderType, TickData,
    TradeData,
};

/// A test strategy that records all callback invocations for testing
///
/// This strategy tracks:
/// - All callback invocations (on_init, on_start, on_stop, on_tick, on_bar, on_order, on_trade)
/// - Received trades and orders
/// - Current position
///
/// It can be configured to:
/// - Automatically place buy orders on each bar
/// - Set a specific buy price
/// - Sell after reaching a target volume
pub struct TestStrategy {
    /// Strategy name
    name: String,
    /// Subscribed symbols
    vt_symbols: Vec<String>,
    /// Current state
    state: StrategyState,
    /// Strategy type
    strategy_type: StrategyType,
    /// Current position (net position)
    position: Arc<Mutex<f64>>,
    /// Target position
    target: Arc<Mutex<Option<f64>>>,

    // Callback tracking
    on_init_called: Arc<AtomicBool>,
    on_start_called: Arc<AtomicBool>,
    on_stop_called: Arc<AtomicBool>,
    on_tick_count: Arc<AtomicU64>,
    on_bar_count: Arc<AtomicU64>,
    on_order_count: Arc<AtomicU64>,
    on_trade_count: Arc<AtomicU64>,

    // Received data tracking
    trades_received: Arc<Mutex<Vec<TradeData>>>,
    orders_received: Arc<Mutex<Vec<OrderData>>>,
    bars_received: Arc<Mutex<Vec<BarData>>>,
    ticks_received: Arc<Mutex<Vec<TickData>>>,

    // Behavior configuration
    buy_on_bar: Arc<AtomicBool>,
    buy_price: Arc<Mutex<f64>>,
    buy_volume: Arc<Mutex<f64>>,
    sell_after_volume: Arc<Mutex<Option<f64>>>,

    // Pending orders for drain
    pending_orders: Arc<Mutex<Vec<OrderRequest>>>,
    pending_stop_orders: Arc<Mutex<Vec<StopOrderRequest>>>,
    pending_cancellations: Arc<Mutex<Vec<CancelRequestType>>>,
}

#[allow(clippy::unwrap_used)]
impl TestStrategy {
    /// Create a new TestStrategy with the given name and symbol
    pub fn new(name: &str, vt_symbol: &str) -> Self {
        Self {
            name: name.to_string(),
            vt_symbols: vec![vt_symbol.to_string()],
            state: StrategyState::NotInited,
            strategy_type: StrategyType::Futures,
            position: Arc::new(Mutex::new(0.0)),
            target: Arc::new(Mutex::new(None)),
            on_init_called: Arc::new(AtomicBool::new(false)),
            on_start_called: Arc::new(AtomicBool::new(false)),
            on_stop_called: Arc::new(AtomicBool::new(false)),
            on_tick_count: Arc::new(AtomicU64::new(0)),
            on_bar_count: Arc::new(AtomicU64::new(0)),
            on_order_count: Arc::new(AtomicU64::new(0)),
            on_trade_count: Arc::new(AtomicU64::new(0)),
            trades_received: Arc::new(Mutex::new(Vec::new())),
            orders_received: Arc::new(Mutex::new(Vec::new())),
            bars_received: Arc::new(Mutex::new(Vec::new())),
            ticks_received: Arc::new(Mutex::new(Vec::new())),
            buy_on_bar: Arc::new(AtomicBool::new(false)),
            buy_price: Arc::new(Mutex::new(0.0)),
            buy_volume: Arc::new(Mutex::new(1.0)),
            sell_after_volume: Arc::new(Mutex::new(None)),
            pending_orders: Arc::new(Mutex::new(Vec::new())),
            pending_stop_orders: Arc::new(Mutex::new(Vec::new())),
            pending_cancellations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if on_init was called
    pub fn was_on_init_called(&self) -> bool {
        self.on_init_called.load(Ordering::SeqCst)
    }

    /// Check if on_start was called
    pub fn was_on_start_called(&self) -> bool {
        self.on_start_called.load(Ordering::SeqCst)
    }

    /// Check if on_stop was called
    pub fn was_on_stop_called(&self) -> bool {
        self.on_stop_called.load(Ordering::SeqCst)
    }

    /// Get the count of on_tick calls
    pub fn tick_count(&self) -> u64 {
        self.on_tick_count.load(Ordering::SeqCst)
    }

    /// Get the count of on_bar calls
    pub fn bar_count(&self) -> u64 {
        self.on_bar_count.load(Ordering::SeqCst)
    }

    /// Get the count of on_order calls
    pub fn order_count(&self) -> u64 {
        self.on_order_count.load(Ordering::SeqCst)
    }

    /// Get the count of on_trade calls
    pub fn trade_count(&self) -> u64 {
        self.on_trade_count.load(Ordering::SeqCst)
    }

    /// Get all received trades
    pub fn trades(&self) -> Vec<TradeData> {
        self.trades_received.lock().unwrap().clone()
    }

    /// Get all received orders
    pub fn orders(&self) -> Vec<OrderData> {
        self.orders_received.lock().unwrap().clone()
    }

    /// Get all received bars
    pub fn bars(&self) -> Vec<BarData> {
        self.bars_received.lock().unwrap().clone()
    }

    /// Get all received ticks
    pub fn ticks(&self) -> Vec<TickData> {
        self.ticks_received.lock().unwrap().clone()
    }

    /// Enable or disable buying on each bar
    pub fn set_buy_on_bar(&self, enabled: bool) {
        self.buy_on_bar.store(enabled, Ordering::SeqCst);
    }

    /// Set the buy price
    pub fn set_buy_price(&self, price: f64) {
        *self.buy_price.lock().unwrap() = price;
    }

    /// Set the buy volume
    pub fn set_buy_volume(&self, volume: f64) {
        *self.buy_volume.lock().unwrap() = volume;
    }

    /// Set the sell-after-volume threshold
    pub fn set_sell_after_volume(&self, volume: Option<f64>) {
        *self.sell_after_volume.lock().unwrap() = volume;
    }

    /// Get current position
    pub fn get_current_position(&self) -> f64 {
        *self.position.lock().unwrap()
    }

    /// Place a buy order (adds to pending orders)
    pub fn buy(&self, vt_symbol: &str, price: f64, volume: f64) {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol);
        let req = OrderRequest {
            symbol,
            exchange,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: Offset::Open,
            reference: self.name.clone(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
        };
        self.pending_orders.lock().unwrap().push(req);
    }

    /// Place a sell order (adds to pending orders)
    pub fn sell(&self, vt_symbol: &str, price: f64, volume: f64) {
        let (symbol, exchange) = parse_vt_symbol(vt_symbol);
        let req = OrderRequest {
            symbol,
            exchange,
            direction: Direction::Short,
            order_type: OrderType::Limit,
            volume,
            price,
            offset: Offset::Close,
            reference: self.name.clone(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
        };
        self.pending_orders.lock().unwrap().push(req);
    }
}

/// Parse a vt_symbol into (symbol, exchange)
fn parse_vt_symbol(vt_symbol: &str) -> (String, Exchange) {
    let parts: Vec<&str> = vt_symbol.split('.').collect();
    let symbol = parts.first().unwrap_or(&"").to_string();
    let exchange = parts
        .get(1)
        .map(|e| match *e {
            "BINANCE" => Exchange::Binance,
            "BINANCE_USDM" => Exchange::BinanceUsdm,
            _ => Exchange::Local,
        })
        .unwrap_or(Exchange::Local);
    (symbol, exchange)
}

impl StrategyTemplate for TestStrategy {
    fn strategy_name(&self) -> &str {
        &self.name
    }

    fn vt_symbols(&self) -> &[String] {
        &self.vt_symbols
    }

    fn strategy_type(&self) -> StrategyType {
        self.strategy_type
    }

    fn state(&self) -> StrategyState {
        self.state
    }

    fn parameters(&self) -> HashMap<String, String> {
        let mut params = HashMap::new();
        params.insert(
            "buy_on_bar".to_string(),
            self.buy_on_bar.load(Ordering::SeqCst).to_string(),
        );
        params.insert(
            "buy_price".to_string(),
            format!("{}", self.buy_price.lock().unwrap()),
        );
        params
    }

    fn variables(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    fn on_init(&mut self, _context: &StrategyContext) {
        self.on_init_called.store(true, Ordering::SeqCst);
        self.state = StrategyState::Inited;
    }

    fn on_start(&mut self) {
        self.on_start_called.store(true, Ordering::SeqCst);
        self.state = StrategyState::Trading;
    }

    fn on_stop(&mut self) {
        self.on_stop_called.store(true, Ordering::SeqCst);
        self.state = StrategyState::Stopped;
    }

    fn on_tick(&mut self, tick: &TickData, _context: &StrategyContext) {
        self.on_tick_count.fetch_add(1, Ordering::SeqCst);
        self.ticks_received.lock().unwrap().push(tick.clone());
    }

    fn on_bar(&mut self, bar: &BarData, _context: &StrategyContext) {
        self.on_bar_count.fetch_add(1, Ordering::SeqCst);
        self.bars_received.lock().unwrap().push(bar.clone());

        // Auto-buy if configured
        if self.buy_on_bar.load(Ordering::SeqCst) {
            let buy_price = *self.buy_price.lock().unwrap();
            let buy_volume = *self.buy_volume.lock().unwrap();
            if buy_price > 0.0 && buy_volume > 0.0 {
                self.buy(&bar.vt_symbol(), buy_price, buy_volume);
            }
        }

        // Check if we should sell
        let current_pos = *self.position.lock().unwrap();
        if let Some(sell_threshold) = *self.sell_after_volume.lock().unwrap() {
            if current_pos >= sell_threshold {
                self.sell(&bar.vt_symbol(), bar.close_price, current_pos);
            }
        }
    }

    fn on_depth(&mut self, _depth: &DepthData, _context: &StrategyContext) {
        // Default: no-op
    }

    fn on_order(&mut self, order: &OrderData) {
        self.on_order_count.fetch_add(1, Ordering::SeqCst);
        self.orders_received.lock().unwrap().push(order.clone());
    }

    fn on_trade(&mut self, trade: &TradeData) {
        self.on_trade_count.fetch_add(1, Ordering::SeqCst);
        self.trades_received.lock().unwrap().push(trade.clone());

        // Update position based on trade direction
        let mut pos = self.position.lock().unwrap();
        match trade.direction {
            Some(Direction::Long) => *pos += trade.volume,
            Some(Direction::Short) => *pos -= trade.volume,
            Some(Direction::Net) | None => {}
        }
    }

    fn on_stop_order(&mut self, _stop_orderid: &str) {
        // Default: no-op
    }

    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        let mut orders = self.pending_orders.lock().unwrap();
        std::mem::take(&mut *orders)
    }

    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        let mut orders = self.pending_stop_orders.lock().unwrap();
        std::mem::take(&mut *orders)
    }

    fn drain_pending_cancellations(&mut self) -> Vec<CancelRequestType> {
        let mut cancellations = self.pending_cancellations.lock().unwrap();
        std::mem::take(&mut *cancellations)
    }

    fn update_position(&mut self, _vt_symbol: &str, position: f64) {
        *self.position.lock().unwrap() = position;
    }

    fn get_position(&self, _vt_symbol: &str) -> f64 {
        *self.position.lock().unwrap()
    }

    fn get_target(&self, _vt_symbol: &str) -> Option<f64> {
        *self.target.lock().unwrap()
    }

    fn set_target(&mut self, _vt_symbol: &str, target: f64) {
        *self.target.lock().unwrap() = Some(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_creation() {
        let strategy = TestStrategy::new("TEST_STRATEGY", "BTCUSDT.BINANCE");
        assert_eq!(strategy.strategy_name(), "TEST_STRATEGY");
        assert_eq!(strategy.vt_symbols(), &["BTCUSDT.BINANCE"]);
        assert_eq!(strategy.state(), StrategyState::NotInited);
    }

    #[test]
    fn test_strategy_lifecycle() {
        let mut strategy = TestStrategy::new("TEST", "BTCUSDT.BINANCE");
        let context = StrategyContext::new();

        assert!(!strategy.was_on_init_called());
        strategy.on_init(&context);
        assert!(strategy.was_on_init_called());
        assert_eq!(strategy.state(), StrategyState::Inited);

        strategy.on_start();
        assert!(strategy.was_on_start_called());
        assert_eq!(strategy.state(), StrategyState::Trading);

        strategy.on_stop();
        assert!(strategy.was_on_stop_called());
        assert_eq!(strategy.state(), StrategyState::Stopped);
    }

    #[test]
    fn test_strategy_receives_bar() {
        let mut strategy = TestStrategy::new("TEST", "BTCUSDT.BINANCE");
        let context = StrategyContext::new();

        let bar = BarData::new(
            "TEST".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );

        assert_eq!(strategy.bar_count(), 0);
        strategy.on_bar(&bar, &context);
        assert_eq!(strategy.bar_count(), 1);
        assert_eq!(strategy.bars().len(), 1);
    }

    #[test]
    fn test_strategy_buy_on_bar() {
        let mut strategy = TestStrategy::new("TEST", "BTCUSDT.BINANCE");
        let context = StrategyContext::new();

        strategy.set_buy_on_bar(true);
        strategy.set_buy_price(50000.0);
        strategy.set_buy_volume(1.0);

        let bar = BarData::new(
            "TEST".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );

        strategy.on_bar(&bar, &context);

        let pending = strategy.drain_pending_orders();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].direction, Direction::Long);
        assert_eq!(pending[0].price, 50000.0);
    }

    #[test]
    fn test_strategy_trade_updates_position() {
        let mut strategy = TestStrategy::new("TEST", "BTCUSDT.BINANCE");

        let trade = TradeData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORD1".to_string(),
            tradeid: "TRD1".to_string(),
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 2.0,
            datetime: Some(chrono::Utc::now()),
            extra: None,
        };

        strategy.on_trade(&trade);
        assert_eq!(strategy.get_current_position(), 2.0);
        assert_eq!(strategy.trade_count(), 1);
    }

    #[test]
    fn test_strategy_position_management() {
        let mut strategy = TestStrategy::new("TEST", "BTCUSDT.BINANCE");

        strategy.update_position("BTCUSDT.BINANCE", 5.0);
        assert_eq!(strategy.get_position("BTCUSDT.BINANCE"), 5.0);

        strategy.set_target("BTCUSDT.BINANCE", 10.0);
        assert_eq!(strategy.get_target("BTCUSDT.BINANCE"), Some(10.0));
    }
}
