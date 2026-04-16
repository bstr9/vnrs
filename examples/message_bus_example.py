"""
Inter-strategy communication using MessageBus.
"""
from trade_engine import Strategy, MessageBus

message_bus = MessageBus()


class SignalGenerator(Strategy):
    """Generates trading signals and broadcasts them."""

    def __init__(self):
        super().__init__()
        self.strategy_name = "SignalGenerator"

    def on_init(self):
        self.write_log("Signal generator initialized")

    def on_bar(self, bar):
        if bar.close_price > bar.open_price * 1.01:
            signal = {"type": "buy", "symbol": bar.vt_symbol, "price": bar.close_price}
            message_bus.publish("signals", signal)
            self.write_log(f"Published buy signal: {signal}")
        elif bar.close_price < bar.open_price * 0.99:
            signal = {"type": "sell", "symbol": bar.vt_symbol, "price": bar.close_price}
            message_bus.publish("signals", signal)
            self.write_log(f"Published sell signal: {signal}")


class SignalConsumer(Strategy):
    """Consumes signals and executes trades."""

    def __init__(self):
        super().__init__()
        self.strategy_name = "SignalConsumer"

    def on_init(self):
        self.write_log("Signal consumer initialized")
        message_bus.subscribe("signals", self.on_signal)

    def on_signal(self, message):
        """Handle incoming signal from MessageBus."""
        self.write_log(f"Received signal: {message}")

        signal_type = message.get("type")
        symbol = message.get("symbol")
        price = message.get("price")

        if signal_type == "buy":
            self.buy(symbol, price, 0.1, stop=False)
        elif signal_type == "sell":
            pos = self.get_pos(symbol)
            if pos > 0:
                self.sell(symbol, price, pos, stop=False)

    def on_bar(self, bar):
        pass  # Signal-driven strategy
