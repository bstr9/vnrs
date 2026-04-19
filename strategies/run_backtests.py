#!/usr/bin/env python3
"""
Backtest runner for all strategies in the strategies directory.

This script loads each strategy, runs a simple backtest with synthetic data,
and reports the Sharpe Ratio and Max Drawdown.
"""

import sys
import os
import importlib.util
from pathlib import Path
from datetime import datetime, timedelta
import random

# Add strategies directory to path
STRATEGIES_DIR = Path(__file__).parent


def load_strategy_class(strategy_file: Path):
    """Load a strategy class from a Python file."""
    spec = importlib.util.spec_from_file_location(strategy_file.stem, strategy_file)
    module = importlib.util.module_from_spec(spec)
    sys.modules[strategy_file.stem] = module
    spec.loader.exec_module(module)
    
    # Find the strategy class (subclass of CtaStrategy)
    from trade_engine import Strategy
    for name in dir(module):
        obj = getattr(module, name)
        if isinstance(obj, type) and issubclass(obj, Strategy) and obj is not Strategy:
            return obj, name
    return None, None


def generate_synthetic_bars(num_bars: int = 500, start_price: float = 50000.0):
    """Generate synthetic OHLCV bar data using PyBarData for backtesting."""
    from trade_engine import PyBarData
    
    bars = []
    price = start_price
    base_time = datetime(2024, 1, 1, 0, 0, 0)
    
    for i in range(num_bars):
        # Random walk with slight drift
        change_pct = random.gauss(0.0001, 0.02)  # 0.01% drift, 2% volatility
        open_price = price
        close_price = price * (1 + change_pct)
        high_price = max(open_price, close_price) * (1 + abs(random.gauss(0, 0.005)))
        low_price = min(open_price, close_price) * (1 - abs(random.gauss(0, 0.005)))
        volume = random.uniform(100, 1000)
        
        dt = base_time + timedelta(minutes=i)
        # PyBarData expects datetime as RFC3339 string
        dt_str = dt.strftime("%Y-%m-%dT%H:%M:%S+00:00")
        
        bar = PyBarData(
            gateway_name="BACKTESTING",
            symbol="BTCUSDT",
            exchange="BINANCE",
            datetime=dt_str,
            interval="1m",
            open_price=open_price,
            high_price=high_price,
            low_price=low_price,
            close_price=close_price,
            volume=volume,
        )
        bars.append(bar)
        price = close_price
    
    return bars


def run_backtest_for_strategy(strategy_class, strategy_name: str, bars: list):
    """Run a backtest for a single strategy and return statistics."""
    from trade_engine import PyBacktestingEngine
    
    try:
        # Create backtesting engine
        engine = PyBacktestingEngine()
        engine.set_parameters(
            vt_symbol="BTCUSDT.BINANCE",
            interval="1m",
            start="2024-01-01",
            end="2024-01-02",
            rate=0.0003,  # 0.03% commission
            slippage=0.0001,  # 0.01% slippage
            size=1.0,
            pricetick=0.01,
            capital=100000.0
        )
        
        # Load historical data
        engine.set_history_data(bars)
        
        # Create strategy instance (handle different constructor signatures)
        strategy = None
        try:
            # Try vnrs-style constructor (strategy_name, vt_symbols)
            strategy = strategy_class(strategy_name, ["BTCUSDT.BINANCE"])
        except TypeError:
            try:
                # Try no-arg constructor
                strategy = strategy_class()
            except TypeError:
                pass
        
        if strategy is None:
            return {
                "success": False,
                "sharpe_ratio": 0.0,
                "max_drawdown": 0.0,
                "total_return": 0.0,
                "total_trades": 0,
                "error": "Could not create strategy instance"
            }
        
        # Add strategy with explicit name and symbols
        engine.add_strategy(strategy, strategy_name, ["BTCUSDT.BINANCE"])
        
        # Run backtest
        result = engine.run_backtesting()
        
        # Calculate statistics — PyBacktestingStatistics is a PyO3 object, use to_dict()
        stats = engine.calculate_statistics(output=False)
        stats_dict = stats.to_dict()
        
        return {
            "success": True,
            "sharpe_ratio": stats_dict.get("sharpe_ratio", 0.0),
            "max_drawdown_percent": stats_dict.get("max_drawdown_percent", 0.0),  # Already in % (0-100)
            "total_net_pnl": stats_dict.get("total_net_pnl", 0.0),  # Absolute dollar PnL
            "total_trade_count": stats_dict.get("total_trade_count", 0),
            "end_balance": stats_dict.get("end_balance", 0.0),
            "start_capital": 100000.0,  # Known from set_parameters
            "error": None
        }
    except Exception as e:
        return {
            "success": False,
            "sharpe_ratio": 0.0,
            "max_drawdown": 0.0,
            "total_return": 0.0,
            "total_trades": 0,
            "error": str(e)
        }


def main():
    """Main entry point."""
    print("=" * 60)
    print("vnrs Strategy Backtest Runner")
    print("=" * 60)
    
    # Find all strategy files
    strategy_files = list(STRATEGIES_DIR.glob("*_strategy.py"))
    
    if not strategy_files:
        print("No strategy files found in:", STRATEGIES_DIR)
        return
    
    print(f"\nFound {len(strategy_files)} strategy files:")
    for f in strategy_files:
        print(f"  - {f.name}")
    
    # Generate synthetic data
    print("\nGenerating synthetic bar data...")
    random.seed(42)  # Deterministic for reproducibility
    bars = generate_synthetic_bars(num_bars=500)
    print(f"  Generated {len(bars)} bars")
    
    # Run backtests
    results = []
    print("\nRunning backtests...")
    print("-" * 60)
    
    for strategy_file in strategy_files:
        print(f"\nTesting: {strategy_file.stem}")
        
        try:
            strategy_class, class_name = load_strategy_class(strategy_file)
            if strategy_class is None:
                print(f"  ERROR: No strategy class found")
                results.append({
                    "file": strategy_file.stem,
                    "class": class_name or "N/A",
                    "success": False,
                    "error": "No strategy class found"
                })
                continue
            
            result = run_backtest_for_strategy(strategy_class, class_name, bars)
            results.append({
                "file": strategy_file.stem,
                "class": class_name,
                **result
            })
            
            if result["success"]:
                pnl_pct = (result["total_net_pnl"] / result["start_capital"] * 100) if result["start_capital"] > 0 else 0
                print(f"  Sharpe Ratio: {result['sharpe_ratio']:.4f}")
                print(f"  Max Drawdown: {result['max_drawdown_percent']:.2f}%")
                print(f"  Total PnL: ${result['total_net_pnl']:.2f} ({pnl_pct:+.2f}%)")
                print(f"  End Balance: ${result['end_balance']:.2f}")
                print(f"  Total Trades: {result['total_trade_count']}")
            else:
                print(f"  ERROR: {result['error']}")
                
        except Exception as e:
            print(f"  ERROR: {e}")
            results.append({
                "file": strategy_file.stem,
                "class": "N/A",
                "success": False,
                "error": str(e)
            })
    
    # Summary
    print("\n" + "=" * 60)
    print("SUMMARY")
    print("=" * 60)
    
    successful = [r for r in results if r.get("success")]
    failed = [r for r in results if not r.get("success")]
    
    print(f"\nSuccessful: {len(successful)}/{len(results)}")
    print(f"Failed: {len(failed)}/{len(results)}")
    
    if failed:
        print("\nFailed strategies:")
        for r in failed:
            print(f"  - {r['file']}: {r.get('error', 'Unknown error')}")
    
    if successful:
        print("\nResults table:")
        print(f"{'Strategy':<30} {'Sharpe':>10} {'MaxDD%':>10} {'PnL$':>12} {'Trades':>8}")
        print("-" * 72)
        for r in sorted(successful, key=lambda x: x.get('sharpe_ratio', 0), reverse=True):
            pnl = r['total_net_pnl']
            print(f"{r['file']:<30} {r['sharpe_ratio']:>10.4f} {r['max_drawdown_percent']:>10.2f} {pnl:>12.2f} {r['total_trade_count']:>8}")
    
    # Write results to backtest_log.md
    log_file = STRATEGIES_DIR.parent / "backtest_log.md"
    print(f"\nAppending results to {log_file}")
    
    with open(log_file, "a", encoding="utf-8") as f:
        f.write(f"\n## Python Strategies Backtest - {datetime.now().strftime('%Y-%m-%d %H:%M')}\n\n")
        f.write(f"**Data**: Synthetic (500 bars, 2% volatility, seed=42)\n\n")
        
        if successful:
            f.write("| Strategy | Sharpe Ratio | Max DD% | PnL ($) | Trades |\n")
            f.write("|----------|-------------|---------|---------|--------|\n")
            for r in successful:
                pnl = r['total_net_pnl']
                f.write(f"| {r['file']} | {r['sharpe_ratio']:.4f} | {r['max_drawdown_percent']:.2f}% | ${pnl:.2f} | {r['total_trade_count']} |\n")
        else:
            f.write("No successful backtests.\n")
        
        if failed:
            f.write(f"\n**Failed strategies**: {len(failed)}\n")
            for r in failed:
                f.write(f"- {r['file']}: {r.get('error', 'Unknown error')}\n")
    
    print("Done!")


if __name__ == "__main__":
    main()
