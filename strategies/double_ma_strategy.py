"""
Double Moving Average Strategy

Migrated from vnpy_ctastrategy. Uses fast/slow SMA crossover
for entry and exit signals.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- API adapted: am.sma(n, array=True) → am.sma(n, array=True)
  (same interface, pure Python implementation)
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class DoubleMaStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"

    fast_window: int = 10
    slow_window: int = 20

    fast_ma0: float = 0.0
    fast_ma1: float = 0.0
    slow_ma0: float = 0.0
    slow_ma1: float = 0.0

    parameters = ["fast_window", "slow_window"]
    variables = ["fast_ma0", "fast_ma1", "slow_ma0", "slow_ma1"]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.bg: BarGenerator = BarGenerator(self.on_bar)
        self.am: ArrayManager = ArrayManager()

        self.load_bar(10)

    def on_start(self) -> None:
        """
        Callback when strategy is started.
        """
        self.write_log("策略启动")
        self.put_event()

    def on_stop(self) -> None:
        """
        Callback when strategy is stopped.
        """
        self.write_log("策略停止")

        self.put_event()

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

        am = self.am
        am.update_bar(bar)
        if not am.inited:
            return

        fast_ma = am.sma(self.fast_window, array=True)
        self.fast_ma0 = fast_ma[-1]
        self.fast_ma1 = fast_ma[-2]

        slow_ma = am.sma(self.slow_window, array=True)
        self.slow_ma0 = slow_ma[-1]
        self.slow_ma1 = slow_ma[-2]

        cross_over = self.fast_ma0 > self.slow_ma0 and self.fast_ma1 < self.slow_ma1
        cross_below = self.fast_ma0 < self.slow_ma0 and self.fast_ma1 > self.slow_ma1

        vt_symbol = self.vt_symbol
        pos = self.pos

        if cross_over:
            if pos == 0:
                self.buy(vt_symbol, bar["close_price"], 1)
            elif pos < 0:
                self.cover(vt_symbol, bar["close_price"], 1)
                self.buy(vt_symbol, bar["close_price"], 1)

        elif cross_below:
            if pos == 0:
                self.short(vt_symbol, bar["close_price"], 1)
            elif pos > 0:
                self.sell(vt_symbol, bar["close_price"], 1)
                self.short(vt_symbol, bar["close_price"], 1)

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
