"""
Volatility Breakout Strategy with Dynamic Take-Profit

A strategy that combines volatility filtering with dynamic take-profit,
as required by the project specification.

Core logic:
1. Volatility Filter: Only enter trades when ATR-based volatility is within
   an acceptable range (not too low = no movement, not too high = too risky).
   Uses ATR percentile ranking over a lookback window.

2. Entry Signal: EMA crossover (fast EMA crosses above slow EMA) confirmed
   by RSI not being overbought (RSI < 70) — avoids chasing tops.

3. Dynamic Take-Profit: Instead of a fixed take-profit level, the target
   scales with volatility. Higher ATR = wider profit target (more room to run).
   Take-profit = entry_price + tp_atr_mult * ATR

4. Stop-Loss: Also volatility-adaptive. Stop = entry_price - sl_atr_mult * ATR
   Checked via bar-price-check pattern (not pending limit orders).

5. Trailing Stop: After price moves favorably by 1x ATR from entry, switch
   to trailing mode — stop moves up with the highest price since entry,
   trailing by trailing_atr_mult * ATR.

This strategy is spot-only (long only).
"""

import math
from trade_engine import CtaStrategy, Direction
from cta_utils import BarGenerator, ArrayManager


class VolatilityBreakoutStrategy(CtaStrategy):
    """
    Volatility-filtered EMA crossover with dynamic ATR-scaled take-profit.

    Parameters:
    - fast_window / slow_window: EMA periods for crossover signal
    - atr_window: ATR lookback period
    - rsi_window: RSI lookback for overbought filter
    - vol_lookback: Bars to rank ATR percentile for volatility filter
    - vol_low_pctile / vol_high_pctile: ATR percentile range for entry
    - tp_atr_mult: Take-profit distance as multiple of ATR
    - sl_atr_mult: Stop-loss distance as multiple of ATR
    - trailing_atr_mult: Trailing stop distance as multiple of ATR
    - fixed_size: Position size (1 = 1 BTC)
    """

    author = "VNRS"

    # Signal parameters
    fast_window: int = 5
    slow_window: int = 20
    atr_window: int = 20
    rsi_window: int = 14

    # Volatility filter parameters
    vol_lookback: int = 50      # Bars to compute ATR percentile ranking
    vol_low_pctile: float = 30  # Min ATR percentile to allow entry (skip low-vol)
    vol_high_pctile: float = 90 # Max ATR percentile to allow entry (skip extreme vol)

    # Exit parameters (ATR-based dynamic)
    tp_atr_mult: float = 3.0       # Take-profit = entry + tp_atr_mult * ATR
    sl_atr_mult: float = 1.5       # Stop-loss = entry - sl_atr_mult * ATR
    trailing_atr_mult: float = 2.0 # Trailing stop distance from peak

    # Position sizing
    fixed_size: int = 1

    # Spot-only
    strategy_type: str = "spot"

    # Display variables
    fast_ma0: float = 0.0
    fast_ma1: float = 0.0
    slow_ma0: float = 0.0
    slow_ma1: float = 0.0
    atr_value: float = 0.0
    rsi_value: float = 0.0
    atr_percentile: float = 0.0
    vol_ok: bool = False
    tp_price: float = 0.0
    sl_price: float = 0.0
    trailing_stop: float = 0.0

    # Internal tracking
    entry_price: float = 0.0
    highest_since_entry: float = 0.0
    trailing_active: bool = False

    parameters = [
        "fast_window", "slow_window", "atr_window", "rsi_window",
        "vol_lookback", "vol_low_pctile", "vol_high_pctile",
        "tp_atr_mult", "sl_atr_mult", "trailing_atr_mult",
        "fixed_size",
    ]

    variables = [
        "fast_ma0", "fast_ma1", "slow_ma0", "slow_ma1",
        "atr_value", "rsi_value", "atr_percentile", "vol_ok",
        "tp_price", "sl_price", "trailing_stop",
    ]

    def on_init(self) -> None:
        """Callback when strategy is inited."""
        self.write_log("策略初始化")

        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

        self.load_bar(10)

    def on_start(self) -> None:
        """Callback when strategy is started."""
        self.write_log("策略启动")

    def on_stop(self) -> None:
        """Callback when strategy is stopped."""
        self.write_log("策略停止")

    def on_tick(self, tick) -> None:
        """Callback of new tick data update."""
        self.bg.update_tick(tick)

    def on_bar(self, bar) -> None:
        """Callback of new bar data update."""
        self.cancel_all()

        am = self.am
        am.update_bar(bar)
        if not am.inited:
            return

        # --- Calculate indicators ---
        fast_ma = am.ema(self.fast_window, array=True)
        slow_ma = am.ema(self.slow_window, array=True)
        self.fast_ma0 = fast_ma[-1]
        self.fast_ma1 = fast_ma[-2]
        self.slow_ma0 = slow_ma[-1]
        self.slow_ma1 = slow_ma[-2]

        self.atr_value = am.atr(self.atr_window)
        self.rsi_value = am.rsi(self.rsi_window)

        # --- Volatility Filter: ATR Percentile Ranking ---
        atr_array = am.atr(self.atr_window, array=True)
        self.atr_percentile = 0.0
        self.vol_ok = False

        # Compute ATR percentile over lookback window
        if len(atr_array) >= self.vol_lookback:
            recent_atr = atr_array[-self.vol_lookback:]
            # Filter out zeros (initial values before ATR stabilizes)
            valid_atr = [v for v in recent_atr if v > 0]
            if len(valid_atr) >= 10:
                current_atr = self.atr_value
                if current_atr > 0:
                    rank = sum(1 for v in valid_atr if v <= current_atr)
                    self.atr_percentile = (rank / len(valid_atr)) * 100
                    # Only trade when volatility is in the acceptable range
                    self.vol_ok = (
                        self.vol_low_pctile <= self.atr_percentile <= self.vol_high_pctile
                    )

        # Skip if ATR is invalid
        if self.atr_value <= 0 or math.isnan(self.atr_value):
            self.put_event()
            return

        # --- Detect crossover ---
        cross_above = (self.fast_ma1 <= self.slow_ma1) and (self.fast_ma0 > self.slow_ma0)
        cross_below = (self.fast_ma1 >= self.slow_ma1) and (self.fast_ma0 < self.slow_ma0)

        # --- Position management ---
        if self.pos > 0:
            # Update highest price since entry
            if bar["high_price"] > self.highest_since_entry:
                self.highest_since_entry = bar["high_price"]

            # Activate trailing stop once price moves 1x ATR in our favor
            if not self.trailing_active:
                if bar["close_price"] >= self.entry_price + self.atr_value:
                    self.trailing_active = True

            # Calculate current stop level
            if self.trailing_active:
                # Trailing stop: follows the highest price
                self.trailing_stop = self.highest_since_entry - self.trailing_atr_mult * self.atr_value
                current_stop = max(self.sl_price, self.trailing_stop)
            else:
                current_stop = self.sl_price

            # --- Exit checks (priority order) ---

            # 1. Dynamic take-profit hit
            if self.tp_price > 0 and bar["high_price"] >= self.tp_price:
                self.sell(self.vt_symbol, self.tp_price, abs(self.pos))
                self.write_log(
                    f"动态止盈触发: 目标{self.tp_price:.2f}, 当前最高{bar['high_price']:.2f}"
                )
                self._reset_position_state()
                self.put_event()
                return

            # 2. Stop-loss hit (bar-price-check pattern)
            if current_stop > 0 and bar["low_price"] <= current_stop:
                self.sell(self.vt_symbol, bar["close_price"] * 0.99, abs(self.pos))
                self.write_log(
                    f"止损触发: 止损位{current_stop:.2f}, 当前最低{bar['low_price']:.2f}"
                )
                self._reset_position_state()
                self.put_event()
                return

            # 3. Death cross exit (signal-based)
            if cross_below:
                self.sell(self.vt_symbol, bar["close_price"], abs(self.pos))
                self.write_log(f"死叉出场: 价格{bar['close_price']:.2f}")
                self._reset_position_state()
                self.put_event()
                return

        elif self.pos == 0:
            # --- Entry: Golden cross + volatility filter + not overbought ---
            if cross_above and self.vol_ok and self.rsi_value < 70:
                self.buy(self.vt_symbol, bar["close_price"], self.fixed_size)
                self.write_log(
                    f"金叉入场: EMA({self.fast_window})上穿EMA({self.slow_window}), "
                    f"ATR百分位{self.atr_percentile:.1f}, RSI{self.rsi_value:.1f}"
                )

        self.put_event()

    def on_trade(self, trade) -> None:
        """Callback of new trade data update — track entry price."""
        # Extract trade fields - support both dict and object access
        if isinstance(trade, dict):
            trade_direction = trade.get("direction", "")
            trade_price = trade["price"]
        else:
            trade_direction = trade.direction
            trade_price = trade.price

        is_long = str(trade_direction).upper() == "LONG"

        if is_long and self.entry_price == 0:
            # New long position opened
            self.entry_price = trade_price
            self.highest_since_entry = trade_price
            self.trailing_active = False

            # Calculate dynamic take-profit and stop-loss levels
            if self.atr_value > 0:
                self.tp_price = self.entry_price + self.tp_atr_mult * self.atr_value
                self.sl_price = self.entry_price - self.sl_atr_mult * self.atr_value
                self.trailing_stop = 0.0
            else:
                # Fallback: fixed percentage
                self.tp_price = self.entry_price * 1.05
                self.sl_price = self.entry_price * 0.97
                self.trailing_stop = 0.0

            self.write_log(
                f"入场价: {self.entry_price:.2f}, "
                f"动态止盈: {self.tp_price:.2f} (+{self.tp_atr_mult}x ATR), "
                f"止损: {self.sl_price:.2f} (-{self.sl_atr_mult}x ATR)"
            )
        elif not is_long:
            # Position closed — reset
            self._reset_position_state()

    def on_order(self, order) -> None:
        """Callback of new order data update."""
        pass

    def _reset_position_state(self) -> None:
        """Reset all position tracking variables."""
        self.entry_price = 0.0
        self.highest_since_entry = 0.0
        self.trailing_active = False
        self.tp_price = 0.0
        self.sl_price = 0.0
        self.trailing_stop = 0.0
