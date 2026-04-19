"""
Turtle 15Min EMA Signal Strategy

Dual EMA crossover strategy (long-only for spot backtesting).
Entry: MA1 (short-term EMA) crosses above MA2 (long-term EMA) → buy
Exit: MA1 crosses below MA2 → sell
ATR is calculated for informational display only, not used for stop-loss.

Migrated from vnpy_ctastrategy. Follows the double_ma_strategy pattern.
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class Turtle15MinEmaSignalStrategy(CtaStrategy):
    """
    Dual EMA crossover strategy (long-only for spot).

    Entry: Golden cross (MA1 crosses above MA2) → buy
    Exit: Death cross (MA1 crosses below MA2) → sell
    """

    author = "用Python的交易员"

    ma1_window: int = 5   # Short-term EMA period
    ma2_window: int = 20  # Long-term EMA period
    atr_window: int = 20
    fixed_size: int = 1

    ma1_value: float = 0.0
    ma1_last: float = 0.0
    ma2_value: float = 0.0
    ma2_last: float = 0.0
    atr_value: float = 0.0

    parameters = ["ma1_window", "ma2_window", "atr_window", "fixed_size"]
    variables = ["ma1_value", "ma1_last", "ma2_value", "ma2_last", "atr_value"]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

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

        ma1_array = am.ema(self.ma1_window, array=True)
        self.ma1_value = ma1_array[-1]
        self.ma1_last = ma1_array[-2]

        ma2_array = am.ema(self.ma2_window, array=True)
        self.ma2_value = ma2_array[-1]
        self.ma2_last = ma2_array[-2]

        # ATR for display only
        self.atr_value = am.atr(self.atr_window)

        cross_over = self.ma1_value > self.ma2_value and self.ma1_last < self.ma2_last
        cross_below = self.ma1_value < self.ma2_value and self.ma1_last > self.ma2_last

        vt_symbol = self.vt_symbol
        pos = self.pos

        if cross_over:
            if pos == 0:
                self.buy(vt_symbol, bar["close_price"], self.fixed_size)

        elif cross_below:
            if pos > 0:
                self.sell(vt_symbol, bar["close_price"], abs(pos))

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
