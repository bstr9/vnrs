"""
Bitcoin Spot Strategy

Simplified EMA crossover strategy with clear entry/exit rules.
Long-only, designed for spot backtesting.

Entry: EMA5 crosses above EMA20 (golden cross)
Exit: EMA5 crosses below EMA20 (death cross) OR ATR stop-loss

Key features:
- ArrayManager-based EMA calculation
- Clear crossover entry (not trend-following)
- ATR-based stop-loss
- Re-entry allowed after exit (no cooldown)
"""

import math
from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class BitcoinSpotStrategy(CtaStrategy):
    """
    Bitcoin spot trading strategy.
    Based on EMA crossover, long-only.
    """

    author = "Modified for vnrs"

    # Parameter definitions
    ma1_window: int = 5  # Short-term EMA period
    ma2_window: int = 20  # Long-term EMA period (wider for fewer false signals)
    atr_window: int = 14
    fixed_size: float = 0.01  # Fixed order size (Bitcoin)
    atr_multiplier: float = 2.5  # ATR stop-loss multiplier

    # Strategy variables
    ma1_value: float = 0
    ma2_value: float = 0
    atr_value: float = 0
    long_stop: float = 0
    long_entry: float = 0

    parameters = [
        "ma1_window",
        "ma2_window",
        "atr_window",
        "fixed_size",
        "atr_multiplier",
    ]
    variables = [
        "ma1_value",
        "ma2_value",
        "atr_value",
        "long_stop",
    ]

    def on_init(self) -> None:
        """Callback when strategy is inited."""
        self.write_log("Strategy initialized")
        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()
        self.load_bar(max(self.ma1_window, self.ma2_window, self.atr_window) + 10)

    def on_start(self) -> None:
        """Callback when strategy is started."""
        self.write_log("Strategy started")

    def on_stop(self) -> None:
        """Callback when strategy is stopped."""
        self.write_log("Strategy stopped")

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

        # Get bar values
        bar_close = bar["close_price"]
        bar_low = bar["low_price"]

        # Calculate EMA
        ma1_array = am.ema(self.ma1_window, array=True)
        ma2_array = am.ema(self.ma2_window, array=True)

        if len(ma1_array) < 2 or len(ma2_array) < 2:
            return

        ma1_prev = ma1_array[-2]
        ma2_prev = ma2_array[-2]
        ma1_curr = ma1_array[-1]
        ma2_curr = ma2_array[-1]

        if (math.isnan(ma1_curr) or math.isnan(ma2_curr)
                or math.isnan(ma1_prev) or math.isnan(ma2_prev)):
            return

        self.ma1_value = ma1_curr
        self.ma2_value = ma2_curr

        # Crossover detection
        cross_above = (ma1_prev <= ma2_prev) and (ma1_curr > ma2_curr)
        cross_below = (ma1_prev >= ma2_prev) and (ma1_curr < ma2_curr)

        # Calculate ATR
        atr_temp = am.atr(self.atr_window)
        if math.isnan(atr_temp) or atr_temp <= 0:
            atr_temp = self.atr_value if self.atr_value > 0 else bar_close * 0.01
        self.atr_value = atr_temp

        # --- Stop-loss check (highest priority) ---
        if self.pos > 0 and self.long_stop > 0:
            if bar_low <= self.long_stop:
                self.sell(self.vt_symbol, bar_close * 0.99, abs(self.pos))
                self.put_event()
                return

        # --- Death cross exit ---
        if self.pos > 0 and cross_below:
            self.sell(self.vt_symbol, bar_close * 0.99, abs(self.pos))
            self.put_event()
            return

        # --- Entry logic ---
        if self.pos == 0 and cross_above:
            # Golden cross — buy at close + 5
            buy_price = bar_close + 5
            self.buy(self.vt_symbol, buy_price, self.fixed_size)

        # --- Update trailing stop ---
        if self.pos > 0 and self.long_entry > 0 and self.atr_value > 0:
            new_stop = self.long_entry - self.atr_multiplier * self.atr_value
            if new_stop > self.long_stop:
                self.long_stop = new_stop

        self.put_event()

    def on_trade(self, trade) -> None:
        """Callback of new trade data update."""
        if hasattr(trade, "direction"):
            trade_direction = trade.direction
        else:
            trade_direction = trade.get("direction", "")

        trade_price = trade.price if hasattr(trade, "price") else trade.get("price", 0)
        is_long = str(trade_direction).upper() == "LONG"

        if is_long:
            self.long_entry = trade_price
            if self.atr_value > 0:
                self.long_stop = self.long_entry - self.atr_multiplier * self.atr_value
            else:
                self.long_stop = self.long_entry * 0.95
        else:
            self.long_entry = 0
            self.long_stop = 0

    def on_order(self, order) -> None:
        """Callback of new order data update."""
        pass
