"""
SpotStrategy — base class for spot (cash) trading strategies

This module provides the `SpotStrategy` class, a Python subclass of the Rust
`Strategy` base class that adds spot-specific semantics:

- No short/cover (spot is long-only)
- Portfolio-aware: ``self.portfolio`` provides balance/positions
- Percentage-based position sizing helpers
- Multi-instrument support built-in
- Cash management: available balance, buying power

Usage:
    class MySpotStrategy(SpotStrategy):
        def on_init(self):
            self.write_log("init")

        def on_bar(self, bar):
            if self.get_quantity("BTCUSDT.BINANCE") == 0:
                qty = self.percent_of_equity(10, bar.close_price)
                self.buy_market("BTCUSDT.BINANCE", qty)
"""

from trade_engine import Strategy


class SpotStrategy(Strategy):
    """
    Base class for spot (cash) trading strategies.

    Key differences from CtaStrategy:

    - No short/cover (spot is long-only)
    - Portfolio-aware: ``self.portfolio`` provides balance/positions
    - Percentage-based position sizing helpers
    - Multi-instrument support built-in
    - Cash management: available balance, buying power

    Constructor:
        ``__init__(self, strategy_name, vt_symbols, setting=None)``
    """

    def __new__(cls, strategy_name, vt_symbols, setting=None):
        """
        SpotStrategy constructor.

        Args:
            strategy_name: Unique strategy instance name.
            vt_symbols: List of instrument identifiers (e.g. ["BTCUSDT.BINANCE"])
                        or a single string for one instrument.
            setting: Dict of strategy parameters. Keys are set as instance
                     attributes for vnpy compatibility.
        """
        # Normalize vt_symbols to a list
        if isinstance(vt_symbols, str):
            vt_symbols = [s.strip() for s in vt_symbols.split(",") if s.strip()]
        else:
            vt_symbols = list(vt_symbols) if vt_symbols else []

        # Call PyO3 Strategy.__new__ to create the instance with "spot" type
        instance = Strategy.__new__(cls, strategy_name, vt_symbols, "spot")
        instance._setting = setting

        return instance

    def __init__(self, strategy_name, vt_symbols, setting=None):
        # Don't call super().__init__() - PyO3 handles this via __new__
        # vnpy-compatible: set setting keys as instance attributes
        if self._setting and isinstance(self._setting, dict):
            for key, value in self._setting.items():
                setattr(self, key, value)
        # Clean up temporary attrs
        delattr(self, "_setting")

    # ------------------------------------------------------------------
    # Position helpers
    # ------------------------------------------------------------------

    def get_quantity(self, vt_symbol):
        """Return quantity held for a symbol (always >= 0 for spot).

        Uses ``self.portfolio.position(vt_symbol).quantity`` if available,
        falls back to ``self.portfolio.net_position(vt_symbol)`` clamped to 0.

        Returns 0.0 if portfolio is not available or no position exists.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
        """
        if self.portfolio is None:
            return 0.0
        pos = self.portfolio.position(vt_symbol)
        if pos is not None:
            return max(pos.quantity, 0.0)
        # Fallback: net position clamped to non-negative
        net = self.portfolio.net_position(vt_symbol)
        return max(net, 0.0)

    def get_avg_price(self, vt_symbol):
        """Return average entry price for a position.

        Returns 0.0 if portfolio is not available or no position exists.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
        """
        if self.portfolio is None:
            return 0.0
        pos = self.portfolio.position(vt_symbol)
        if pos is not None:
            return pos.avg_price
        return 0.0

    def get_unrealized_pnl(self, vt_symbol):
        """Return unrealized PnL for a position.

        Returns 0.0 if portfolio is not available or no position exists.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
        """
        if self.portfolio is None:
            return 0.0
        pos = self.portfolio.position(vt_symbol)
        if pos is not None:
            return pos.unrealized_pnl
        return 0.0

    def get_position_value(self, vt_symbol):
        """Return current market value of a position.

        Uses quantity * avg_price as a fallback when mark_price is not
        available. Returns 0.0 if portfolio is not available or no position
        exists.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
        """
        qty = self.get_quantity(vt_symbol)
        if qty == 0.0:
            return 0.0
        avg_price = self.get_avg_price(vt_symbol)
        return qty * avg_price

    # ------------------------------------------------------------------
    # Cash management helpers
    # ------------------------------------------------------------------

    @property
    def available_balance(self):
        """Available (unfrozen) balance from portfolio.

        Returns 0.0 if portfolio is not available.
        """
        if self.portfolio is None:
            return 0.0
        return self.portfolio.available

    @property
    def total_equity(self):
        """Total equity (balance + unrealized PnL) from portfolio.

        Returns 0.0 if portfolio is not available.
        """
        if self.portfolio is None:
            return 0.0
        return self.portfolio.equity

    def buying_power(self, price):
        """Return max quantity affordable at given price.

        Calculated as ``self.portfolio.available / price``.
        Returns 0.0 if portfolio is not available or price <= 0.

        Args:
            price: Current price per unit
        """
        if self.portfolio is None or price <= 0:
            return 0.0
        return self.portfolio.available / price

    def can_afford(self, price, quantity):
        """Check if the strategy can afford to buy the given quantity.

        Returns True if ``price * quantity <= self.portfolio.available``.
        Returns False if portfolio is not available or price/quantity invalid.

        Args:
            price: Price per unit
            quantity: Number of units to buy
        """
        if self.portfolio is None or price <= 0 or quantity <= 0:
            return False
        return price * quantity <= self.portfolio.available

    # ------------------------------------------------------------------
    # Position sizing helpers
    # ------------------------------------------------------------------

    def percent_of_equity(self, percent, price):
        """Return quantity such that quantity * price = equity * percent/100.

        Args:
            percent: Percentage of equity to allocate (e.g. 10 for 10%)
            price: Current price per unit

        Returns:
            Quantity (floored to avoid over-allocation). Returns 0.0 if
            price <= 0 or portfolio is not available.
        """
        if self.portfolio is None or price <= 0:
            return 0.0
        equity = self.portfolio.equity
        return (equity * percent / 100.0) / price

    def percent_of_balance(self, percent, price):
        """Return quantity such that quantity * price = balance * percent/100.

        Args:
            percent: Percentage of available balance to allocate (e.g. 10 for 10%)
            price: Current price per unit

        Returns:
            Quantity (floored to avoid over-allocation). Returns 0.0 if
            price <= 0 or portfolio is not available.
        """
        if self.portfolio is None or price <= 0:
            return 0.0
        balance = self.portfolio.available
        return (balance * percent / 100.0) / price

    def risk_based_size(self, risk_percent, entry_price, stop_price):
        """Return quantity based on risk per trade.

        Calculated as ``equity * risk_percent / abs(entry_price - stop_price)``.

        Args:
            risk_percent: Percentage of equity to risk (e.g. 1 for 1%)
            entry_price: Intended entry price
            stop_price: Stop-loss price

        Returns:
            Quantity. Returns 0.0 if entry_price == stop_price, prices are
            invalid, or portfolio is not available.
        """
        if self.portfolio is None:
            return 0.0
        risk_per_unit = abs(entry_price - stop_price)
        if risk_per_unit == 0.0 or entry_price <= 0 or stop_price <= 0:
            return 0.0
        equity = self.portfolio.equity
        return (equity * risk_percent / 100.0) / risk_per_unit

    # ------------------------------------------------------------------
    # Order helpers — enforce spot-only semantics
    # ------------------------------------------------------------------

    def short(self, vt_symbol, price, volume):
        """Not supported for spot strategies.

        Raises:
            NotImplementedError: Spot strategies cannot short.
        """
        raise NotImplementedError(
            "Spot strategies cannot short. Use Strategy base class for futures."
        )

    def cover(self, vt_symbol, price, volume):
        """Not supported for spot strategies.

        Raises:
            NotImplementedError: Spot strategies cannot cover.
        """
        raise NotImplementedError(
            "Spot strategies cannot cover. Use Strategy base class for futures."
        )

    # ------------------------------------------------------------------
    # Convenience buy/sell methods via OrderFactory
    # ------------------------------------------------------------------

    def buy_market(self, vt_symbol, quantity):
        """Create and submit a market buy order.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
            quantity: Number of units to buy

        Returns:
            List of vt_orderid strings on success, empty list if
            order_factory is not available.
        """
        if self.order_factory is None:
            return []
        return self.order_factory.market(vt_symbol, quantity, "BUY").submit()

    def sell_market(self, vt_symbol, quantity):
        """Create and submit a market sell order.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
            quantity: Number of units to sell

        Returns:
            List of vt_orderid strings on success, empty list if
            order_factory is not available.
        """
        if self.order_factory is None:
            return []
        return self.order_factory.market(vt_symbol, quantity, "SELL").submit()

    def buy_limit(self, vt_symbol, price, quantity):
        """Create and submit a limit buy order.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
            price: Limit price
            quantity: Number of units to buy

        Returns:
            List of vt_orderid strings on success, empty list if
            order_factory is not available.
        """
        if self.order_factory is None:
            return []
        return self.order_factory.limit(vt_symbol, price, quantity, "BUY").submit()

    def sell_limit(self, vt_symbol, price, quantity):
        """Create and submit a limit sell order.

        Args:
            vt_symbol: Instrument identifier (e.g. "BTCUSDT.BINANCE")
            price: Limit price
            quantity: Number of units to sell

        Returns:
            List of vt_orderid strings on success, empty list if
            order_factory is not available.
        """
        if self.order_factory is None:
            return []
        return self.order_factory.limit(vt_symbol, price, quantity, "SELL").submit()

    # ------------------------------------------------------------------
    # Lifecycle overrides — provide spot-style logging
    # ------------------------------------------------------------------

    def on_init(self):
        """Default on_init — override in subclass."""
        self.write_log(f"SpotStrategy {self.strategy_name} initialized")

    def on_start(self):
        """Default on_start — override in subclass."""
        self.write_log(f"SpotStrategy {self.strategy_name} started")

    def on_stop(self):
        """Default on_stop — override in subclass."""
        self.write_log(f"SpotStrategy {self.strategy_name} stopped")

    # ------------------------------------------------------------------
    # Representation
    # ------------------------------------------------------------------

    def __repr__(self):
        balance = self.available_balance
        n_positions = 0
        if self.portfolio is not None:
            n_positions = len(self.portfolio.positions)
        return (
            f"SpotStrategy(name={self.strategy_name!r}, "
            f"symbols={self.vt_symbols!r}, "
            f"balance={balance:.2f}, "
            f"positions={n_positions})"
        )
