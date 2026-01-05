"""
Strategy Example - Python CTA Strategy similar to vnpy_ctastrategy

This example demonstrates how to implement a Python strategy that:
1. Works with the Rust strategy engine
2. Supports both spot and futures trading
3. Follows the vnpy_ctastrategy template pattern
"""

from typing import Dict, List
from datetime import datetime


class CtaTemplate:
    """
    Base CTA Strategy Template
    
    Similar to vnpy_ctastrategy.template.CtaTemplate but works with Rust engine
    """
    
    def __init__(
        self,
        strategy_name: str,
        vt_symbols: List[str],
        strategy_type: str = "spot"  # "spot" or "futures"
    ):
        """Initialize strategy"""
        self.strategy_name = strategy_name
        self.vt_symbols = vt_symbols
        self.strategy_type = strategy_type
        
        # State flags
        self.inited = False
        self.trading = False
        
        # Positions tracking
        self.positions: Dict[str, float] = {}
        for symbol in vt_symbols:
            self.positions[symbol] = 0.0
        
        # Parameters (to be set by subclass)
        self.parameters: Dict[str, any] = {}
        
        # Variables (to be updated by subclass)
        self.variables: Dict[str, any] = {}
    
    def on_init(self):
        """
        Called when strategy is initialized
        Load historical data, initialize indicators, etc.
        """
        self.write_log("Strategy initialized")
    
    def on_start(self):
        """
        Called when strategy starts trading
        """
        self.write_log("Strategy started")
    
    def on_stop(self):
        """
        Called when strategy stops trading
        """
        self.write_log("Strategy stopped")
    
    def on_tick(self, tick: dict):
        """
        Called when new tick data arrives
        
        Args:
            tick: {
                'symbol': str,
                'exchange': str,
                'datetime': datetime,
                'last_price': float,
                'volume': float,
                'bid_price_1': float,
                'ask_price_1': float,
                ...
            }
        """
        pass
    
    def on_bar(self, bar: dict):
        """
        Called when new bar data arrives
        
        Args:
            bar: {
                'symbol': str,
                'exchange': str,
                'datetime': datetime,
                'interval': str,
                'open': float,
                'high': float,
                'low': float,
                'close': float,
                'volume': float,
            }
        """
        pass
    
    def on_bars(self, bars: Dict[str, dict]):
        """
        Called when new bars for multiple symbols arrive
        Used for multi-symbol strategies
        
        Args:
            bars: {symbol: bar_data}
        """
        pass
    
    def on_order(self, order: dict):
        """
        Called when order status updates
        
        Args:
            order: {
                'orderid': str,
                'symbol': str,
                'direction': str,  # 'long' or 'short'
                'offset': str,     # 'open', 'close', 'closetoday', 'closeyesterday'
                'price': float,
                'volume': float,
                'traded': float,
                'status': str,
            }
        """
        pass
    
    def on_trade(self, trade: dict):
        """
        Called when trade happens
        
        Args:
            trade: {
                'tradeid': str,
                'orderid': str,
                'symbol': str,
                'direction': str,
                'offset': str,
                'price': float,
                'volume': float,
                'datetime': datetime,
            }
        """
        # Update position
        vt_symbol = f"{trade['symbol']}.{trade['exchange']}"
        if trade['direction'] == 'long':
            if trade['offset'] == 'open':
                self.positions[vt_symbol] += trade['volume']
            else:
                self.positions[vt_symbol] -= trade['volume']
        else:  # short
            if trade['offset'] == 'open':
                self.positions[vt_symbol] -= trade['volume']
            else:
                self.positions[vt_symbol] += trade['volume']
    
    def buy(self, vt_symbol: str, price: float, volume: float, lock: bool = False) -> str:
        """
        Send buy order (open long for futures, buy for spot)
        
        Returns:
            orderid
        """
        if not self.trading:
            return ""
        
        return self._send_order(vt_symbol, price, volume, "long", "open", lock)
    
    def sell(self, vt_symbol: str, price: float, volume: float, lock: bool = False) -> str:
        """
        Send sell order (close long for futures, sell for spot)
        
        Returns:
            orderid
        """
        if not self.trading:
            return ""
        
        offset = "close" if self.strategy_type == "futures" else "open"
        return self._send_order(vt_symbol, price, volume, "short", offset, lock)
    
    def short(self, vt_symbol: str, price: float, volume: float, lock: bool = False) -> str:
        """
        Send short order (futures only)
        
        Returns:
            orderid
        """
        if self.strategy_type == "spot":
            self.write_log("Short not supported for spot trading")
            return ""
        
        if not self.trading:
            return ""
        
        return self._send_order(vt_symbol, price, volume, "short", "open", lock)
    
    def cover(self, vt_symbol: str, price: float, volume: float, lock: bool = False) -> str:
        """
        Send cover order (close short for futures)
        
        Returns:
            orderid
        """
        if self.strategy_type == "spot":
            self.write_log("Cover not supported for spot trading")
            return ""
        
        if not self.trading:
            return ""
        
        return self._send_order(vt_symbol, price, volume, "long", "close", lock)
    
    def cancel_order(self, vt_orderid: str):
        """Cancel order"""
        if not self.trading:
            return
        
        self.write_log(f"Cancel order: {vt_orderid}")
        # Call Rust engine to cancel
    
    def cancel_all(self):
        """Cancel all active orders"""
        if not self.trading:
            return
        
        self.write_log("Cancel all orders")
        # Call Rust engine to cancel all
    
    def _send_order(
        self,
        vt_symbol: str,
        price: float,
        volume: float,
        direction: str,
        offset: str,
        lock: bool
    ) -> str:
        """Internal method to send order"""
        self.write_log(
            f"Send order: {vt_symbol} {direction} {offset} @ {price} x{volume}"
        )
        # Call Rust engine to send order
        # Return orderid from Rust
        return f"order_{datetime.now().timestamp()}"
    
    def get_pos(self, vt_symbol: str) -> float:
        """Get current position for symbol"""
        return self.positions.get(vt_symbol, 0.0)
    
    def write_log(self, msg: str):
        """Write log message"""
        print(f"[{self.strategy_name}] {msg}")
    
    def load_bars(self, days: int, interval: str = "1m"):
        """
        Load historical bars for all symbols
        
        Args:
            days: Number of days to load
            interval: Bar interval (1m, 5m, 15m, 1h, 1d)
        """
        self.write_log(f"Loading {days} days of {interval} bars")
        # Call Rust engine to load historical data
    
    def put_event(self):
        """Update strategy UI"""
        pass


class DoubleMaStrategy(CtaTemplate):
    """
    Double Moving Average Strategy Example
    
    Strategy Logic:
    - When fast MA crosses above slow MA -> Buy
    - When fast MA crosses below slow MA -> Sell
    """
    
    def __init__(
        self,
        strategy_name: str,
        vt_symbols: List[str],
        fast_window: int = 10,
        slow_window: int = 20,
        fixed_size: float = 1.0
    ):
        """Initialize with parameters"""
        super().__init__(strategy_name, vt_symbols, strategy_type="spot")
        
        # Parameters
        self.fast_window = fast_window
        self.slow_window = slow_window
        self.fixed_size = fixed_size
        
        self.parameters = {
            "fast_window": fast_window,
            "slow_window": slow_window,
            "fixed_size": fixed_size,
        }
        
        # Variables
        self.fast_ma = 0.0
        self.slow_ma = 0.0
        self.ma_trend = 0  # 1: bullish, -1: bearish, 0: neutral
        
        self.variables = {
            "fast_ma": 0.0,
            "slow_ma": 0.0,
            "ma_trend": 0,
        }
        
        # Historical data
        self.bars: List[dict] = []
    
    def on_init(self):
        """Initialize strategy"""
        self.write_log("Initializing DoubleMaStrategy")
        
        # Load 10 days of 1-minute bars
        self.load_bars(10, interval="1m")
        
        self.inited = True
    
    def on_start(self):
        """Start strategy"""
        self.write_log("Starting DoubleMaStrategy")
        self.trading = True
    
    def on_stop(self):
        """Stop strategy"""
        self.write_log("Stopping DoubleMaStrategy")
        self.trading = False
    
    def on_bar(self, bar: dict):
        """Process new bar"""
        # Add to history
        self.bars.append(bar)
        
        # Keep only necessary data
        if len(self.bars) > self.slow_window * 2:
            self.bars.pop(0)
        
        # Need enough data
        if len(self.bars) < self.slow_window:
            return
        
        # Calculate MAs
        self.calculate_ma()
        
        # Trading logic
        vt_symbol = f"{bar['symbol']}.{bar['exchange']}"
        pos = self.get_pos(vt_symbol)
        
        # Check for crossover
        if self.fast_ma > self.slow_ma:
            # Golden cross - bullish
            if self.ma_trend != 1:
                self.ma_trend = 1
                
                # If no position or short, go long
                if pos <= 0:
                    self.buy(vt_symbol, bar['close'], self.fixed_size)
                    
        elif self.fast_ma < self.slow_ma:
            # Death cross - bearish
            if self.ma_trend != -1:
                self.ma_trend = -1
                
                # If long position, close
                if pos > 0:
                    self.sell(vt_symbol, bar['close'], abs(pos))
        
        # Update UI
        self.put_event()
    
    def calculate_ma(self):
        """Calculate moving averages"""
        if len(self.bars) < self.slow_window:
            return
        
        # Calculate fast MA
        fast_data = self.bars[-self.fast_window:]
        self.fast_ma = sum(b['close'] for b in fast_data) / self.fast_window
        
        # Calculate slow MA
        slow_data = self.bars[-self.slow_window:]
        self.slow_ma = sum(b['close'] for b in slow_data) / self.slow_window
        
        # Update variables
        self.variables['fast_ma'] = self.fast_ma
        self.variables['slow_ma'] = self.slow_ma
        self.variables['ma_trend'] = self.ma_trend


class GridStrategy(CtaTemplate):
    """
    Grid Trading Strategy Example
    
    Strategy Logic:
    - Place buy orders at grid levels below current price
    - Place sell orders at grid levels above current price
    - Profit from oscillating market
    """
    
    def __init__(
        self,
        strategy_name: str,
        vt_symbols: List[str],
        grid_size: float = 10.0,
        grid_num: int = 5,
        order_size: float = 0.1
    ):
        """Initialize with parameters"""
        super().__init__(strategy_name, vt_symbols, strategy_type="spot")
        
        # Parameters
        self.grid_size = grid_size  # Price distance between grids
        self.grid_num = grid_num    # Number of grids
        self.order_size = order_size
        
        self.parameters = {
            "grid_size": grid_size,
            "grid_num": grid_num,
            "order_size": order_size,
        }
        
        # Grid orders tracking
        self.buy_orders: Dict[str, str] = {}   # price -> orderid
        self.sell_orders: Dict[str, str] = {}  # price -> orderid
        
        self.last_price = 0.0
    
    def on_init(self):
        """Initialize strategy"""
        self.write_log("Initializing GridStrategy")
        self.inited = True
    
    def on_start(self):
        """Start strategy"""
        self.write_log("Starting GridStrategy")
        self.trading = True
    
    def on_stop(self):
        """Stop strategy"""
        self.write_log("Stopping GridStrategy")
        
        # Cancel all grid orders
        self.cancel_all()
        
        self.trading = False
    
    def on_tick(self, tick: dict):
        """Process new tick"""
        self.last_price = tick['last_price']
        
        # Check if need to place new grid orders
        self.check_grid_orders(tick)
    
    def on_trade(self, trade: dict):
        """Process trade"""
        super().on_trade(trade)
        
        # When a grid order is filled, place reverse order
        # This is the core grid trading logic
        
        if trade['direction'] == 'long':
            # Buy order filled, place sell order above
            sell_price = trade['price'] + self.grid_size
            self.sell(
                f"{trade['symbol']}.{trade['exchange']}",
                sell_price,
                trade['volume']
            )
        else:
            # Sell order filled, place buy order below
            buy_price = trade['price'] - self.grid_size
            self.buy(
                f"{trade['symbol']}.{trade['exchange']}",
                buy_price,
                trade['volume']
            )
    
    def check_grid_orders(self, tick: dict):
        """Check and place grid orders"""
        vt_symbol = f"{tick['symbol']}.{tick['exchange']}"
        current_price = tick['last_price']
        
        # Place buy orders below current price
        for i in range(1, self.grid_num + 1):
            buy_price = current_price - i * self.grid_size
            
            # Check if order already exists
            if str(buy_price) not in self.buy_orders:
                orderid = self.buy(vt_symbol, buy_price, self.order_size)
                if orderid:
                    self.buy_orders[str(buy_price)] = orderid
        
        # Place sell orders above current price
        for i in range(1, self.grid_num + 1):
            sell_price = current_price + i * self.grid_size
            
            # Check if order already exists
            if str(sell_price) not in self.sell_orders:
                orderid = self.sell(vt_symbol, sell_price, self.order_size)
                if orderid:
                    self.sell_orders[str(sell_price)] = orderid


def run_strategy_example():
    """Run strategy examples"""
    
    # Example 1: Double MA Strategy
    print("=" * 50)
    print("Double MA Strategy Example")
    print("=" * 50)
    
    dma_strategy = DoubleMaStrategy(
        strategy_name="DMA_BTC",
        vt_symbols=["BTCUSDT.BINANCE"],
        fast_window=10,
        slow_window=20,
        fixed_size=0.01
    )
    
    dma_strategy.on_init()
    dma_strategy.on_start()
    
    # Simulate bar data
    test_bar = {
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
    
    dma_strategy.on_bar(test_bar)
    
    print(f"Fast MA: {dma_strategy.fast_ma}")
    print(f"Slow MA: {dma_strategy.slow_ma}")
    print(f"Position: {dma_strategy.get_pos('BTCUSDT.BINANCE')}")
    
    dma_strategy.on_stop()
    
    # Example 2: Grid Strategy
    print("\n" + "=" * 50)
    print("Grid Strategy Example")
    print("=" * 50)
    
    grid_strategy = GridStrategy(
        strategy_name="GRID_BTC",
        vt_symbols=["BTCUSDT.BINANCE"],
        grid_size=100.0,
        grid_num=5,
        order_size=0.01
    )
    
    grid_strategy.on_init()
    grid_strategy.on_start()
    
    # Simulate tick data
    test_tick = {
        'symbol': 'BTCUSDT',
        'exchange': 'BINANCE',
        'datetime': datetime.now(),
        'last_price': 50000.0,
        'volume': 100.0,
        'bid_price_1': 49995.0,
        'ask_price_1': 50005.0,
    }
    
    grid_strategy.on_tick(test_tick)
    
    print(f"Buy orders: {len(grid_strategy.buy_orders)}")
    print(f"Sell orders: {len(grid_strategy.sell_orders)}")
    
    grid_strategy.on_stop()


if __name__ == "__main__":
    run_strategy_example()
