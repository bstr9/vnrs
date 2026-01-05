# Strategy Framework Implementation

## Overview

Implemented a comprehensive CTA-like strategy framework inspired by `vnpy_ctastrategy` with the following key features:

1. **Support for Multiple Strategy Types**: Spot, Futures, Grid Trading, Market Making, and Arbitrage
2. **Python-Rust Hybrid**: Strategy logic in Python, execution engine in Rust
3. **Event-Driven Architecture**: Async/await with Tokio runtime
4. **Position and Order Management**: Full lifecycle tracking
5. **Stop Order Support**: Price-triggered conditional orders

## Architecture

### Rust Components

#### 1. Core Types (`src/strategy/base.rs`)

- **StrategyType**: Defines strategy categories
  ```rust
  pub enum StrategyType {
      Spot,           // Spot trading
      Futures,        // Futures trading
      Grid,           // Grid trading
      MarketMaking,   // Market making
      Arbitrage,      // Arbitrage trading
  }
  ```

- **StrategyState**: Strategy lifecycle states
  ```rust
  pub enum StrategyState {
      NotInited,
      Inited,
      Trading,
      Stopped,
  }
  ```

- **StopOrder**: Price-triggered conditional orders with full tracking

#### 2. Strategy Template (`src/strategy/template.rs`)

- **StrategyTemplate** trait: Core interface for all strategies
  - `on_init()`: Initialize strategy
  - `on_start()`: Start trading
  - `on_stop()`: Stop trading
  - `on_tick()`: Process tick data
  - `on_bar()`: Process bar data
  - `on_bars()`: Process multiple bars (multi-symbol)
  - `on_order()`: Handle order updates
  - `on_trade()`: Handle trade fills
  - `get_position()`, `update_position()`: Position management
  - `get_parameters()`, `get_variables()`: Parameter access

- **BaseStrategy**: Base implementation with common functionality
- **TargetPosTemplate**: Trait for DMA/Grid strategies with position rebalancing
- **StrategyContext**: Market data cache for strategy access

#### 3. Strategy Engine (`src/strategy/engine.rs`)

Main engine managing all strategies:

- **Strategy Lifecycle**:
  - `add_strategy()`: Register new strategy
  - `init_strategy()`: Initialize strategy
  - `start_strategy()`: Start trading
  - `stop_strategy()`: Stop trading
  - `remove_strategy()`: Remove strategy

- **Event Handling**:
  - `process_tick_event()`: Route tick data to subscribed strategies
  - `process_order_event()`: Update order status
  - `process_trade_event()`: Update positions, deduplicate trades

- **Stop Order Management**:
  - `check_stop_orders()`: Check if price conditions met
  - `trigger_stop_order()`: Convert stop order to market order

- **Data Management**:
  - `query_history()`: Load historical bars
  - Symbol-to-strategy mapping for efficient routing
  - Order-to-strategy mapping for callback routing

#### 4. Python Bindings (`src/python/strategy_bindings.rs`)

PyO3-based Python interface:

- **PyStrategy**: Python strategy wrapper with callbacks
  - `buy()`, `sell()`, `short()`, `cover()`: Order placement
  - `cancel_order()`, `cancel_all()`: Order cancellation
  - `get_pos()`: Query position
  - `write_log()`: Logging
  - Callback registration: `set_on_init()`, `set_on_tick()`, etc.

- **PyStrategyEngine**: Python engine wrapper
  - `add_strategy()`: Add Python strategy
  - `init_strategy()`, `start_strategy()`, `stop_strategy()`: Lifecycle control
  - `get_all_strategies()`: Query strategies
  - `get_strategy_info()`: Get strategy details

### Python Components

#### 1. Strategy Template (`examples/strategy_example.py`)

**CtaTemplate**: Base class for Python strategies

```python
class CtaTemplate:
    def __init__(self, strategy_name, vt_symbols, strategy_type="spot")
    def on_init(self)      # Initialize
    def on_start(self)     # Start
    def on_stop(self)      # Stop
    def on_tick(tick)      # Process tick
    def on_bar(bar)        # Process bar
    def on_bars(bars)      # Process multiple bars
    def on_order(order)    # Order update
    def on_trade(trade)    # Trade fill
    
    # Trading methods
    def buy(vt_symbol, price, volume, lock=False) -> orderid
    def sell(vt_symbol, price, volume, lock=False) -> orderid
    def short(vt_symbol, price, volume, lock=False) -> orderid
    def cover(vt_symbol, price, volume, lock=False) -> orderid
    def cancel_order(vt_orderid)
    def cancel_all()
    
    # Position management
    def get_pos(vt_symbol) -> float
    
    # Utility
    def write_log(msg)
    def load_bars(days, interval)
```

#### 2. Example Strategies

**DoubleMaStrategy**: Moving average crossover strategy
- Fast MA crosses above slow MA → Buy
- Fast MA crosses below slow MA → Sell
- Supports spot trading
- Parameters: fast_window, slow_window, fixed_size

**GridStrategy**: Grid trading strategy  
- Place buy orders below current price
- Place sell orders above current price
- Auto-rebalance when orders fill
- Supports spot trading
- Parameters: grid_size, grid_num, order_size

## Key Features

### 1. Multi-Symbol Support

Strategies can subscribe to multiple symbols:
```python
strategy = DoubleMaStrategy(
    strategy_name="DMA_Multi",
    vt_symbols=["BTCUSDT.BINANCE", "ETHUSDT.BINANCE"],
    fast_window=10,
    slow_window=20
)
```

### 2. Stop Orders

Price-triggered conditional orders:
- Set trigger price and direction
- Auto-submit when price condition met
- Tracked separately from regular orders

### 3. Position Management

- Automatic position tracking per symbol
- Long/Short/Net position support
- Trade deduplication to prevent double-counting

### 4. Historical Data Loading

```python
def on_init(self):
    # Load 10 days of 1-minute bars
    self.load_bars(10, interval="1m")
```

### 5. Spot vs Futures

**Spot Trading**:
- `buy()` → Open long
- `sell()` → Close long
- No short/cover support

**Futures Trading**:
- `buy()` → Open long
- `sell()` → Close long
- `short()` → Open short
- `cover()` → Close short

## Usage Example

```python
from strategy_example import DoubleMaStrategy

# Create strategy instance
strategy = DoubleMaStrategy(
    strategy_name="DMA_BTC",
    vt_symbols=["BTCUSDT.BINANCE"],
    fast_window=10,
    slow_window=20,
    fixed_size=0.01
)

# Initialize
strategy.on_init()

# Start trading
strategy.on_start()

# Process bar data
bar = {
    'symbol': 'BTCUSDT',
    'exchange': 'BINANCE',
    'datetime': datetime.now(),
    'interval': '1m',
    'open': 50000.0,
    'high': 50100.0,
    'low': 49900.0,
    'close': 50050.0,
    'volume': 100.0,
}

strategy.on_bar(bar)

# Check position
pos = strategy.get_pos("BTCUSDT.BINANCE")
print(f"Current position: {pos}")

# Stop trading
strategy.on_stop()
```

## Comparison with vnpy_ctastrategy

| Feature | vnpy_ctastrategy | This Implementation |
|---------|------------------|---------------------|
| Language | Python | Python + Rust |
| Trading Type | Futures (CTA) | Spot + Futures + Grid + More |
| Execution | Python | Rust (faster) |
| Event System | Python EventEngine | Rust async/tokio |
| Position Tracking | Yes | Yes |
| Stop Orders | Yes | Yes |
| Multi-symbol | Yes | Yes |
| Backtesting | Built-in | Planned |
| Python API | Native | PyO3 bindings |

## File Structure

```
trade_engine/
├── src/
│   ├── strategy/
│   │   ├── mod.rs              # Module exports
│   │   ├── base.rs             # Core types (~115 lines)
│   │   ├── template.rs         # Strategy template (~294 lines)
│   │   └── engine.rs           # Strategy engine (~496 lines)
│   └── python/
│       ├── mod.rs              # Python module exports
│       └── strategy_bindings.rs # PyO3 bindings (~314 lines)
└── examples/
    └── strategy_example.py     # Python examples (~550 lines)
```

## Next Steps

1. **Complete Python Bindings**:
   - Connect PyStrategy to Rust StrategyTemplate
   - Implement callback forwarding

2. **Integration with MainEngine**:
   - Wire up order routing
   - Connect to gateway for real trading

3. **UI Components**:
   - Strategy management panel
   - Parameter configuration dialog
   - Position and PnL display

4. **Backtesting**:
   - Historical data replay
   - Performance analytics
   - Strategy optimization

5. **Additional Features**:
   - Strategy persistence (save/load)
   - Risk management (max position, max loss)
   - Performance metrics (Sharpe, drawdown)
   - Multi-account support

## Testing

The Python example can be run directly:

```bash
python examples/strategy_example.py
```

Output shows:
- Double MA strategy initialization
- Grid strategy with 5 buy and 5 sell orders
- Position tracking
- Order placement logging

## Technical Notes

### Event Routing

Events flow: Gateway → EventEngine → StrategyEngine → Individual Strategies

```
Tick Data
  ↓
EventEngine (EVENT_TICK)
  ↓
StrategyEngine.process_tick_event()
  ↓
Strategy.on_tick()
```

### Thread Safety

- All shared state uses `Arc<RwLock<T>>`
- Async operations with tokio
- Python GIL released during Rust execution

### Performance Considerations

- Event handlers registered once at startup
- Symbol-to-strategy mapping for O(1) routing
- Trade deduplication with HashSet
- Historical data cached per strategy

## Dependencies

**Rust**:
- `tokio`: Async runtime
- `pyo3`: Python bindings
- `serde`: Serialization
- `chrono`: DateTime handling

**Python**:
- Standard library only (datetime, typing, collections)

## Compilation

```bash
# Build with Python support
cargo build --features python

# Release build
cargo build --release --features python
```

## Known Issues

1. **Python Callback Integration**: PyO3 callbacks not yet fully integrated with Rust trait system
2. **MainEngine Connection**: Order routing not connected to actual gateway
3. **Historical Data Loading**: query_history() method not implemented
4. **Strategy Persistence**: Save/load functionality not implemented

## License

Same as parent project
