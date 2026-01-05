import sys
import os
import datetime
from typing import Dict, Any

# Add project root to path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../")))
# Add vnpy_ctastrategy to path
sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../vnpy_ctastrategy")))

from trade_engine import PyBacktestingEngine, PyBarData
from vnpy_ctastrategy.strategies.boll_channel_strategy import BollChannelStrategy

def run_backtest():
    """Run backtest for BollChannelStrategy"""
    print("Initializing Backtesting Engine...")
    engine = PyBacktestingEngine()
    
    # Set parameters
    engine.set_parameters(
        vt_symbol="BTCUSDT.BINANCE",
        interval="15m",
        start="2024-01-01",
        end="2024-02-01",
        rate=0.001,
        slippage=0.0,
        size=1.0,
        pricetick=0.01,
        capital=10000.0
    )
    
    print("Generating Mock Data...")
    # Generate some mock data for verification as we don't have DB connected
    bars = []
    base_price = 40000.0
    import random
    
    start_dt = datetime.datetime(2024, 1, 1)
    for i in range(1000):
        dt = start_dt + datetime.timedelta(minutes=15 * i)
        open_price = base_price + random.uniform(-50, 50)
        high_price = open_price + random.uniform(0, 50)
        low_price = open_price - random.uniform(0, 50)
        close_price = random.uniform(low_price, high_price)
        
        # Add some trend to make it interesting
        base_price = close_price
        
        bar = PyBarData(
            gateway_name="BACKTESTING",
            symbol="BTCUSDT",
            exchange="BINANCE",
            datetime=dt.isoformat() + "+00:00",
            interval="1m",
            open_price=open_price,
            high_price=high_price,
            low_price=low_price,
            close_price=close_price, 
            volume=random.uniform(10, 100)
        )
        bars.append(bar)
        
    print(f"Loading {len(bars)} mock bars...")
    engine.set_history_data(bars)
    
    print("Adding Strategy...")
    # Add strategy
    # Instantiate explicitly to ensure correct initialization
    strategy = BollChannelStrategy(
        engine, 
        "BollChannel", 
        "BTCUSDT.BINANCE", 
        {"boll_window": 20, "boll_dev": 2.0}
    )
    
    engine.add_strategy(
        strategy,
        "BollChannel",
        ["BTCUSDT.BINANCE"],
        {}
    )
    
    print("Running Backtest...")
    # Since PyBacktestingEngine doesn't seemingly expose a run_backtesting method directly 
    # (based on previous view_file, it has calculate_result but maybe not explicit run trigger visible in PythonEngine wrapper?)
    # Wait, looking at backtesting_bindings.rs again...
    # It has set_parameters, set_history_data, but NO run_backtesting exposed!
    # I missed that in the plan. I need to expose run_backtesting too.
    
    # I will add the call here assuming I fix it, or I might need to check if calculate_result runs it implicitly? 
    # In engine.rs: calculate_result just calculates based on daily_results. 
    # run_backtesting is what populates daily_results.
    
    # WARNING: I missed exposing `run_backtesting` in the Rust bindings.
    # I will assume I need to add it.
    
    try:
        engine.run_backtesting() 
    except AttributeError:
        print("Error: run_backtesting method not found! (As expected, need to fix bindings)")
        return

    print("Calculating Statistics...")
    stats = engine.calculate_statistics(True)
    
    # Print results
    print("\nBacktesting Results:")
    print(f"Total Return: {stats.total_net_pnl:.2f}")
    print(f"Sharpe Ratio: {stats.sharpe_ratio:.2f}")
    print(f"Max Drawdown: {stats.max_drawdown:.2f}")

if __name__ == "__main__":
    run_backtest()
