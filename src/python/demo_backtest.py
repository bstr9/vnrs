"""
Self-contained backtest demonstration script for vnrs.

Tests all three strategy types:
1. Strategy (raw base class)
2. CtaStrategy (vnpy CtaTemplate compatibility)
3. SpotStrategy (spot trading helpers)

No external dependencies beyond trade_engine.
"""

import datetime
import random
import sys
from pathlib import Path

# Add the directory containing cta_strategy.py and spot_strategy.py
sys.path.insert(0, str(Path(__file__).parent))

from trade_engine import (
    PyBacktestingEngine,
    PyBarData,
    Strategy,
)

# Import our local strategy base classes
from cta_strategy import CtaStrategy
from spot_strategy import SpotStrategy


# =============================================================================
# Strategy 1: Simple Moving Average Crossover (raw Strategy base class)
# =============================================================================


class SimpleMAStrategy(Strategy):
    """Simple MA crossover strategy using raw Strategy base class."""

    def __new__(cls, strategy_name, vt_symbols, strategy_type="spot"):
        instance = Strategy.__new__(cls, strategy_name, vt_symbols, strategy_type)
        instance.fast_window = 5
        instance.slow_window = 20
        instance.fast_prices = []
        instance.slow_prices = []
        instance.in_position = False
        return instance

    def __init__(self, strategy_name, vt_symbols, strategy_type="spot"):
        # Don't call super().__init__() - PyO3 handles this via __new__
        pass

    def on_init(self):
        self.write_log(f"SimpleMAStrategy initialized: {self.strategy_name}")

    def on_bar(self, bar):
        """Process bar and make trading decisions."""
        close = bar["close_price"]

        # Update price arrays
        self.fast_prices.append(close)
        self.slow_prices.append(close)

        # Keep only necessary history
        if len(self.fast_prices) > self.fast_window:
            self.fast_prices.pop(0)
        if len(self.slow_prices) > self.slow_window:
            self.slow_prices.pop(0)

        # Wait for enough data
        if len(self.slow_prices) < self.slow_window:
            return

        # Calculate MAs
        fast_ma = sum(self.fast_prices) / len(self.fast_prices)
        slow_ma = sum(self.slow_prices) / len(self.slow_prices)

        vt_symbol = f"{bar['symbol']}.{bar['exchange']}"

        # Trading logic
        if fast_ma > slow_ma and not self.in_position:
            self.buy(vt_symbol, close, 0.1)
            self.in_position = True
            self.write_log(f"BUY signal: fast_ma={fast_ma:.2f} > slow_ma={slow_ma:.2f}")
        elif fast_ma < slow_ma and self.in_position:
            self.sell(vt_symbol, close, 0.1)
            self.in_position = False
            self.write_log(
                f"SELL signal: fast_ma={fast_ma:.2f} < slow_ma={slow_ma:.2f}"
            )


# =============================================================================
# Strategy 2: Bollinger Band Strategy (CtaStrategy for vnpy compatibility)
# =============================================================================


class BollingerStrategy(CtaStrategy):
    """Bollinger Band strategy using CtaStrategy (vnpy compatibility)."""

    def __new__(cls, engine, strategy_name, vt_symbol, setting=None):
        instance = CtaStrategy.__new__(cls, engine, strategy_name, vt_symbol, setting)
        # Default parameters (will be overwritten by setting if provided)
        instance.boll_window = 20
        instance.boll_dev = 2.0
        instance.fixed_size = 1.0
        # Internal state
        instance.prices = []
        instance.inited = False
        return instance

    def __init__(self, engine, strategy_name, vt_symbol, setting=None):
        # CtaStrategy.__init__ handles setting attributes from setting dict
        CtaStrategy.__init__(self, engine, strategy_name, vt_symbol, setting)

    def on_init(self):
        self.write_log(f"BollingerStrategy initialized: {self.strategy_name}")

    def on_bar(self, bar):
        """Process bar and make trading decisions."""
        close = bar["close_price"]

        # Update price array
        self.prices.append(close)
        if len(self.prices) > self.boll_window:
            self.prices.pop(0)

        # Wait for enough data
        if len(self.prices) < self.boll_window:
            return

        if not self.inited:
            self.inited = True
            self.write_log("Bollinger bands ready")

        # Calculate Bollinger Bands
        sma = sum(self.prices) / len(self.prices)
        variance = sum((p - sma) ** 2 for p in self.prices) / len(self.prices)
        std = variance**0.5
        upper = sma + self.boll_dev * std
        lower = sma - self.boll_dev * std

        # Trading logic (use self.pos from CtaStrategy)
        if self.pos == 0:
            if close < lower:
                self.buy(self.vt_symbol, close, self.fixed_size)
                self.write_log(
                    f"BUY at lower band: close={close:.2f}, lower={lower:.2f}"
                )
        else:
            if close > upper:
                self.sell(self.vt_symbol, close, abs(self.pos))
                self.write_log(
                    f"SELL at upper band: close={close:.2f}, upper={upper:.2f}"
                )


# =============================================================================
# Strategy 3: Spot Accumulation Strategy (SpotStrategy)
# =============================================================================


class SpotAccumulationStrategy(SpotStrategy):
    """Dollar-cost averaging spot strategy using SpotStrategy."""

    def __new__(cls, strategy_name, vt_symbols, setting=None):
        instance = SpotStrategy.__new__(cls, strategy_name, vt_symbols, setting)
        # Default parameters
        instance.buy_threshold_pct = -2.0  # Buy when price drops 2%
        instance.sell_threshold_pct = 5.0  # Sell when profit > 5%
        instance.trade_size_pct = 10.0  # Use 10% of equity per trade
        # Internal state
        instance.last_price = None
        instance.entry_price = None
        return instance

    def __init__(self, strategy_name, vt_symbols, setting=None):
        # SpotStrategy.__init__ handles setting attributes from setting dict
        SpotStrategy.__init__(self, strategy_name, vt_symbols, setting)

    def on_init(self):
        self.write_log(f"SpotAccumulationStrategy initialized: {self.strategy_name}")

    def on_bar(self, bar):
        """Process bar and make trading decisions."""
        close = bar["close_price"]
        vt_symbol = f"{bar['symbol']}.{bar['exchange']}"

        # First bar - just record price
        if self.last_price is None:
            self.last_price = close
            return

        # Calculate price change
        price_change_pct = ((close - self.last_price) / self.last_price) * 100
        self.last_price = close

        # Get current position
        qty = self.get_quantity(vt_symbol)

        # Buy on dips
        if price_change_pct <= self.buy_threshold_pct:
            if self.portfolio is not None:
                qty_to_buy = self.percent_of_equity(self.trade_size_pct, close)
                if qty_to_buy > 0:
                    self.buy(vt_symbol, close, qty_to_buy)
                    self.entry_price = close
                    self.write_log(
                        f"BUY on dip: {price_change_pct:.2f}%, qty={qty_to_buy:.4f}"
                    )

        # Sell on profit
        elif qty > 0 and self.entry_price is not None:
            profit_pct = ((close - self.entry_price) / self.entry_price) * 100
            if profit_pct >= self.sell_threshold_pct:
                self.sell(vt_symbol, close, qty)
                self.write_log(f"SELL on profit: {profit_pct:.2f}%")
                self.entry_price = None


# =============================================================================
# Backtest Runner
# =============================================================================


def generate_mock_bars(n_bars=1000, base_price=40000.0, volatility=0.02):
    """Generate realistic-looking mock bar data."""
    bars = []
    price = base_price
    start_dt = datetime.datetime(2024, 1, 1, tzinfo=datetime.timezone.utc)

    for i in range(n_bars):
        # Random walk with trend and mean reversion
        trend = 0.0001 * (i % 100 - 50)  # Oscillating trend
        noise = random.gauss(0, volatility)
        change = trend + noise

        price = price * (1 + change)
        price = max(price, base_price * 0.5)  # Floor
        price = min(price, base_price * 1.5)  # Ceiling

        # Generate OHLC
        high_mult = 1 + abs(random.gauss(0, 0.005))
        low_mult = 1 - abs(random.gauss(0, 0.005))

        open_price = price * (1 + random.gauss(0, 0.001))
        close_price = price
        high_price = max(open_price, close_price) * high_mult
        low_price = min(open_price, close_price) * low_mult

        dt = start_dt + datetime.timedelta(minutes=15 * i)

        bar = PyBarData(
            gateway_name="BACKTESTING",
            symbol="BTCUSDT",
            exchange="BINANCE",
            datetime=dt.isoformat(),
            interval="15m",
            open_price=open_price,
            high_price=high_price,
            low_price=low_price,
            close_price=close_price,
            volume=random.uniform(100, 1000),
        )
        bars.append(bar)

    return bars


def run_backtest(strategy_class, strategy_name, strategy_args):
    """Run a single backtest and return statistics."""
    print(f"\n{'=' * 60}")
    print(f"Running backtest: {strategy_name}")
    print(f"{'=' * 60}")

    # Create engine
    engine = PyBacktestingEngine()

    # Set parameters
    engine.set_parameters(
        vt_symbol="BTCUSDT.BINANCE",
        interval="15m",
        start="2024-01-01",
        end="2024-02-01",
        rate=0.001,  # 0.1% commission
        slippage=0.01,  # $0.01 slippage
        size=1.0,  # Contract size
        pricetick=0.01,  # Price tick
        capital=100000.0,  # Starting capital
        mode="bar",
    )

    # Generate mock data
    print("Generating mock bar data...")
    bars = generate_mock_bars(n_bars=1000, base_price=40000.0, volatility=0.02)
    print(f"  Generated {len(bars)} bars")

    # Load data
    print("Loading history data...")
    engine.set_history_data(bars)

    # Add strategy
    print("Adding strategy...")
    engine.add_strategy(
        strategy_class(*strategy_args), strategy_name, ["BTCUSDT.BINANCE"]
    )

    # Run backtest
    print("Running backtest...")
    try:
        engine.run_backtesting()
    except Exception as e:
        print(f"  Error during backtest: {e}")
        return None

    # Calculate statistics
    print("Calculating statistics...")
    stats = engine.calculate_statistics(True)

    return stats


def main():
    """Run all backtests."""
    print("\n" + "=" * 60)
    print("VNRS Python Strategy Backtest Demo")
    print("=" * 60)

    results = {}

    # Test 1: Simple MA Strategy (raw Strategy base class)
    try:
        stats = run_backtest(
            SimpleMAStrategy,
            "SimpleMA_v1",
            ("SimpleMA_v1", ["BTCUSDT.BINANCE"], "spot"),
        )
        if stats:
            results["SimpleMA"] = stats
    except Exception as e:
        print(f"SimpleMA Strategy failed: {e}")

    # Test 2: Bollinger Strategy (CtaStrategy vnpy compatibility)
    try:
        stats = run_backtest(
            BollingerStrategy,
            "Bollinger_v1",
            (
                None,
                "Bollinger_v1",
                "BTCUSDT.BINANCE",
                {"boll_window": 20, "boll_dev": 2.0},
            ),
        )
        if stats:
            results["Bollinger"] = stats
    except Exception as e:
        print(f"Bollinger Strategy failed: {e}")

    # Test 3: Spot Accumulation Strategy
    try:
        stats = run_backtest(
            SpotAccumulationStrategy,
            "SpotAccum_v1",
            (
                "SpotAccum_v1",
                ["BTCUSDT.BINANCE"],
                {"buy_threshold_pct": -2.0, "sell_threshold_pct": 5.0},
            ),
        )
        if stats:
            results["SpotAccum"] = stats
    except Exception as e:
        print(f"SpotAccumulation Strategy failed: {e}")

    # Print summary
    print("\n" + "=" * 60)
    print("BACKTEST RESULTS SUMMARY")
    print("=" * 60)

    if not results:
        print("\nNo successful backtests!")
        return

    # Header
    print(
        f"\n{'Strategy':<20} {'Total PnL':>12} {'Sharpe':>10} {'Max DD':>12} {'End Balance':>14}"
    )
    print("-" * 68)

    for name, stats in results.items():
        stats_dict = stats.to_dict()
        trade_count = stats_dict.get("total_trade_count", "N/A")
        print(
            f"{name:<20} ${stats.total_net_pnl:>10,.2f} {stats.sharpe_ratio:>10.2f} ${stats.max_drawdown:>10,.2f} ${stats.end_balance:>12,.2f}"
        )

    print("\n" + "=" * 60)
    print("DETAILED STATISTICS")
    print("=" * 60)

    for name, stats in results.items():
        stats_dict = stats.to_dict()
        print(f"\n--- {name} ---")
        print(f"  Start Date:      {stats.start_date}")
        print(f"  End Date:        {stats.end_date}")
        print(f"  Total Days:      {stats.total_days}")
        print(f"  Profit Days:     {stats.profit_days}")
        print(f"  Loss Days:       {stats.loss_days}")
        print(f"  End Balance:     ${stats.end_balance:,.2f}")
        print(
            f"  Max Drawdown:    ${stats.max_drawdown:,.2f} ({stats.max_drawdown_percent:.2f}%)"
        )
        print(f"  Total Net PnL:   ${stats.total_net_pnl:,.2f}")
        print(f"  Sharpe Ratio:    {stats.sharpe_ratio:.2f}")
        print(f"  Return Mean:     {stats.return_mean:.4f}")
        for key in [
            "total_commission",
            "total_slippage",
            "total_turnover",
            "total_trade_count",
        ]:
            if key in stats_dict:
                print(f"  {key:<18}{stats_dict[key]}")

    print("\n" + "=" * 60)
    print("Backtest demo completed successfully!")
    print("=" * 60)


if __name__ == "__main__":
    main()
