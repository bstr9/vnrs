"""
Turtle Signal Strategy

Migrated from vnpy_ctastrategy. Uses Donchian Channel breakout
for entry signals and ATR-based stop loss for exit.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- Direction enum referenced via string comparison
- stop=True parameter removed from order calls (engine doesn't support stop orders)
- am.donchian() / am.atr() use cta_utils pure Python implementation
- Adapted for spot trading (long only, no short/cover)
- Fixed direction string comparison in on_trade (engine sends "Long"/"Short")
- Entry: signal-based (buy when close breaks above 20-bar high)
- Exit: signal-based (sell when close breaks below 10-bar low or ATR trailing stop)
"""

import math
from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class TurtleSignalStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"

    entry_window: int = 20
    exit_window: int = 10
    atr_window: int = 20
    fixed_size: int = 1

    entry_up: float = 0
    entry_down: float = 0
    exit_up: float = 0
    exit_down: float = 0
    atr_value: float = 0
    long_entry: float = 0
    long_stop: float = 0

    parameters = ["entry_window", "exit_window", "atr_window", "fixed_size"]
    variables = ["entry_up", "entry_down", "exit_up", "exit_down", "atr_value"]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

        self.load_bar(20)

    def on_start(self) -> None:
        """
        Callback when strategy is started.
        """
        self.write_log("策略启动")

    def on_stop(self) -> None:
        """
        Callback when strategy is stopped.
        """
        self.write_log("策略停止")

    def on_tick(self, tick) -> None:
        """
        Callback of new tick data update.
        """
        self.bg.update_tick(tick)

    def on_bar(self, bar) -> None:
        """
        Callback of new bar data update.
        """
        self.cancel_all()

        self.am.update_bar(bar)
        if not self.am.inited:
            return

        # Calculate indicators
        self.entry_up, self.entry_down = self.am.donchian(self.entry_window)
        self.exit_up, self.exit_down = self.am.donchian(self.exit_window)

        if math.isnan(self.entry_up) or math.isnan(self.entry_down):
            return
        if math.isnan(self.exit_up) or math.isnan(self.exit_down):
            return

        atr_temp = self.am.atr(self.atr_window)
        if not math.isnan(atr_temp) and atr_temp > 0:
            self.atr_value = atr_temp

        if self.atr_value == 0 or math.isnan(self.atr_value):
            return

        vt_symbol = self.vt_symbol
        pos = self.pos

        # Extract bar prices - support both dict and object access
        if isinstance(bar, dict):
            close_price = bar["close_price"]
            high_price = bar["high_price"]
            low_price = bar["low_price"]
        else:
            close_price = getattr(bar, "close_price", 0)
            high_price = getattr(bar, "high_price", 0)
            low_price = getattr(bar, "low_price", 0)

        if not pos:
            # No position — check for breakout entry signal
            # Buy when high breaks above the entry_up (20-bar high)
            # This is more faithful to turtle rules: enter when price
            # touches the breakout level, not just when close exceeds it
            if high_price >= self.entry_up:
                # Place buy limit at the breakout level (entry_up)
                # This simulates a stop-entry order: if price traded at or
                # above entry_up during the bar, we would have been filled
                buy_price = self.entry_up
                self.buy(vt_symbol, buy_price, self.fixed_size)

        elif pos > 0:
            # In long position — check exit conditions
            # Sell when low breaks below exit_down (10-bar low) or ATR trailing stop
            if low_price <= self.exit_down:
                # Channel breakout exit — sell at exit_down price
                self.sell(vt_symbol, self.exit_down, abs(pos))
            elif self.long_stop > 0 and low_price <= self.long_stop:
                # ATR trailing stop exit — sell at stop price
                self.sell(vt_symbol, self.long_stop, abs(pos))

        self.put_event()

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        """
        direction = trade.get("direction", "") if isinstance(trade, dict) else ""
        price = trade.get("price", 0) if isinstance(trade, dict) else getattr(trade, "price", 0)
        # Engine sends "Long"/"Short" (Rust Debug format), handle both cases
        if direction in ("long", "Long"):
            self.long_entry = price
            self.long_stop = self.long_entry - 2 * self.atr_value

    def on_order(self, order) -> None:
        """
        Callback of new order data update.
        """
        pass
