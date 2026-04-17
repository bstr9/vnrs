"""
Turtle Signal Strategy

Migrated from vnpy_ctastrategy. Uses Donchian Channel breakout
for entry signals and ATR-based stop loss for exit.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- Direction enum referenced via string comparison
- stop=True parameter removed from order calls
- am.donchian() / am.atr() use cta_utils pure Python implementation
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
    short_entry: float = 0
    long_stop: float = 0
    short_stop: float = 0

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

        # Calculate exit channel and ATR
        self.exit_up, self.exit_down = self.am.donchian(self.exit_window)

        # Check if exit channel is valid
        if math.isnan(self.exit_up) or math.isnan(self.exit_down):
            return  # Not enough data

        atr_temp = self.am.atr(self.atr_window)

        # Check if ATR is valid
        if not math.isnan(atr_temp) and atr_temp > 0:
            self.atr_value = atr_temp

        # Skip if ATR is invalid
        if self.atr_value == 0 or math.isnan(self.atr_value):
            return

        vt_symbol = self.vt_symbol
        pos = self.pos

        if not pos:
            # Only update entry channel when no position
            self.entry_up, self.entry_down = self.am.donchian(self.entry_window)

            # Check if entry channel is valid
            if math.isnan(self.entry_up) or math.isnan(self.entry_down):
                return  # Not enough data

            self.long_entry = 0
            self.short_entry = 0
            self.long_stop = 0
            self.short_stop = 0

            self.send_buy_orders(self.entry_up)
            self.send_short_orders(self.entry_down)

        elif pos > 0:
            self.send_buy_orders(self.entry_up)

            sell_price = max(self.long_stop, self.exit_down)
            self.sell(vt_symbol, sell_price, abs(pos))

        elif pos < 0:
            self.send_short_orders(self.entry_down)

            cover_price = min(self.short_stop, self.exit_up)
            self.cover(vt_symbol, cover_price, abs(pos))

        self.put_event()

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        """
        direction = trade.get("direction", "") if isinstance(trade, dict) else ""
        if direction == "long":
            self.long_entry = trade["price"] if isinstance(trade, dict) else trade.price
            self.long_stop = self.long_entry - 2 * self.atr_value
        else:
            self.short_entry = (
                trade["price"] if isinstance(trade, dict) else trade.price
            )
            self.short_stop = self.short_entry + 2 * self.atr_value

    def on_order(self, order) -> None:
        """
        Callback of new order data update.
        """
        pass

    def send_buy_orders(self, price: float) -> None:
        """"""
        t: float = self.pos / self.fixed_size

        vt_symbol = self.vt_symbol

        if t < 1:
            self.buy(vt_symbol, price, self.fixed_size)

        if t < 2:
            self.buy(vt_symbol, price + self.atr_value * 0.5, self.fixed_size)

        if t < 3:
            self.buy(vt_symbol, price + self.atr_value, self.fixed_size)

        if t < 4:
            self.buy(vt_symbol, price + self.atr_value * 1.5, self.fixed_size)

    def send_short_orders(self, price: float) -> None:
        """"""
        t: float = self.pos / self.fixed_size

        vt_symbol = self.vt_symbol

        if t > -1:
            self.short(vt_symbol, price, self.fixed_size)

        if t > -2:
            self.short(vt_symbol, price - self.atr_value * 0.5, self.fixed_size)

        if t > -3:
            self.short(vt_symbol, price - self.atr_value, self.fixed_size)

        if t > -4:
            self.short(vt_symbol, price - self.atr_value * 1.5, self.fixed_size)
