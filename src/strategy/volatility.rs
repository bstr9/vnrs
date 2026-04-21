//! Volatility Breakout Strategy for BTCUSDT
//!
//! Strategy logic:
//! 1. **Volatility filter**: Only trade when NATR (ATR/Close * 100) exceeds a threshold.
//!    Low volatility = no signal, avoid choppy sideways markets.
//! 2. **Entry**: Bollinger Band breakout — buy when close crosses above upper band,
//!    sell when close crosses below lower band.
//! 3. **Dynamic take-profit**: Trailing stop at `highest_close_since_entry - tp_atr_mult * ATR`.
//! 4. **Dynamic stop-loss**: Fixed at `entry_price - sl_atr_mult * ATR`.
//!
//! This strategy works on spot (no short selling). For futures, extend with short/cover logic.

use std::collections::HashMap;

use crate::strategy::base::{StrategySetting, StrategyState, StrategyType};
use crate::strategy::template::{BaseStrategy, StrategyContext, StrategyTemplate};
use crate::trader::{
    ArrayManager, BarData, Direction, OrderData, OrderRequest, TickData, TradeData,
};

/// Volatility breakout strategy with ATR-based filter and dynamic take-profit.
///
/// Parameters (passed via StrategySetting):
/// - `atr_length`: ATR period (default 14)
/// - `boll_length`: Bollinger Band period (default 30)
/// - `boll_dev`: Bollinger Band deviation multiplier (default 2.0)
/// - `natr_threshold`: Minimum NATR to allow trading (default 0.6, i.e. 0.6%)
/// - `tp_atr_mult`: Take-profit trailing stop ATR multiplier (default 4.0)
/// - `sl_atr_mult`: Stop-loss ATR multiplier (default 1.0)
/// - `fixed_size`: Order size per trade (default 0.01)
/// - `am_length`: ArrayManager window size (default 100)
pub struct VolatilityStrategy {
    base: BaseStrategy,

    // Strategy parameters
    atr_length: usize,
    boll_length: usize,
    boll_dev: f64,
    natr_threshold: f64,
    tp_atr_mult: f64,
    sl_atr_mult: f64,
    fixed_size: f64,

    // Internal state
    am: ArrayManager,
    vt_symbol: String,

    // Position tracking for dynamic exits
    entry_price: f64,
    highest_since_entry: f64,
    trailing_stop: f64,
    stop_loss: f64,
    intra_trade_bar_count: usize,
    bar_count: usize, // Total bars processed
}

impl VolatilityStrategy {
    pub fn new(strategy_name: String, vt_symbol: String, setting: StrategySetting) -> Self {
        let atr_length = setting
            .get("atr_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(14) as usize;
        let boll_length = setting
            .get("boll_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(30) as usize;
        let boll_dev = setting
            .get("boll_dev")
            .and_then(|v| v.as_f64())
            .unwrap_or(2.0);
        let natr_threshold = setting
            .get("natr_threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.6);
        let tp_atr_mult = setting
            .get("tp_atr_mult")
            .and_then(|v| v.as_f64())
            .unwrap_or(4.0);
        let sl_atr_mult = setting
            .get("sl_atr_mult")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        let fixed_size = setting
            .get("fixed_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.01);
        let am_length = setting
            .get("am_length")
            .and_then(|v| v.as_u64())
            .unwrap_or(100) as usize;

        Self {
            base: BaseStrategy::new(
                strategy_name,
                vec![vt_symbol.clone()],
                StrategyType::Spot,
                setting,
            ),
            atr_length,
            boll_length,
            boll_dev,
            natr_threshold,
            tp_atr_mult,
            sl_atr_mult,
            fixed_size,
            am: ArrayManager::new(am_length),
            vt_symbol,
            entry_price: 0.0,
            highest_since_entry: 0.0,
            trailing_stop: 0.0,
            stop_loss: 0.0,
            intra_trade_bar_count: 0,
            bar_count: 0,
        }
    }

    /// Get current position for the traded symbol
    fn pos(&self) -> f64 {
        self.base
            .positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&self.vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }
}

impl StrategyTemplate for VolatilityStrategy {
    fn strategy_name(&self) -> &str {
        &self.base.strategy_name
    }

    fn vt_symbols(&self) -> &[String] {
        &self.base.vt_symbols
    }

    fn strategy_type(&self) -> StrategyType {
        self.base.strategy_type
    }

    fn state(&self) -> StrategyState {
        self.base.state
    }

    fn parameters(&self) -> HashMap<String, String> {
        self.base.parameters.clone()
    }

    fn variables(&self) -> HashMap<String, String> {
        let mut vars = self.base.variables.clone();
        vars.insert("entry_price".to_string(), format!("{:.2}", self.entry_price));
        vars.insert("trailing_stop".to_string(), format!("{:.2}", self.trailing_stop));
        vars.insert("stop_loss".to_string(), format!("{:.2}", self.stop_loss));
        vars
    }

    fn on_init(&mut self, _context: &StrategyContext) {
        self.base.write_log("VolatilityStrategy initializing");
        self.base.state = StrategyState::Inited;
    }

    fn on_start(&mut self) {
        self.base.write_log("VolatilityStrategy started");
        self.base.state = StrategyState::Trading;
    }

    fn on_stop(&mut self) {
        self.base.write_log("VolatilityStrategy stopped");
        self.base.state = StrategyState::Stopped;
    }

    fn on_tick(&mut self, _tick: &TickData, _context: &StrategyContext) {
        // This strategy operates on bars only
    }

    fn on_bar(&mut self, bar: &BarData, _context: &StrategyContext) {
        self.bar_count += 1;

        // Update ArrayManager
        self.am.update_bar(bar);

        // Need enough data for indicators
        if !self.am.is_inited() {
            return;
        }

        let pos = self.pos();
        let close = bar.close_price;

        // Calculate indicators needed for both exit and entry logic
        let atr = self.am.atr(self.atr_length);

        // Guard against zero indicators (shouldn't happen after init, but be safe)
        if atr <= 0.0 || close <= 0.0 {
            return;
        }

        // ============ EXIT LOGIC (check before entry) ============

        if pos > 0.0 {
            self.intra_trade_bar_count += 1;

            // Update trailing stop: ratchet up only
            self.highest_since_entry = self.highest_since_entry.max(close);
            let new_trailing = self.highest_since_entry - self.tp_atr_mult * atr;
            if new_trailing > self.trailing_stop {
                self.trailing_stop = new_trailing;
            }

            // Trailing stop hit — take profit
            if close <= self.trailing_stop && self.intra_trade_bar_count > 1 {
                self.base.write_log(&format!(
                    "Trailing stop hit: close={:.2} <= trailing={:.2}",
                    close, self.trailing_stop
                ));
                self.base.sell(&self.vt_symbol, close, pos, false);
                self.entry_price = 0.0;
                self.highest_since_entry = 0.0;
                self.trailing_stop = 0.0;
                self.stop_loss = 0.0;
                self.intra_trade_bar_count = 0;
                return;
            }

            // Stop-loss hit
            if self.stop_loss > 0.0 && close <= self.stop_loss {
                self.base.write_log(&format!(
                    "Stop-loss hit: close={:.2} <= sl={:.2}",
                    close, self.stop_loss
                ));
                self.base.sell(&self.vt_symbol, close, pos, false);
                self.entry_price = 0.0;
                self.highest_since_entry = 0.0;
                self.trailing_stop = 0.0;
                self.stop_loss = 0.0;
                self.intra_trade_bar_count = 0;
                return;
            }

            // No exit signal this bar
            return;
        }

        // ============ ENTRY LOGIC (only when flat) ============

        // Calculate entry-specific indicators
        let natr = self.am.natr(self.atr_length);
        let (boll_upper, boll_mid, _boll_lower) = self.am.boll(self.boll_length, self.boll_dev);

        if boll_mid <= 0.0 {
            return;
        }

        // Volatility filter: skip if NATR too low (choppy/sideways market)
        if natr < self.natr_threshold {
            return;
        }

        // Bollinger Band breakout entry (spot: long only)
        if close > boll_upper {
            self.base.write_log(&format!(
                "BUY signal: close={:.2} > boll_upper={:.2}, NATR={:.2}%",
                close, boll_upper, natr
            ));
            self.base.buy(&self.vt_symbol, close, self.fixed_size, false);

            // Set dynamic stops based on ATR
            self.entry_price = close;
            self.highest_since_entry = close;
            self.trailing_stop = close - self.tp_atr_mult * atr;
            self.stop_loss = close - self.sl_atr_mult * atr;
            self.intra_trade_bar_count = 0;
        }
        // Note: For spot strategies we don't short. If you want a futures version,
        // add: `else if close < boll_lower { self.base.short(...) }`
    }

    fn on_order(&mut self, _order: &OrderData) {}

    fn on_trade(&mut self, trade: &TradeData) {
        // Sync position from trade
        let vt_symbol = format!("{}.{}", trade.symbol, trade.exchange);
        let current = self.pos();
        let new_pos = if trade.direction == Some(Direction::Long) {
            current + trade.volume
        } else {
            current - trade.volume
        };
        self.base.sync_position(&vt_symbol, new_pos);
        self.base.write_log(&format!(
            "Trade filled: {} {:?} @ {:.2} x {:.4}, new_pos={:.4}",
            vt_symbol, trade.direction, trade.price, trade.volume, new_pos
        ));
    }

    fn on_stop_order(&mut self, _stop_orderid: &str) {}

    fn drain_pending_orders(&mut self) -> Vec<OrderRequest> {
        self.base.drain_pending_orders()
    }

    fn update_position(&mut self, vt_symbol: &str, position: f64) {
        self.base.sync_position(vt_symbol, position);
    }

    fn get_position(&self, vt_symbol: &str) -> f64 {
        self.base
            .positions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(vt_symbol)
            .copied()
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{BarData, Exchange, Interval};
    use chrono::Utc;

    fn make_bar(price: f64, dt_offset: i64) -> BarData {
        BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: Utc::now() + chrono::Duration::minutes(dt_offset),
            interval: Some(Interval::Minute),
            open_price: price,
            high_price: price * 1.005,
            low_price: price * 0.995,
            close_price: price,
            volume: 100.0,
            turnover: price * 100.0,
            open_interest: 0.0,
            extra: None,
        }
    }

    #[test]
    fn test_volatility_strategy_creation() {
        let setting = StrategySetting::new();
        let strategy = VolatilityStrategy::new(
            "VolBTC".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        assert_eq!(strategy.strategy_name(), "VolBTC");
        assert_eq!(strategy.vt_symbols(), &["BTCUSDT.BINANCE".to_string()]);
        assert_eq!(strategy.strategy_type(), StrategyType::Spot);
    }

    #[test]
    fn test_volatility_strategy_with_custom_params() {
        let mut setting = StrategySetting::new();
        setting.insert("atr_length".to_string(), serde_json::json!(20));
        setting.insert("natr_threshold".to_string(), serde_json::json!(2.0));
        setting.insert("fixed_size".to_string(), serde_json::json!(0.1));

        let strategy = VolatilityStrategy::new(
            "VolBTC".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        assert_eq!(strategy.atr_length, 20);
        assert!((strategy.natr_threshold - 2.0).abs() < 1e-10);
        assert!((strategy.fixed_size - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_volatility_strategy_on_init() {
        let setting = StrategySetting::new();
        let mut strategy = VolatilityStrategy::new(
            "VolBTC".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        let ctx = StrategyContext::new();
        strategy.on_init(&ctx);
        assert_eq!(strategy.state(), StrategyState::Inited);
    }

    #[test]
    fn test_volatility_strategy_on_bar_not_inited() {
        let setting = StrategySetting::new();
        let mut strategy = VolatilityStrategy::new(
            "VolBTC".to_string(),
            "BTCUSDT.BINANCE".to_string(),
            setting,
        );
        let ctx = StrategyContext::new();

        // Feed bars — ArrayManager not yet inited (needs 100 bars), should produce no orders
        for i in 0..10 {
            strategy.on_bar(&make_bar(50000.0, i), &ctx);
        }
        let orders = strategy.drain_pending_orders();
        assert!(orders.is_empty());
    }
}
