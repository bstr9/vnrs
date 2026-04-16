"""
Multi-symbol strategy using synchronized bar generation.
"""
from trade_engine import Strategy, SyncBarGenerator


class MultiSymbolStrategy(Strategy):
    """Strategy trading multiple correlated symbols."""

    def __init__(self):
        super().__init__()
        self.vt_symbols = ["BTCUSDT.BINANCE", "ETHUSDT.BINANCE"]
        self.sync_bars = None

    def on_init(self):
        self.write_log("Multi-symbol strategy initialized")
        self.sync_bars = SyncBarGenerator(self.vt_symbols)

    def on_start(self):
        self.write_log("Multi-symbol strategy started")

    def on_bar(self, bar):
        self.sync_bars.update_bar(bar.vt_symbol, bar)

        event = self.sync_bars.get_synchronized_bars()
        if event:
            self.on_synchronized_bars(event)

    def on_synchronized_bars(self, event):
        """Called when all symbols have bars at same timestamp."""
        btc_bar = event.get_bar("BTCUSDT.BINANCE")
        eth_bar = event.get_bar("ETHUSDT.BINANCE")

        self.write_log(f"Synchronized bars: BTC={btc_bar.close_price}, ETH={eth_bar.close_price}")

        btc_change = (btc_bar.close_price - btc_bar.open_price) / btc_bar.open_price
        eth_change = (eth_bar.close_price - eth_bar.open_price) / eth_bar.open_price

        # Trade the spread: buy ETH when BTC outperforms by 0.1%+
        if btc_change > eth_change + 0.001:
            self.buy("ETHUSDT.BINANCE", eth_bar.close_price, 0.1, stop=False)
