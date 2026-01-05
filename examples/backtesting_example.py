"""
Backtesting Example - Python CTA Strategy Backtesting

This example demonstrates how to backtest a Python strategy using the Rust engine.
Similar to vnpy_ctabacktester but with Rust performance.
"""

from datetime import datetime, timedelta
from typing import List
import sys
sys.path.append("../examples")

# Import strategy template from previous example
from strategy_example import CtaTemplate, DoubleMaStrategy, GridStrategy


class BacktestingExample:
    """
    Example showing how to backtest strategies
    """
    
    def __init__(self):
        # Placeholder for Rust backtesting engine
        # In production: from trade_engine import PyBacktestingEngine
        self.engine = None
    
    def run_double_ma_backtest(self):
        """Run Double MA strategy backtest"""
        print("=" * 60)
        print("Double MA Strategy Backtesting")
        print("=" * 60)
        
        # Create strategy
        strategy = DoubleMaStrategy(
            strategy_name="DMA_BACKTEST",
            vt_symbols=["BTCUSDT.BINANCE"],
            fast_window=10,
            slow_window=20,
            fixed_size=0.01
        )
        
        # Set backtesting parameters
        params = {
            "vt_symbol": "BTCUSDT.BINANCE",
            "interval": "1m",
            "start": "2024-01-01",
            "end": "2024-12-31",
            "rate": 0.0003,      # 0.03% commission
            "slippage": 0.5,     # $0.5 slippage
            "size": 1.0,         # Spot trading
            "pricetick": 0.01,
            "capital": 100000.0,  # $100k
        }
        
        print(f"\nBacktest Parameters:")
        print(f"  Symbol: {params['vt_symbol']}")
        print(f"  Period: {params['start']} to {params['end']}")
        print(f"  Initial Capital: ${params['capital']:,.2f}")
        print(f"  Commission Rate: {params['rate']*100:.3f}%")
        print(f"  Slippage: ${params['slippage']}")
        
        # Generate sample data
        bars = self.generate_sample_bars(params['start'], params['end'])
        print(f"\nGenerated {len(bars)} bars for backtesting")
        
        # Initialize strategy
        strategy.on_init()
        strategy.on_start()
        
        # Run backtest (simplified version)
        print("\nRunning backtest...")
        self.run_simplified_backtest(strategy, bars)
        
        # Statistics
        print("\n" + "=" * 60)
        print("Backtesting Results")
        print("=" * 60)
        
        stats = self.calculate_simple_stats(strategy)
        self.print_statistics(stats)
        
        strategy.on_stop()
    
    def run_grid_backtest(self):
        """Run Grid strategy backtest"""
        print("\n" + "=" * 60)
        print("Grid Strategy Backtesting")
        print("=" * 60)
        
        # Create strategy
        strategy = GridStrategy(
            strategy_name="GRID_BACKTEST",
            vt_symbols=["BTCUSDT.BINANCE"],
            grid_size=100.0,
            grid_num=5,
            order_size=0.01
        )
        
        # Set backtesting parameters
        params = {
            "vt_symbol": "BTCUSDT.BINANCE",
            "interval": "5m",
            "start": "2024-01-01",
            "end": "2024-03-31",  # Shorter period for grid
            "rate": 0.0003,
            "slippage": 0.5,
            "size": 1.0,
            "pricetick": 0.01,
            "capital": 50000.0,   # $50k
        }
        
        print(f"\nBacktest Parameters:")
        print(f"  Symbol: {params['vt_symbol']}")
        print(f"  Period: {params['start']} to {params['end']}")
        print(f"  Initial Capital: ${params['capital']:,.2f}")
        print(f"  Grid Size: ${strategy.grid_size}")
        print(f"  Grid Number: {strategy.grid_num}")
        
        # Generate sample data
        bars = self.generate_sample_bars(params['start'], params['end'], oscillating=True)
        print(f"\nGenerated {len(bars)} bars for backtesting")
        
        # Initialize strategy
        strategy.on_init()
        strategy.on_start()
        
        # Run backtest
        print("\nRunning backtest...")
        self.run_simplified_backtest(strategy, bars)
        
        # Statistics
        print("\n" + "=" * 60)
        print("Backtesting Results")
        print("=" * 60)
        
        stats = self.calculate_simple_stats(strategy)
        self.print_statistics(stats)
        
        strategy.on_stop()
    
    def generate_sample_bars(self, start: str, end: str, oscillating: bool = False):
        """Generate sample bar data for backtesting"""
        bars = []
        
        start_dt = datetime.strptime(start, "%Y-%m-%d")
        end_dt = datetime.strptime(end, "%Y-%m-%d")
        
        current_dt = start_dt
        base_price = 50000.0
        
        i = 0
        while current_dt <= end_dt:
            # Generate price movement
            if oscillating:
                # Oscillating pattern for grid strategy
                price_change = 200 * (i % 10 - 5)  # Oscillate ±1000
            else:
                # Trending pattern for MA strategy
                trend = 10 if i % 100 < 50 else -10
                noise = (i % 17 - 8) * 5
                price_change = trend + noise
            
            close = base_price + price_change
            open_price = close - 5
            high = close + 10
            low = close - 10
            
            bar = {
                'symbol': 'BTCUSDT',
                'exchange': 'BINANCE',
                'datetime': current_dt,
                'interval': '1m',
                'open': open_price,
                'high': high,
                'low': low,
                'close': close,
                'volume': 100.0 + (i % 50),
            }
            
            bars.append(bar)
            
            # Update for next iteration
            base_price = close
            current_dt += timedelta(minutes=1)
            i += 1
        
        return bars
    
    def run_simplified_backtest(self, strategy: CtaTemplate, bars: List[dict]):
        """
        Run simplified backtest (without Rust engine)
        In production, this would call the Rust backtesting engine
        """
        # Load bars into strategy
        strategy.bars = []
        
        for bar in bars:
            # Process bar
            strategy.on_bar(bar)
            
            # Show progress every 1000 bars
            if len(strategy.bars) % 1000 == 0:
                print(f"  Processed {len(strategy.bars)} bars...")
    
    def calculate_simple_stats(self, strategy: CtaTemplate) -> dict:
        """Calculate simple statistics"""
        # This is a placeholder - in production would use Rust engine
        return {
            'total_days': len(strategy.bars) if hasattr(strategy, 'bars') else 0,
            'total_bars': len(strategy.bars) if hasattr(strategy, 'bars') else 0,
            'fast_ma': getattr(strategy, 'fast_ma', 0.0),
            'slow_ma': getattr(strategy, 'slow_ma', 0.0),
            'position': strategy.get_pos(strategy.vt_symbols[0]) if strategy.vt_symbols else 0.0,
        }
    
    def print_statistics(self, stats: dict):
        """Print statistics"""
        print(f"\nTotal Bars: {stats['total_bars']}")
        print(f"Final Position: {stats['position']}")
        
        if 'fast_ma' in stats:
            print(f"Fast MA: {stats['fast_ma']:.2f}")
            print(f"Slow MA: {stats['slow_ma']:.2f}")


def run_with_rust_engine():
    """
    Example of using Rust backtesting engine (requires compilation)
    """
    print("=" * 60)
    print("Using Rust Backtesting Engine")
    print("=" * 60)
    
    # NOTE: This requires compiling with Python bindings
    # cargo build --release --features python
    
    try:
        from trade_engine import PyBacktestingEngine, PyBarData
        
        # Create engine
        engine = PyBacktestingEngine()
        
        # Set parameters
        engine.set_parameters(
            vt_symbol="BTCUSDT.BINANCE",
            interval="1m",
            start="2024-01-01",
            end="2024-12-31",
            rate=0.0003,
            slippage=0.5,
            size=1.0,
            pricetick=0.01,
            capital=100000.0,
            mode="bar"
        )
        
        # Generate sample bars
        bars = []
        start_dt = datetime(2024, 1, 1)
        base_price = 50000.0
        
        for i in range(1000):
            dt = start_dt + timedelta(minutes=i)
            price = base_price + (i % 100 - 50) * 10
            
            bar = PyBarData(
                symbol="BTCUSDT",
                exchange="BINANCE",
                datetime=dt.isoformat(),
                interval="1m",
                open=price - 5,
                high=price + 10,
                low=price - 10,
                close=price,
                volume=100.0
            )
            bars.append(bar)
        
        # Set history data
        engine.set_history_data(bars)
        
        # Calculate statistics
        stats = engine.calculate_statistics(output=True)
        
        print("\nBacktesting completed successfully!")
        print(f"End Balance: ${stats.end_balance:,.2f}")
        print(f"Sharpe Ratio: {stats.sharpe_ratio:.4f}")
        print(f"Max Drawdown: {stats.max_drawdown_percent:.2f}%")
        
    except ImportError as e:
        print(f"\nRust engine not available: {e}")
        print("Please compile with: cargo build --release --features python")
        print("For now, using Python-only simulation...\n")
        return False
    
    return True


if __name__ == "__main__":
    # Try to use Rust engine first
    if not run_with_rust_engine():
        # Fall back to Python simulation
        example = BacktestingExample()
        
        # Run Double MA backtest
        example.run_double_ma_backtest()
        
        # Run Grid backtest
        example.run_grid_backtest()
    
    print("\n" + "=" * 60)
    print("Backtesting examples completed!")
    print("=" * 60)
