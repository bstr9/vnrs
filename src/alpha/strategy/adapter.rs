//! Adapter that wraps an [`AlphaStrategy`] and implements [`StrategyTemplate`],
//! allowing alpha strategies to run through the standard [`BacktestingEngine`].
//!
//! This adapter bridges the gap between the Alpha module's strategy interface
//! and the unified StrategyTemplate trait, enabling alpha strategies to benefit
//! from fill models, look-ahead bias prevention, and the full statistics module.

use std::collections::HashMap;

use crate::alpha::strategy::template::AlphaStrategy;
use crate::strategy::base::{CancelRequestType, StrategySetting, StrategyState, StrategyType, StopOrderRequest};
use crate::strategy::template::{BaseStrategy, StrategyContext, StrategyTemplate};
use crate::trader::{BarData, DepthData, OrderData, OrderRequest, TickData, TradeData};

/// Adapter that wraps an [`AlphaStrategy`] and implements [`StrategyTemplate`].
///
/// Alpha strategies use a target-position rebalancing model: after each bar,
/// the strategy sets target positions, and the adapter reconciles the current
/// positions with the targets by placing buy/sell/short/cover orders through
/// the standard [`BaseStrategy`] order pipeline.
///
/// # Order pipeline
///
/// ```text
/// on_bar → inner.on_bars() → read targets → base.buy/sell/short/cover → drain_pending_orders()
/// ```
///
/// The backtesting engine calls `drain_pending_orders()` after each callback,
/// so orders flow through the same fill-model path as regular strategies.
pub struct AlphaStrategyAdapter {
    /// The wrapped alpha strategy.
    inner: AlphaStrategy,
    /// Base strategy for position/order/state management (required by `StrategyTemplate`).
    base: BaseStrategy,
}

impl AlphaStrategyAdapter {
    /// Create a new adapter wrapping the given [`AlphaStrategy`].
    #[must_use]
    pub fn new(inner: AlphaStrategy) -> Self {
        let setting = StrategySetting::new();
        let base = BaseStrategy::new(
            inner.strategy_name.clone(),
            inner.vt_symbols.clone(),
            StrategyType::Futures,
            setting,
        );
        Self { inner, base }
    }
}

/// Threshold below which position differences are considered noise and no
/// order is generated. Prevents tiny orders from floating-point drift.
const REBALANCE_THRESHOLD: f64 = 1e-4;

impl StrategyTemplate for AlphaStrategyAdapter {
    fn strategy_name(&self) -> &str {
        &self.inner.strategy_name
    }

    fn vt_symbols(&self) -> &[String] {
        &self.inner.vt_symbols
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

    fn on_init(&mut self, _context: &StrategyContext) {
        self.inner.on_init();
        self.base.state = StrategyState::Inited;
        tracing::info!(
            "[AlphaStrategyAdapter] strategy '{}' initialized",
            self.inner.strategy_name
        );
    }

    fn on_start(&mut self) {
        self.base.state = StrategyState::Trading;
    }

    fn on_stop(&mut self) {
        self.inner.on_stop();
        self.base.state = StrategyState::Stopped;
    }

    fn on_tick(&mut self, _tick: &TickData, _context: &StrategyContext) {
        // Alpha strategies primarily work on bars; ticks are ignored.
    }

    fn on_bar(&mut self, bar: &BarData, _context: &StrategyContext) {
        // 1. Convert BarData to TickData for the alpha strategy's on_bars callback.
        let vt_symbol = bar.vt_symbol();
        let mut tick = TickData::new(
            bar.gateway_name.clone(),
            bar.symbol.clone(),
            bar.exchange,
            bar.datetime,
        );
        tick.last_price = bar.close_price;
        tick.open_price = bar.open_price;
        tick.high_price = bar.high_price;
        tick.low_price = bar.low_price;
        tick.volume = bar.volume;
        tick.turnover = bar.turnover;
        tick.open_interest = bar.open_interest;
        tick.bid_price_1 = bar.close_price;
        tick.ask_price_1 = bar.close_price;

        let mut tick_map = HashMap::new();
        tick_map.insert(vt_symbol.clone(), tick);

        // 2. Call alpha strategy's on_bars callback (may update target_data).
        self.inner.on_bars(&tick_map);

        // 3. Rebalance: read targets, compare with positions, place orders.
        for sym in &self.inner.vt_symbols.clone() {
            let target = self.inner.get_target(sym);
            let pos = self.get_position(sym);
            let diff = target - pos;

            if diff.abs() <= REBALANCE_THRESHOLD {
                continue;
            }

            let price = bar.close_price;

            if diff > 0.0 {
                // Need to increase net long position.
                if pos >= 0.0 {
                    // Currently flat or long — just buy to open.
                    self.base.buy(sym, price, diff, false);
                } else {
                    // Currently short — cover first, then buy the rest.
                    let cover_volume = diff.min(pos.abs());
                    self.base.cover(sym, price, cover_volume, false);
                    let remaining = diff - cover_volume;
                    if remaining > REBALANCE_THRESHOLD {
                        self.base.buy(sym, price, remaining, false);
                    }
                }
            } else {
                // Need to decrease net long (increase net short) position.
                let abs_diff = diff.abs();
                if pos <= 0.0 {
                    // Currently flat or short — just short to open.
                    self.base.short(sym, price, abs_diff, false);
                } else {
                    // Currently long — sell first, then short the rest.
                    let sell_volume = abs_diff.min(pos);
                    self.base.sell(sym, price, sell_volume, false);
                    let remaining = abs_diff - sell_volume;
                    if remaining > REBALANCE_THRESHOLD {
                        self.base.short(sym, price, remaining, false);
                    }
                }
            }
        }
    }

    fn on_depth(&mut self, _depth: &DepthData, _context: &StrategyContext) {
        // Alpha strategies do not use order-book depth.
    }

    fn on_order(&mut self, _order: &OrderData) {
        // No additional processing needed for order callbacks.
    }

    fn on_trade(&mut self, trade: &TradeData) {
        // Keep alpha strategy's position tracking in sync.
        self.inner.update_trade(trade);
        // Also update base positions to match.
        self.base.sync_position(&trade.vt_symbol(), self.inner.get_pos(&trade.vt_symbol()));
    }

    fn on_stop_order(&mut self, _stop_orderid: &str) {
        // No additional processing needed.
    }

    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        self.base.drain_pending_orders()
    }

    fn drain_pending_stop_orders(&mut self) -> Vec<StopOrderRequest> {
        self.base.drain_pending_stop_orders()
    }

    fn drain_pending_cancellations(&mut self) -> Vec<CancelRequestType> {
        self.base.drain_pending_cancellations()
    }

    fn update_position(&mut self, vt_symbol: &str, position: f64) {
        // Update both base and inner position tracking.
        self.base.sync_position(vt_symbol, position);
        self.inner
            .pos_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(vt_symbol.to_string(), position);
    }

    fn get_position(&self, vt_symbol: &str) -> f64 {
        let positions = self.base.positions.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *positions.get(vt_symbol).unwrap_or(&0.0)
    }

    fn get_target(&self, vt_symbol: &str) -> Option<f64> {
        let target = self.inner.get_target(vt_symbol);
        if target.abs() < REBALANCE_THRESHOLD {
            None
        } else {
            Some(target)
        }
    }

    fn set_target(&mut self, vt_symbol: &str, target: f64) {
        self.inner.set_target(vt_symbol, target);
    }

    fn set_engine_type(&mut self, engine_type: &str) {
        self.base.set_engine_type(engine_type);
    }

    fn set_parameters(&mut self, params: HashMap<String, String>) {
        self.base.set_parameters(params);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{Direction, Exchange, Offset};

    /// Helper: create an AlphaStrategy for testing.
    fn create_test_alpha_strategy() -> AlphaStrategy {
        AlphaStrategy::new(
            "TestAlpha".to_string(),
            vec!["BTCUSDT.BINANCE".to_string()],
            HashMap::new(),
        )
    }

    /// Helper: create a BarData for testing.
    fn create_test_bar() -> BarData {
        let mut bar = BarData::new(
            "TEST".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            chrono::Utc::now(),
        );
        bar.close_price = 50000.0;
        bar.open_price = 49900.0;
        bar.high_price = 50100.0;
        bar.low_price = 49800.0;
        bar.volume = 100.0;
        bar
    }

    #[test]
    fn test_alpha_strategy_adapter_on_bar_generates_buy_order() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);
        let ctx = StrategyContext::new();

        // Set target to 1.0 — position is 0, so diff = 1.0 → buy.
        adapter.inner.set_target("BTCUSDT.BINANCE", 1.0);

        let bar = create_test_bar();
        adapter.on_bar(&bar, &ctx);

        let orders = adapter.drain_pending_orders();
        assert_eq!(orders.len(), 1, "expected exactly 1 buy order");
        assert_eq!(orders[0].direction, Direction::Long);
        assert_eq!(orders[0].offset, Offset::Open);
        assert!((orders[0].volume - 1.0).abs() < 1e-6);
        assert!((orders[0].price - 50000.0).abs() < 1e-6);
    }

    #[test]
    fn test_alpha_strategy_adapter_on_bar_no_order_when_flat() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);
        let ctx = StrategyContext::new();

        // Target is 0, position is 0 → no rebalancing needed.
        let bar = create_test_bar();
        adapter.on_bar(&bar, &ctx);

        let orders = adapter.drain_pending_orders();
        assert!(orders.is_empty(), "expected no orders when target matches position");
    }

    #[test]
    fn test_alpha_strategy_adapter_positions() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);

        // Initially zero.
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 0.0).abs() < 1e-6);

        // Update via adapter.
        adapter.update_position("BTCUSDT.BINANCE", 2.5);
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 2.5).abs() < 1e-6);

        // Also reflected in inner.
        assert!((adapter.inner.get_pos("BTCUSDT.BINANCE") - 2.5).abs() < 1e-6);
    }

    #[test]
    fn test_alpha_strategy_adapter_rebalance_from_long_to_short() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);
        let ctx = StrategyContext::new();

        // Current position: long 1.0
        adapter.update_position("BTCUSDT.BINANCE", 1.0);

        // Target: short 1.0 → diff = -1.0 - 1.0 = -2.0
        adapter.inner.set_target("BTCUSDT.BINANCE", -1.0);

        let bar = create_test_bar();
        adapter.on_bar(&bar, &ctx);

        let orders = adapter.drain_pending_orders();
        // Should sell 1.0 (close long) and short 1.0 (open short).
        assert_eq!(orders.len(), 2, "expected 2 orders: sell + short");

        // First order: sell to close long.
        assert_eq!(orders[0].direction, Direction::Short);
        assert_eq!(orders[0].offset, Offset::Close);
        assert!((orders[0].volume - 1.0).abs() < 1e-6);

        // Second order: short to open.
        assert_eq!(orders[1].direction, Direction::Short);
        assert_eq!(orders[1].offset, Offset::Open);
        assert!((orders[1].volume - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_alpha_strategy_adapter_rebalance_from_short_to_long() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);
        let ctx = StrategyContext::new();

        // Current position: short 1.0
        adapter.update_position("BTCUSDT.BINANCE", -1.0);

        // Target: long 1.0 → diff = 1.0 - (-1.0) = 2.0
        adapter.inner.set_target("BTCUSDT.BINANCE", 1.0);

        let bar = create_test_bar();
        adapter.on_bar(&bar, &ctx);

        let orders = adapter.drain_pending_orders();
        // Should cover 1.0 (close short) and buy 1.0 (open long).
        assert_eq!(orders.len(), 2, "expected 2 orders: cover + buy");

        // First order: cover to close short.
        assert_eq!(orders[0].direction, Direction::Long);
        assert_eq!(orders[0].offset, Offset::Close);
        assert!((orders[0].volume - 1.0).abs() < 1e-6);

        // Second order: buy to open long.
        assert_eq!(orders[1].direction, Direction::Long);
        assert_eq!(orders[1].offset, Offset::Open);
        assert!((orders[1].volume - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_alpha_strategy_adapter_on_trade_updates_positions() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);

        let mut trade = TradeData::new(
            "BACKTEST".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "ORD001".to_string(),
            "TRD001".to_string(),
        );
        trade.direction = Some(Direction::Long);
        trade.offset = Offset::Open;
        trade.volume = 2.0;
        trade.price = 50000.0;

        adapter.on_trade(&trade);

        // Position should be updated in both inner and base.
        assert!((adapter.get_position("BTCUSDT.BINANCE") - 2.0).abs() < 1e-6);
        assert!((adapter.inner.get_pos("BTCUSDT.BINANCE") - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_alpha_strategy_adapter_strategy_name() {
        let alpha = create_test_alpha_strategy();
        let adapter = AlphaStrategyAdapter::new(alpha);
        assert_eq!(adapter.strategy_name(), "TestAlpha");
    }

    #[test]
    fn test_alpha_strategy_adapter_strategy_type() {
        let alpha = create_test_alpha_strategy();
        let adapter = AlphaStrategyAdapter::new(alpha);
        assert_eq!(adapter.strategy_type(), StrategyType::Futures);
    }

    #[test]
    fn test_alpha_strategy_adapter_vt_symbols() {
        let alpha = create_test_alpha_strategy();
        let adapter = AlphaStrategyAdapter::new(alpha);
        assert_eq!(adapter.vt_symbols(), &["BTCUSDT.BINANCE".to_string()]);
    }

    #[test]
    fn test_alpha_strategy_adapter_set_engine_type() {
        let alpha = create_test_alpha_strategy();
        let mut adapter = AlphaStrategyAdapter::new(alpha);
        adapter.set_engine_type("BACKTESTING");
        assert_eq!(adapter.base.get_engine_type(), "BACKTESTING");
    }
}
