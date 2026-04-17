"""
Multi-timeframe strategy using RSI for entry signals and MA for trend confirmation.
5-min bars for RSI-based entry, 15-min bars for MA trend direction.
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class MultiTimeframeStrategy(CtaStrategy):
    """Multi-timeframe strategy with RSI entry and MA trend confirmation."""

    author = "VNRS Migration"

    # Strategy parameters
    fast_window = 10
    slow_window = 20
    rsi_window = 14
    rsi_entry = 30
    fixed_size = 1

    # Strategy variables
    ma_trend = 0
    rsi_value = 0.0
    fast_ma = 0.0
    slow_ma = 0.0

    parameters = [
        "fast_window",
        "slow_window",
        "rsi_window",
        "rsi_entry",
        "fixed_size",
    ]

    variables = [
        "ma_trend",
        "rsi_value",
        "fast_ma",
        "slow_ma",
    ]

    def __init__(self, vt_symbol, engine):
        """Initialize the strategy."""
        super().__init__(vt_symbol, engine)

        # 5-minute timeframe components
        self.bg5 = BarGenerator(self.on_bar, window=5, on_window_bar=self.on_5min_bar)
        self.am5 = ArrayManager()

        # 15-minute timeframe components
        self.bg15 = BarGenerator(
            self.on_bar, window=15, on_window_bar=self.on_15min_bar
        )
        self.am15 = ArrayManager()

    def on_init(self):
        """Callback when strategy is initialized."""
        self.write_log("Strategy initialized")

    def on_start(self):
        """Callback when strategy is started."""
        self.write_log("Strategy started")

    def on_stop(self):
        """Callback when strategy is stopped."""
        self.write_log("Strategy stopped")

    def on_tick(self, tick):
        """Callback of new tick data update."""
        self.bg5.update_tick(tick)

    def on_bar(self, bar):
        """Callback of new bar data update."""
        # Feed both BarGenerators
        self.bg5.update_bar(bar)
        self.bg15.update_bar(bar)

    def on_5min_bar(self, bar):
        """Callback for 5-minute bar generation."""
        # Cancel all pending orders
        self.cancel_all()

        # Update 5-min ArrayManager
        self.am5.update_bar(bar)
        if not self.am5.inited:
            return

        # Calculate RSI on 5-min timeframe
        self.rsi_value = self.am5.rsi(self.rsi_window)

        # Check for trading signals
        # Long signal: RSI < entry level AND MA trend is up
        if self.rsi_value < self.rsi_entry and self.ma_trend > 0:
            if self.pos == 0:
                # Open long position
                self.buy(self.vt_symbol, bar["close_price"] + 5, self.fixed_size)
            elif self.pos < 0:
                # Close short and open long
                self.cover(self.vt_symbol, bar["close_price"] + 5, abs(self.pos))
                self.buy(self.vt_symbol, bar["close_price"] + 5, self.fixed_size)

        # Short signal: RSI > (100 - entry level) AND MA trend is down
        elif self.rsi_value > (100 - self.rsi_entry) and self.ma_trend < 0:
            if self.pos == 0:
                # Open short position
                self.short(self.vt_symbol, bar["close_price"] - 5, self.fixed_size)
            elif self.pos > 0:
                # Close long and open short
                self.sell(self.vt_symbol, bar["close_price"] - 5, abs(self.pos))
                self.short(self.vt_symbol, bar["close_price"] - 5, self.fixed_size)

        # Update GUI
        self.put_event()

    def on_15min_bar(self, bar):
        """Callback for 15-minute bar generation."""
        # Update 15-min ArrayManager
        self.am15.update_bar(bar)
        if not self.am15.inited:
            return

        # Calculate fast and slow SMAs on 15-min timeframe
        self.fast_ma = self.am15.sma(self.fast_window)
        self.slow_ma = self.am15.sma(self.slow_window)

        # Determine MA trend
        if self.fast_ma > self.slow_ma:
            self.ma_trend = 1  # Uptrend
        else:
            self.ma_trend = -1  # Downtrend

        # Update GUI
        self.put_event()

    def on_order(self, order):
        """Callback of new order update."""
        pass

    def on_trade(self, trade):
        """Callback of new trade update."""
        self.put_event()

    def on_stop_order(self, stop_order):
        """Callback of stop order update."""
        pass
