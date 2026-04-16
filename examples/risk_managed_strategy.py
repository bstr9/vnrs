"""
Strategy with risk management controls.
"""
from trade_engine import Strategy, RiskConfig


class RiskManagedStrategy(Strategy):
    """Strategy with pre-trade risk checks."""

    def __init__(self):
        super().__init__()
        self.max_position = 10.0
        self.max_daily_trades = 5
        self.daily_trade_count = 0

    def on_init(self):
        self.write_log("Risk-managed strategy initialized")

    def on_start(self):
        self.write_log("Risk-managed strategy started")
        self.daily_trade_count = 0

    def on_bar(self, bar):
        pos = self.get_pos()

        if abs(pos) >= self.max_position:
            self.write_log(f"Position limit reached: {pos}")
            return

        if self.daily_trade_count >= self.max_daily_trades:
            self.write_log(f"Daily trade limit reached: {self.daily_trade_count}")
            return

        if pos == 0:
            if self.can_trade(bar):
                self.buy(bar.vt_symbol, bar.close_price, 1.0, stop=False)

    def can_trade(self, bar):
        """Pre-trade risk check combining position and daily limits."""
        pos = self.get_pos()

        if abs(pos) >= self.max_position:
            return False

        if self.daily_trade_count >= self.max_daily_trades:
            return False

        return True

    def on_trade(self, trade):
        self.daily_trade_count += 1
        self.write_log(f"Trade executed: {trade.vt_tradeid}, daily count: {self.daily_trade_count}")
