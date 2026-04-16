"""
Spot trading strategy example using SpotStrategyTemplate.
Shows position management without leverage/offset.
"""
from trade_engine import Strategy, SpotStrategyTemplate


class SimpleSpotStrategy(SpotStrategyTemplate):
    """Simple moving average crossover for spot trading."""

    def __init__(self):
        super().__init__()
        self.fast_period = 10
        self.slow_period = 20

    def on_init(self):
        self.write_log("Spot strategy initialized")

    def on_start(self):
        self.write_log("Spot strategy started")

    def on_stop(self):
        self.write_log("Spot strategy stopped")

    def on_bar(self, bar):
        pos = self.get_pos()
        avg_price = self.get_avg_price()
        unrealized_pnl = self.get_unrealized_pnl()

        self.write_log(f"Position: {pos}, Avg: {avg_price}, PnL: {unrealized_pnl}")

        # Buy if no position and price dropped 1%+ from open
        if pos == 0 and bar.close_price < bar.open_price * 0.99:
            self.buy(bar.vt_symbol, bar.close_price, 1.0, stop=False)

    def on_order(self, order):
        self.write_log(f"Order update: {order.vt_orderid} status: {order.status}")

    def on_trade(self, trade):
        self.write_log(f"Trade: {trade.vt_tradeid} @ {trade.price}")
