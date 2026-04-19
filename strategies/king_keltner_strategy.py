"""
King Keltner Strategy

Migrated from vnpy_ctastrategy. Uses Keltner Channel for breakout entry
with trailing stop exits. Spot-only (long-only) version.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- bar.high_price -> bar["high_price"], bar.low_price -> bar["low_price"], etc.
- Market-price execution instead of limit orders at band prices
  (limit orders at kk_up caused unrealistic fill-price distortion in backtest)
- Trailing stop uses bar-price-check pattern instead of limit sell orders
- BarGenerator uses no window aggregation (1-min bars directly)
  so ArrayManager inits within 500 bars
- Spot-only: removed short/cover, buy on upper band breakout only
- Explicit strategy_type = "spot"
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class KingKeltnerStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"
    strategy_type = "spot"

    kk_length: int = 11
    kk_dev: float = 1.6
    trailing_percent: float = 3.5
    fixed_size: int = 1

    kk_up: float = 0
    kk_down: float = 0
    intra_trade_high: float = 0
    intra_trade_low: float = 0

    parameters = ["kk_length", "kk_dev", "trailing_percent", "fixed_size"]
    variables = ["kk_up", "kk_down"]

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
        am: ArrayManager = self.am
        am.update_bar(bar)
        if not am.inited:
            return

        self.kk_up, self.kk_down = am.keltner(self.kk_length, self.kk_dev)

        close_price: float = bar["close_price"]

        if self.pos == 0:
            self.intra_trade_high = bar["high_price"]
            self.intra_trade_low = bar["low_price"]
            # Spot-only: buy on upper band breakout at market price
            if close_price > self.kk_up:
                self.buy(self.vt_symbol, close_price, self.fixed_size)

        elif self.pos > 0:
            self.intra_trade_high = max(self.intra_trade_high, bar["high_price"])
            self.intra_trade_low = bar["low_price"]

            # Trailing stop: check if bar's low crossed the trailing level
            trailing_stop_price: float = self.intra_trade_high * (1 - self.trailing_percent / 100)
            if bar["low_price"] <= trailing_stop_price:
                self.sell(self.vt_symbol, close_price * 0.99, abs(self.pos))
                self.write_log("追踪止损平仓")

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
