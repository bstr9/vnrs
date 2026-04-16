"""
CtaStrategy — vnpy CtaTemplate compatibility shim

This module provides the `CtaStrategy` class, a Python subclass of the Rust
`Strategy` base class that adds vnpy CtaTemplate compatibility:

- `self.pos` property: net position for the primary instrument
- vnpy-style constructor: ``__init__(self, engine, strategy_name, vt_symbol, setting)``
- ``buy/sell/short/cover`` convenience methods (inherited from Strategy)

Usage:
    # Existing vnpy CTA strategies can inherit CtaStrategy instead of CtaTemplate:
    class MyStrategy(CtaStrategy):
        def on_init(self):
            self.write_log("init")

        def on_bar(self, bar):
            if self.pos == 0:
                self.buy(self.vt_symbol, bar.close_price, 1)

    # New strategies should use the clean Strategy base class directly:
    class MyStrategy(Strategy):
        def on_bar(self, bar):
            net = self.portfolio.net_position("BTCUSDT.BINANCE")
            if net == 0:
                self.buy("BTCUSDT.BINANCE", 50000.0, 1)
"""

from trade_engine import Strategy


class CtaStrategy(Strategy):
    """
    vnpy CtaTemplate compatibility shim.

    Extends the unified `Strategy` base class with:
    - ``self.pos``: Net position for the primary (first) instrument.
      Maps to ``self.get_pos(self.vt_symbol)``.
    - vnpy-style constructor signature:
      ``__init__(self, engine, strategy_name, vt_symbol, setting)``

    This allows existing vnpy CTA strategies to run on vnrs with
    minimal code changes — just change the parent class from
    ``CtaTemplate`` to ``CtaStrategy``.
    """

    def __init__(self, engine, strategy_name, vt_symbol, setting=None):
        """
        vnpy CtaTemplate-compatible constructor.

        Args:
            engine: Engine object (can be None for backtesting; the Rust
                    side will inject the real engine reference).
            strategy_name: Unique strategy instance name.
            vt_symbol: Primary instrument identifier (e.g. "BTCUSDT.BINANCE").
                       For multi-instrument strategies, pass a comma-separated
                       string or override ``vt_symbols`` after init.
            setting: Dict of strategy parameters. Keys are set as instance
                     attributes for vnpy compatibility.
        """
        # Normalize vt_symbol to a list
        if isinstance(vt_symbol, str):
            vt_symbols = [s.strip() for s in vt_symbol.split(",") if s.strip()]
        else:
            vt_symbols = list(vt_symbol) if vt_symbol else []

        # Determine strategy type from the primary symbol's exchange
        # (default to "spot" for crypto, "futures" for Chinese futures)
        strategy_type = "spot"
        if vt_symbols:
            primary = vt_symbols[0]
            if "." in primary:
                exchange = primary.rsplit(".", 1)[-1].upper()
                # Chinese futures exchanges
                futures_exchanges = {"SHFE", "DCE", "CZCE", "CFFEX", "INE", "GFEX"}
                if exchange in futures_exchanges:
                    strategy_type = "futures"
                # Crypto futures
                elif exchange in ("BINANCE_USDM", "BINANCE_COINM"):
                    strategy_type = "futures"

        # Call the Strategy base class constructor
        super().__init__(strategy_name, vt_symbols, strategy_type)

        # Store the primary instrument for self.vt_symbol / self.pos
        self._vt_symbol = vt_symbols[0] if vt_symbols else ""

        # vnpy-compatible: set setting keys as instance attributes
        if setting and isinstance(setting, dict):
            for key, value in setting.items():
                if not hasattr(self, key) or key in setting:
                    setattr(self, key, value)

    # ------------------------------------------------------------------
    # vnpy CtaTemplate compatibility properties
    # ------------------------------------------------------------------

    @property
    def vt_symbol(self):
        """Primary instrument identifier (vnpy compat: single string).

        For multi-instrument strategies, use ``self.vt_symbols`` (plural)
        or ``self.portfolio.net_position(vt_symbol)`` for individual positions.
        """
        return self._vt_symbol

    @vt_symbol.setter
    def vt_symbol(self, value):
        self._vt_symbol = value
        # Keep vt_symbols list in sync
        if value and value not in self.vt_symbols:
            self.vt_symbols.insert(0, value)

    @property
    def pos(self):
        """Net position for the primary instrument (vnpy compat).

        Positive = long, negative = short, 0 = flat.
        Maps to ``self.get_pos(self.vt_symbol)``.

        For multi-instrument strategies, use
        ``self.get_pos("OTHER.EXCHANGE")`` or
        ``self.portfolio.net_position("OTHER.EXCHANGE")``.
        """
        return self.get_pos(self._vt_symbol)

    # ------------------------------------------------------------------
    # Lifecycle overrides — provide vnpy-style logging
    # ------------------------------------------------------------------

    def on_init(self):
        """Default on_init — override in subclass."""
        self.write_log(f"Strategy {self.strategy_name} initialized")

    def on_start(self):
        """Default on_start — override in subclass."""
        self.write_log(f"Strategy {self.strategy_name} started")

    def on_stop(self):
        """Default on_stop — override in subclass."""
        self.write_log(f"Strategy {self.strategy_name} stopped")

    # ------------------------------------------------------------------
    # vnpy compatibility aliases
    # ------------------------------------------------------------------

    def load_bar(self, days, interval="1m", callback=None, use_database=False):
        """Load historical bar data (vnpy compat stub).

        In vnpy, this loads ``days`` of historical bars from the database.
        The vnrs engine provides bars through the backtesting data feed
        automatically, so this is typically a no-op during backtesting.

        For live trading, override to implement data loading logic.
        """
        pass

    def load_tick(self, days, callback=None, use_database=False):
        """Load historical tick data (vnpy compat stub)."""
        pass

    def put_event(self):
        """Update strategy UI (vnpy compat stub).

        vnpy uses this to refresh the strategy monitor UI.
        No-op in vnrs.
        """
        pass

    def sync_pos_from_engine(self, vt_symbol, position):
        """Update internal position tracking from engine push.

        Called by the engine when position data changes.
        """
        self.pos_data[vt_symbol] = position

    def __repr__(self):
        return (
            f"CtaStrategy(name={self.strategy_name!r}, "
            f"vt_symbol={self._vt_symbol!r}, "
            f"pos={self.pos}, "
            f"state={self.state})"
        )
