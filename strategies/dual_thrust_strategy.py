"""
Dual Thrust Strategy

Simplified for spot backtesting on 1-min synthetic bars.
Uses ATR breakout instead of daily range, since daily boundaries
are less meaningful with limited bars per day.

Entry: Close breaks above open + k1 * ATR(20)
Exit: Close breaks below entry - 1.5 * ATR(20) (stop-loss)
      OR close drops below open - k2 * ATR(20) (reverse signal)
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class DualThrustStrategy(CtaStrategy):
    """"""

    strategy_type = "spot"
    author = "用Python的交易员"

    fixed_size: int = 1
    k1: float = 0.4
    k2: float = 0.6
    atr_window: int = 20

    day_open: float = 0
    day_high: float = 0
    day_low: float = 0
    long_entry: float = 0
    long_entry_price: float = 0

    parameters = ["k1", "k2", "fixed_size", "atr_window"]
    variables = ["long_entry"]

    def on_init(self) -> None:
        """Callback when strategy is inited."""
        self.write_log("策略初始化")
        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()
        self.last_bar_date = ""
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

        # Extract bar date for day tracking
        bar_dt = bar.get("datetime", "")
        bar_date = str(bar_dt)[:10] if bar_dt else ""

        # Detect new day
        if bar_date != self.last_bar_date:
            self.day_open = bar["open_price"]
            self.day_high = bar["high_price"]
            self.day_low = bar["low_price"]
            self.last_bar_date = bar_date
        else:
            self.day_high = max(self.day_high, bar["high_price"])
            self.day_low = min(self.day_low, bar["low_price"])

        # Calculate ATR
        atr_value = am.atr(self.atr_window)
        if atr_value <= 0:
            self.put_event()
            return

        # Calculate breakout levels using ATR instead of day_range
        # This works on any timeframe, not just daily bars
        self.long_entry = self.day_open + self.k1 * atr_value

        close = bar["close_price"]
        pos = self.pos

        # --- Stop-loss check (highest priority) ---
        if pos > 0 and self.long_entry_price > 0:
            # ATR-based trailing stop: exit if close drops 1.5x ATR below entry
            if close < self.long_entry_price - 1.5 * atr_value:
                self.sell(self.vt_symbol, close * 0.99, abs(pos))
                self.long_entry_price = 0
                self.put_event()
                return

        # --- Reverse signal exit ---
        if pos > 0:
            reverse_level = self.day_open - self.k2 * atr_value
            if close < reverse_level:
                self.sell(self.vt_symbol, close * 0.99, abs(pos))
                self.long_entry_price = 0
                self.put_event()
                return

        # --- Entry: breakout above long_entry level ---
        if pos == 0:
            if close > self.long_entry:
                # Buy at close + 5 (ensures fill as buy limit)
                self.buy(self.vt_symbol, close + 5, self.fixed_size)

        self.put_event()

    def on_order(self, order) -> None:
        """Callback of new order data update."""
        pass

    def on_trade(self, trade) -> None:
        """Callback of new trade data update."""
        trade_direction = getattr(trade, "direction", "")
        if str(trade_direction).upper() == "LONG":
            self.long_entry_price = getattr(trade, "price", 0)
        else:
            self.long_entry_price = 0
        self.put_event()
