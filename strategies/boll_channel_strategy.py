"""
Bollinger Channel Strategy

Migrated from vnpy_ctastrategy. Uses Bollinger Bands + CCI for entry
and ATR for trailing stop-loss.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- API adapted: buy/sell/short/cover require vt_symbol as first arg
- bar.high_price -> bar["high_price"] (dict access)
- Removed type hint imports (StopOrder, TickData, BarData, TradeData, OrderData)
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class BollChannelStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"

    boll_window: float = 18
    boll_dev: float = 3.4
    cci_window: int = 10
    atr_window: int = 30
    sl_multiplier: float = 5.2
    fixed_size: int = 1

    boll_up: float = 0
    boll_down: float = 0
    cci_value: float = 0
    atr_value: float = 0
    intra_trade_high: float = 0
    intra_trade_low: float = 0
    long_stop: float = 0
    short_stop: float = 0

    parameters = [
        "boll_window",
        "boll_dev",
        "cci_window",
        "atr_window",
        "sl_multiplier",
        "fixed_size",
    ]
    variables = [
        "boll_up",
        "boll_down",
        "cci_value",
        "atr_value",
        "intra_trade_high",
        "intra_trade_low",
        "long_stop",
        "short_stop",
    ]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.bg = BarGenerator(self.on_bar, window=15, on_window_bar=self.on_15min_bar)
        self.am = ArrayManager()

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
        self.bg.update_bar(bar)

    def on_15min_bar(self, bar) -> None:
        """"""
        self.cancel_all()

        am = self.am
        am.update_bar(bar)
        if not am.inited:
            return

        self.boll_up, self.boll_down = am.boll(self.boll_window, self.boll_dev)
        self.cci_value = am.cci(self.cci_window)
        self.atr_value = am.atr(self.atr_window)

        if self.pos == 0:
            self.intra_trade_high = bar["high_price"]
            self.intra_trade_low = bar["low_price"]

            if self.cci_value > 0:
                self.buy(self.vt_symbol, self.boll_up, self.fixed_size)
            elif self.cci_value < 0:
                self.short(self.vt_symbol, self.boll_down, self.fixed_size)

        elif self.pos > 0:
            self.intra_trade_high = max(self.intra_trade_high, bar["high_price"])
            self.intra_trade_low = bar["low_price"]

            self.long_stop = self.intra_trade_high - self.atr_value * self.sl_multiplier
            self.sell(self.vt_symbol, self.long_stop, abs(self.pos))

        elif self.pos < 0:
            self.intra_trade_high = bar["high_price"]
            self.intra_trade_low = min(self.intra_trade_low, bar["low_price"])

            self.short_stop = self.intra_trade_low + self.atr_value * self.sl_multiplier
            self.cover(self.vt_symbol, self.short_stop, abs(self.pos))

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

    def on_stop_order(self, stop_order) -> None:
        """
        Callback of stop order update.
        """
        pass
