//! Adapter that wraps an [`AsyncStrategy`] and implements [`StrategyTemplate`],
//! allowing async strategies to run through the standard [`BacktestingEngine`].

use std::collections::HashMap;

use super::async_template::{AsyncStrategy, DecisionRecord};
use super::base::{CancelRequestType, StopOrderRequest, StrategySetting, StrategyState, StrategyType};
use super::template::{BaseStrategy, StrategyContext, StrategyTemplate};
use crate::trader::{
    Direction, OrderData, OrderRequest, TickData, TradeData,
};

/// Adapter that wraps a [`Box<dyn AsyncStrategy>`] and implements
/// [`StrategyTemplate`], bridging the async strategy interface to the
/// synchronous backtesting engine.
///
/// # How it works
///
/// Async strategies return `Vec<OrderRequest>` from `on_bar`/`on_tick`,
/// while `StrategyTemplate` expects orders to be collected via
/// [`drain_pending_orders()`](StrategyTemplate::drain_pending_orders).
/// The adapter stores returned orders in an internal buffer that is
/// drained on demand.
///
/// Async methods are invoked via [`tokio::task::block_in_place`] +
/// [`tokio::runtime::Handle::block_on`], which is safe inside a
/// multi-threaded Tokio runtime.
pub struct AsyncStrategyAdapter {
    /// The wrapped async strategy.
    inner: Box<dyn AsyncStrategy>,
    /// Base strategy for position/order/state management.
    base: BaseStrategy,
    /// Orders returned from async `on_bar`/`on_tick`, drained by the engine.
    pending_orders_buffer: Vec<OrderRequest>,
    /// Decision records accumulated from the async strategy.
    decisions_buffer: Vec<DecisionRecord>,
}

impl AsyncStrategyAdapter {
    /// Create a new adapter wrapping the given async strategy.
    #[must_use]
    pub fn new(inner: Box<dyn AsyncStrategy>) -> Self {
        let name = inner.strategy_name().to_string();
        let symbols = inner.vt_symbols().to_vec();
        let setting = StrategySetting::new();
        let base = BaseStrategy::new(name, symbols, StrategyType::Futures, setting);
        Self {
            inner,
            base,
            pending_orders_buffer: Vec::new(),
            decisions_buffer: Vec::new(),
        }
    }

    /// Drain and return the audit trail of decisions accumulated since the
    /// last call. The internal buffer is cleared.
    pub fn drain_decisions(&mut self) -> Vec<DecisionRecord> {
        // Also pull fresh decisions from the inner strategy.
        let mut fresh = self.inner.drain_decisions();
        self.decisions_buffer.append(&mut fresh);
        std::mem::take(&mut self.decisions_buffer)
    }
}

impl StrategyTemplate for AsyncStrategyAdapter {
    fn strategy_name(&self) -> &str {
        self.inner.strategy_name()
    }

    fn vt_symbols(&self) -> &[String] {
        self.inner.vt_symbols()
    }

    fn strategy_type(&self) -> StrategyType {
        StrategyType::Futures
    }

    fn state(&self) -> StrategyState {
        self.base.state
    }

    fn parameters(&self) -> HashMap<String, String> {
        self.base.parameters.clone()
    }

    fn variables(&self) -> HashMap<String, String> {
        self.base.variables.clone()
    }

    fn on_init(&mut self, context: &StrategyContext) {
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.inner.on_init(context).await
            })
        });
        if let Err(e) = result {
            tracing::error!("AsyncStrategy {} init failed: {}", self.inner.strategy_name(), e);
        }
    }

    fn on_start(&mut self) {
        // Async strategies don't have a separate start phase.
    }

    fn on_stop(&mut self) {
        // No-op by default.
    }

    fn on_tick(&mut self, tick: &TickData, context: &StrategyContext) {
        let orders = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.inner.on_tick(tick, context).await
            })
        });
        self.pending_orders_buffer.extend(orders);
        // Pull any decisions the inner strategy accumulated.
        let fresh = self.inner.drain_decisions();
        self.decisions_buffer.extend(fresh);
    }

    fn on_bar(&mut self, bar: &crate::trader::BarData, context: &StrategyContext) {
        let orders = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                self.inner.on_bar(bar, context).await
            })
        });
        self.pending_orders_buffer.extend(orders);
        // Pull any decisions the inner strategy accumulated.
        let fresh = self.inner.drain_decisions();
        self.decisions_buffer.extend(fresh);
    }

    fn on_order(&mut self, _order: &OrderData) {
        // Async strategies don't process order callbacks by default.
    }

    fn on_trade(&mut self, trade: &TradeData) {
        // Update position based on trade direction and volume.
        let vt_symbol = trade.vt_symbol();
        let current = self.get_position(&vt_symbol);
        let delta = match trade.direction {
            Some(Direction::Long) => trade.volume,
            Some(Direction::Short) => -trade.volume,
            Some(Direction::Net) => 0.0, // Net direction doesn't affect position delta
            None => 0.0,
        };
        self.base.sync_position(&vt_symbol, current + delta);
    }

    fn on_stop_order(&mut self, _stop_orderid: &str) {
        // No-op.
    }

    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        std::mem::take(&mut self.pending_orders_buffer)
    }

    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        Vec::new()
    }

    fn drain_pending_cancellations(&mut self) -> Vec<CancelRequestType> {
        Vec::new()
    }

    fn update_position(&mut self, vt_symbol: &str, position: f64) {
        self.base.sync_position(vt_symbol, position);
    }

    fn get_position(&self, vt_symbol: &str) -> f64 {
        self.base
            .positions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }

    fn set_engine_type(&mut self, engine_type: &str) {
        self.base.set_engine_type(engine_type);
    }

    fn set_parameters(&mut self, params: HashMap<String, String>) {
        self.base.set_parameters(params);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::async_template::{SignalType, StrategyError};
    use crate::trader::{BarData, Exchange, Interval, Offset, OrderType};
    use chrono::Utc;
    use std::collections::HashMap;

    /// A dummy async strategy that returns a single order from on_bar.
    struct DummyAsyncStrategy {
        name: String,
        symbols: Vec<String>,
        decisions: Vec<DecisionRecord>,
    }

    impl DummyAsyncStrategy {
        fn new(name: &str, symbols: Vec<String>) -> Self {
            Self {
                name: name.to_string(),
                symbols,
                decisions: Vec::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl AsyncStrategy for DummyAsyncStrategy {
        fn strategy_name(&self) -> &str {
            &self.name
        }

        fn vt_symbols(&self) -> &[String] {
            &self.symbols
        }

        async fn on_init(
            &mut self,
            _context: &StrategyContext,
        ) -> Result<(), StrategyError> {
            Ok(())
        }

        async fn on_bar(
            &mut self,
            bar: &BarData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            let _vt = bar.vt_symbol();
            self.decisions.push(DecisionRecord {
                timestamp: Utc::now(),
                strategy: self.name.clone(),
                signal: SignalType::Long,
                confidence: 0.9,
                features_used: vec!["close_price".into()],
                model_version: "dummy-v1".into(),
                inference_latency_us: 100,
                orders_generated: Vec::new(),
            });
            vec![OrderRequest {
                symbol: bar.symbol.clone(),
                exchange: bar.exchange,
                direction: Direction::Long,
                order_type: OrderType::Limit,
                volume: 1.0,
                price: bar.close_price,
                offset: Offset::Open,
                reference: self.name.clone(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
                gateway_name: String::new(),
            }]
        }

        async fn on_tick(
            &mut self,
            _tick: &TickData,
            _context: &StrategyContext,
        ) -> Vec<OrderRequest> {
            Vec::new()
        }

        fn target_weights(&self) -> HashMap<String, f64> {
            HashMap::new()
        }

        fn drain_decisions(&mut self) -> Vec<DecisionRecord> {
            std::mem::take(&mut self.decisions)
        }
    }

    fn make_bar(close: f64) -> BarData {
        let mut bar = BarData::new(
            "TEST".into(),
            "BTCUSDT".into(),
            Exchange::Binance,
            Utc::now(),
        );
        bar.interval = Some(Interval::Minute);
        bar.open_price = close - 50.0;
        bar.high_price = close + 100.0;
        bar.low_price = close - 100.0;
        bar.close_price = close;
        bar.volume = 1000.0;
        bar
    }

    fn make_trade(direction: Direction, volume: f64) -> TradeData {
        let mut trade = TradeData::new(
            "TEST".into(),
            "BTCUSDT".into(),
            Exchange::Binance,
            "ORDER_1".into(),
            "TRADE_1".into(),
        );
        trade.direction = Some(direction);
        trade.volume = volume;
        trade.price = 50000.0;
        trade
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_async_strategy_adapter_on_bar() {
        let inner = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let mut adapter = AsyncStrategyAdapter::new(Box::new(inner));
        let ctx = StrategyContext::new();
        let bar = make_bar(50000.0);

        adapter.on_bar(&bar, &ctx);

        let orders = adapter.drain_pending_orders();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].direction, Direction::Long);
        assert!((orders[0].price - 50000.0).abs() < f64::EPSILON);
        assert!((orders[0].volume - 1.0).abs() < f64::EPSILON);

        // Second drain should return empty (buffer was taken).
        let orders2 = adapter.drain_pending_orders();
        assert!(orders2.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_async_strategy_adapter_on_init() {
        let inner = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let mut adapter = AsyncStrategyAdapter::new(Box::new(inner));
        let ctx = StrategyContext::new();

        adapter.on_init(&ctx);

        assert_eq!(adapter.strategy_name(), "dummy");
        assert_eq!(adapter.state(), StrategyState::NotInited);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_async_strategy_adapter_positions() {
        let inner = DummyAsyncStrategy::new("dummy", vec!["BTCUSDT.BINANCE".into()]);
        let mut adapter = AsyncStrategyAdapter::new(Box::new(inner));

        // Initial position should be 0.
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 0.0).abs() < f64::EPSILON);

        // Simulate a long trade fill.
        let trade = make_trade(Direction::Long, 2.0);
        adapter.on_trade(&trade);
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 2.0).abs() < f64::EPSILON);

        // Simulate a short trade fill (reduces position).
        let trade2 = make_trade(Direction::Short, 1.0);
        adapter.on_trade(&trade2);
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 1.0).abs() < f64::EPSILON);

        // update_position should also work.
        adapter.update_position("BTCUSDT.BINANCE", 5.0);
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 5.0).abs() < f64::EPSILON);
    }
}
