"""
Bollinger Channel Strategy

Breakout-based strategy using Bollinger Bands. Buys on upper band breakout
(momentum) and exits on death cross or trailing stop. Spot-only (long-only).

Original vnpy version used mean-reversion at lower band, but this causes
repeated stop-outs in trending crypto markets. This version uses breakout
entry which captures momentum moves.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- API adapted: buy/sell require vt_symbol as first arg
- bar.high_price -> bar["high_price"] (dict access)
- Spot-only: removed short/cover calls, long-only breakout
- Market-price execution to avoid fill-price distortion
- Breakout entry instead of mean-reversion entry
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class BollChannelStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"
    strategy_type = "spot"

    boll_window: float = 20
    boll_dev: float = 2.0
    atr_window: int = 30
    trailing_percent: float = 2.0  # Trailing stop percentage from high
    fixed_size: int = 1

    boll_up: float = 0
    boll_down: float = 0
    atr_value: float = 0
    intra_trade_high: float = 0

    parameters = [
        "boll_window",
        "boll_dev",
        "atr_window",
        "trailing_percent",
        "fixed_size",
    ]
    variables = [
        "boll_up",
        "boll_down",
        "atr_value",
        "intra_trade_high",
    ]

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

        am = self.am
        am.update_bar(bar)
        if not am.inited:
            return

        self.boll_up, self.boll_down = am.boll(self.boll_window, self.boll_dev)
        self.atr_value = am.atr(self.atr_window)

        close_price = bar["close_price"]

        if self.pos == 0:
            self.intra_trade_high = bar["high_price"]

            # Breakout entry: buy when price breaks above upper Bollinger Band
            # This is a momentum signal, not mean-reversion
            if close_price > self.boll_up:
                self.buy(self.vt_symbol, close_price, self.fixed_size)
                self.write_log(f"突破买入: 价格={close_price:.2f}, 上轨={self.boll_up:.2f}")

        elif self.pos > 0:
            self.intra_trade_high = max(self.intra_trade_high, bar["high_price"])

            # Trailing stop exit: sell if bar low drops below trailing level
            trailing_stop = self.intra_trade_high * (1 - self.trailing_percent / 100)
            if bar["low_price"] <= trailing_stop:
                self.sell(self.vt_symbol, close_price * 0.99, abs(self.pos))
                self.write_log(f"追踪止损平仓: 最高={self.intra_trade_high:.2f}, 止损价={trailing_stop:.2f}, 价格={close_price:.2f}")
            # Take profit: exit when CCI goes negative (momentum fading)
            cci_value = am.cci(10)
            if cci_value < -100:
                self.sell(self.vt_symbol, close_price, abs(self.pos))
                self.write_log(f"CCI反转平仓: CCI={cci_value:.1f}, 价格={close_price:.2f}")

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
