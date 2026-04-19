"""
Multi-timeframe strategy using RSI for entry signals and MA for trend confirmation.
Simplified to single 1-min timeframe for spot backtesting compatibility.
Both RSI and MA trend are calculated on the same ArrayManager to ensure
initialization within 500 bars.
"""

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class MultiTimeframeStrategy(CtaStrategy):
    """Spot-only strategy with RSI entry and MA trend confirmation on 1-min bars."""

    author = "VNRS Migration"

    # Strategy parameters
    fast_window = 5
    slow_window = 20
    rsi_window = 14
    rsi_entry = 45  # RSI level for entry confirmation (not too restrictive)
    rsi_exit = 60   # RSI level for exit (earlier profit taking)
    fixed_size = 1
    max_holding_bars = 100  # Maximum holding time in bars

    # Strategy variables
    ma_trend = 0
    rsi_value = 0.0
    fast_ma = 0.0
    slow_ma = 0.0
    fast_ma_last = 0.0
    slow_ma_last = 0.0
    bars_held = 0  # Counter for holding time

    # Spot-only marker
    strategy_type = "spot"

    parameters = [
        "fast_window",
        "slow_window",
        "rsi_window",
        "rsi_entry",
        "rsi_exit",
        "fixed_size",
        "max_holding_bars",
    ]

    variables = [
        "ma_trend",
        "rsi_value",
        "fast_ma",
        "slow_ma",
        "fast_ma_last",
        "slow_ma_last",
        "bars_held",
    ]

    def __init__(self, strategy_name="MultiTimeframe", vt_symbols=None):
        """Initialize the strategy."""
        self.strategy_name = strategy_name
        self.vt_symbols = vt_symbols or ["BTCUSDT.BINANCE"]
        self.strategy_type = "spot"

        # Single BarGenerator with no window — passes 1-min bars directly
        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

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
        self.bg.update_tick(tick)

    def on_bar(self, bar):
        """Callback of new 1-min bar data update."""
        # Cancel all pending orders
        self.cancel_all()

        # Update ArrayManager
        self.am.update_bar(bar)
        if not self.am.inited:
            return

        # Calculate RSI
        self.rsi_value = self.am.rsi(self.rsi_window)

        # Calculate fast and slow SMA for trend direction (with array for crossover)
        fast_ma_arr = self.am.sma(self.fast_window, array=True)
        slow_ma_arr = self.am.sma(self.slow_window, array=True)
        self.fast_ma_last = self.fast_ma
        self.slow_ma_last = self.slow_ma
        self.fast_ma = fast_ma_arr[-1]
        self.slow_ma = slow_ma_arr[-1]

        # Determine MA trend: 1 = uptrend, -1 = downtrend
        if self.fast_ma > self.slow_ma:
            self.ma_trend = 1
        else:
            self.ma_trend = -1

        # Detect golden cross (fast crosses above slow) and death cross (fast crosses below slow)
        cross_above = (self.fast_ma > self.slow_ma) and (self.fast_ma_last <= self.slow_ma_last)
        cross_below = (self.fast_ma < self.slow_ma) and (self.fast_ma_last >= self.slow_ma_last)

        # --- Exit logic (signal-based only, no stop-loss) ---
        if self.pos > 0:
            # Increment holding counter
            self.bars_held += 1

            # Exit 1: Death cross exit (signal-based)
            if cross_below:
                self.write_log("Exit: Death cross detected")
                self.sell(self.vt_symbol, bar["close_price"], abs(self.pos))
                self.bars_held = 0
                self.put_event()
                return

            # Exit 2: RSI overbought exit (earlier profit taking at 60)
            if self.rsi_value > self.rsi_exit:
                self.write_log(f"Exit: RSI overbought ({self.rsi_value:.2f} > {self.rsi_exit})")
                self.sell(self.vt_symbol, bar["close_price"], abs(self.pos))
                self.bars_held = 0
                self.put_event()
                return

            # Exit 3: Maximum holding time exit
            if self.bars_held >= self.max_holding_bars:
                self.write_log(f"Exit: Max holding time reached ({self.bars_held} bars)")
                self.sell(self.vt_symbol, bar["close_price"], abs(self.pos))
                self.bars_held = 0
                self.put_event()
                return

        # --- Spot-only (long) entry logic ---
        # Buy signal: Golden cross (fast MA crosses above slow MA) AND RSI not overbought
        if cross_above and self.rsi_value < self.rsi_entry:
            if self.pos == 0:
                self.buy(self.vt_symbol, bar["close_price"], self.fixed_size)

        # Update GUI
        self.put_event()

    def on_order(self, order):
        """Callback of new order update."""
        pass

    def on_trade(self, trade):
        """Callback of new trade update."""
        # Reset bar counter on entry
        if hasattr(trade, "direction"):
            trade_direction = trade.direction
        else:
            trade_direction = trade.get("direction", "")
        
        is_long = str(trade_direction).upper() == "LONG"
        
        if is_long:
            self.bars_held = 0
        
        self.put_event()

    def on_stop_order(self, stop_order):
        """Callback of stop order update."""
        pass
