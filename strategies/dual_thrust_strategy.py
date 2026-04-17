"""
Dual Thrust Strategy

Migrated from vnpy_ctastrategy. Uses yesterday's range to calculate
today's breakout levels.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- bar.datetime is a dict key; datetime comparison adapted
- stop=True parameter removed from buy/sell/short/cover calls
  (vnrs does not support stop orders via Python API in the same way)
"""

from datetime import time
from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class DualThrustStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"

    fixed_size: int = 1
    k1: float = 0.4
    k2: float = 0.6

    day_open: float = 0
    day_high: float = 0
    day_low: float = 0
    day_range: float = 0
    long_entry: float = 0
    short_entry: float = 0
    long_entered: bool = False
    short_entered: bool = False

    parameters = ["k1", "k2", "fixed_size"]
    variables = ["day_range", "long_entry", "short_entry"]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.bg: BarGenerator = BarGenerator(self.on_bar)
        self.am: ArrayManager = ArrayManager()

        self.bars: list = []
        self.exit_time: time = time(hour=14, minute=55)

        self.load_bar(10)

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

        self.bars.append(bar)
        if len(self.bars) <= 2:
            return
        else:
            self.bars.pop(0)
        last_bar = self.bars[-2]

        # Date comparison - bar datetime may be string or datetime
        bar_dt = bar.get("datetime", "")
        last_dt = last_bar.get("datetime", "")

        # Compare dates
        bar_date = str(bar_dt)[:10] if bar_dt else ""
        last_date = str(last_dt)[:10] if last_dt else ""

        if bar_date != last_date:
            if self.day_high:
                self.day_range = self.day_high - self.day_low
                self.long_entry = bar["open_price"] + self.k1 * self.day_range
                self.short_entry = bar["open_price"] - self.k2 * self.day_range

            self.day_open = bar["open_price"]
            self.day_high = bar["high_price"]
            self.day_low = bar["low_price"]

            self.long_entered = False
            self.short_entered = False
        else:
            self.day_high = max(self.day_high, bar["high_price"])
            self.day_low = min(self.day_low, bar["low_price"])

        if not self.day_range:
            return

        vt_symbol = self.vt_symbol
        pos = self.pos

        # Time check - extract time from bar datetime
        bar_time_str = str(bar_dt)[11:16] if len(str(bar_dt)) > 16 else ""
        try:
            hour, minute = (
                map(int, bar_time_str.split(":")) if ":" in bar_time_str else (0, 0)
            )
            bar_time = time(hour=hour, minute=minute)
        except (ValueError, AttributeError):
            bar_time = time(0, 0)

        if bar_time < self.exit_time:
            if pos == 0:
                if bar["close_price"] > self.day_open:
                    if not self.long_entered:
                        self.buy(vt_symbol, self.long_entry, self.fixed_size)
                else:
                    if not self.short_entered:
                        self.short(vt_symbol, self.short_entry, self.fixed_size)

            elif pos > 0:
                self.long_entered = True

                self.sell(vt_symbol, self.short_entry, self.fixed_size)

                if not self.short_entered:
                    self.short(vt_symbol, self.short_entry, self.fixed_size)

            elif pos < 0:
                self.short_entered = True

                self.cover(vt_symbol, self.long_entry, self.fixed_size)

                if not self.long_entered:
                    self.buy(vt_symbol, self.long_entry, self.fixed_size)

        else:
            if pos > 0:
                self.sell(vt_symbol, bar["close_price"] * 0.99, abs(pos))
            elif pos < 0:
                self.cover(vt_symbol, bar["close_price"] * 1.01, abs(pos))

        self.put_event()

    def on_order(self, order) -> None:
        """
        Callback of new order data update.
        """
        pass

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        """
        self.put_event()
