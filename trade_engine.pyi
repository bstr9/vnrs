"""Type stubs for the trade_engine native module (PyO3).

This file provides IDE autocompletion and type checking support
for the Rust trading engine's Python bindings. All method bodies
are ellipses (...) per the .pyi convention.
"""
from __future__ import annotations

from datetime import datetime
from typing import Any, Dict, List, Optional, Sequence, Tuple, Union


# ---------------------------------------------------------------------------
# Module-level constants
# ---------------------------------------------------------------------------

LONG: str
SHORT: str
NET: str


class Direction:
    """Trading direction constants (vnpy compatible)."""
    LONG: str
    SHORT: str
    NET: str


# ---------------------------------------------------------------------------
# Data type classes
# ---------------------------------------------------------------------------

class PyTickData:
    """Typed tick data with attribute and dict-style access."""

    gateway_name: str
    symbol: str
    exchange: str
    name: str
    volume: float
    turnover: float
    open_interest: float
    last_price: float
    last_volume: float
    limit_up: float
    limit_down: float
    open_price: float
    high_price: float
    low_price: float
    pre_close: float
    bid_price_1: float
    bid_price_2: float
    bid_price_3: float
    bid_price_4: float
    bid_price_5: float
    ask_price_1: float
    ask_price_2: float
    ask_price_3: float
    ask_price_4: float
    ask_price_5: float
    bid_volume_1: float
    bid_volume_2: float
    bid_volume_3: float
    bid_volume_4: float
    bid_volume_5: float
    ask_volume_1: float
    ask_volume_2: float
    ask_volume_3: float
    ask_volume_4: float
    ask_volume_5: float

    @property
    def datetime(self) -> datetime: ...

    @datetime.setter
    def datetime(self, value: Union[str, datetime]) -> None: ...

    def __init__(
        self,
        gateway_name: str = ...,
        symbol: str = ...,
        exchange: str = ...,
        datetime: str = ...,
        name: str = ...,
        volume: float = ...,
        turnover: float = ...,
        open_interest: float = ...,
        last_price: float = ...,
        last_volume: float = ...,
        limit_up: float = ...,
        limit_down: float = ...,
        open_price: float = ...,
        high_price: float = ...,
        low_price: float = ...,
        pre_close: float = ...,
        bid_price_1: float = ...,
        bid_price_2: float = ...,
        bid_price_3: float = ...,
        bid_price_4: float = ...,
        bid_price_5: float = ...,
        ask_price_1: float = ...,
        ask_price_2: float = ...,
        ask_price_3: float = ...,
        ask_price_4: float = ...,
        ask_price_5: float = ...,
        bid_volume_1: float = ...,
        bid_volume_2: float = ...,
        bid_volume_3: float = ...,
        bid_volume_4: float = ...,
        bid_volume_5: float = ...,
        ask_volume_1: float = ...,
        ask_volume_2: float = ...,
        ask_volume_3: float = ...,
        ask_volume_4: float = ...,
        ask_volume_5: float = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...
    def get(self, key: str, default_value: Any = ...) -> Any: ...


class PyOrderData:
    """Typed order data with attribute and dict-style access."""

    gateway_name: str
    symbol: str
    exchange: str
    orderid: str
    order_type: str  # "LIMIT", "MARKET", "STOP", "STOP_LIMIT", "FAK", "FOK", "PEGGED_BEST"
    direction: str  # "LONG", "SHORT", "NET", ""
    offset: str  # "NONE", "OPEN", "CLOSE", "CLOSETODAY", "CLOSEYESTERDAY"
    price: float
    volume: float
    traded: float
    status: str  # "SUBMITTING", "NOTTRADED", "PARTTRADED", "ALLTRADED", "CANCELLED", "REJECTED"
    reference: str
    post_only: bool
    reduce_only: bool

    @property
    def datetime(self) -> Optional[datetime]: ...

    @datetime.setter
    def datetime(self, value: Union[str, datetime, None]) -> None: ...

    def __init__(
        self,
        gateway_name: str = ...,
        symbol: str = ...,
        exchange: str = ...,
        orderid: str = ...,
        order_type: str = ...,
        direction: str = ...,
        offset: str = ...,
        price: float = ...,
        volume: float = ...,
        traded: float = ...,
        status: str = ...,
        datetime: str = ...,
        reference: str = ...,
        post_only: bool = ...,
        reduce_only: bool = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...
    def get(self, key: str, default_value: Any = ...) -> Any: ...


class PyTradeData:
    """Typed trade data with attribute and dict-style access."""

    gateway_name: str
    symbol: str
    exchange: str
    orderid: str
    tradeid: str
    direction: str
    offset: str
    price: float
    volume: float

    @property
    def datetime(self) -> Optional[datetime]: ...

    @datetime.setter
    def datetime(self, value: Union[str, datetime, None]) -> None: ...

    def __init__(
        self,
        gateway_name: str = ...,
        symbol: str = ...,
        exchange: str = ...,
        orderid: str = ...,
        tradeid: str = ...,
        direction: str = ...,
        offset: str = ...,
        price: float = ...,
        volume: float = ...,
        datetime: str = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...
    def get(self, key: str, default_value: Any = ...) -> Any: ...


class PyDepthData:
    """Typed depth/orderbook data with attribute and dict-style access."""

    gateway_name: str
    symbol: str
    exchange: str
    bid_prices: List[float]
    bid_volumes: List[float]
    ask_prices: List[float]
    ask_volumes: List[float]

    @property
    def datetime(self) -> Optional[datetime]: ...

    @datetime.setter
    def datetime(self, value: Union[str, datetime, None]) -> None: ...

    def __init__(
        self,
        gateway_name: str = ...,
        symbol: str = ...,
        exchange: str = ...,
        datetime: str = ...,
        bid_prices: List[float] = ...,
        bid_volumes: List[float] = ...,
        ask_prices: List[float] = ...,
        ask_volumes: List[float] = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...
    def get(self, key: str, default_value: Any = ...) -> Any: ...


# ---------------------------------------------------------------------------
# Strategy base class
# ---------------------------------------------------------------------------

class Strategy:
    """Base class for trading strategies.

    Python users subclass this to implement trading strategies::

        class MyStrategy(Strategy):
            def on_init(self):
                self.write_log("Initialized")

            def on_bar(self, bar):
                self.buy("BTCUSDT.BINANCE", bar.close, 1.0)
    """

    strategy_name: str
    vt_symbols: List[str]
    strategy_type: str  # "spot" or "futures"
    pos_data: Dict[str, float]
    target_data: Dict[str, float]
    parameters: Dict[str, str]
    parameter_types: Dict[str, str]
    variables: Dict[str, str]
    active_orderids: List[str]
    active_stop_orderids: List[str]
    engine: Optional[Any]
    portfolio: Optional[PortfolioFacade]
    order_factory: Optional[OrderFactory]
    message_bus: Optional[MessageBus]
    context: Optional[PyStrategyContext]

    @property
    def state(self) -> str: ...
    @property
    def vt_symbol(self) -> str: ...
    @property
    def pos(self) -> float: ...

    def __init__(
        self,
        strategy_name: str = ...,
        vt_symbols: List[str] = ...,
        strategy_type: str = ...,
    ) -> None: ...

    # -- Lifecycle callbacks (override in subclass) --

    def on_init(self) -> None: ...
    def on_start(self) -> None: ...
    def on_stop(self) -> None: ...
    def on_reset(self) -> None: ...
    def on_tick(self, tick: Any) -> None: ...
    def on_bar(self, bar: Any) -> None: ...
    def on_bars(self, bars: Any) -> None: ...
    def on_order(self, order: Any) -> None: ...
    def on_trade(self, trade: Any) -> None: ...
    def on_depth(self, depth: Any) -> None: ...
    def on_stop_order(self, stop_orderid: str) -> None: ...
    def on_timer(self, timer_id: str) -> None: ...

    # -- State mutators (called by engine) --

    def set_inited(self) -> None: ...
    def set_trading(self) -> None: ...
    def set_stopped(self) -> None: ...

    # -- Order convenience methods --

    def buy(self, vt_symbol: str, price: float, volume: float, offset: Optional[str] = ...) -> List[str]: ...
    def sell(self, vt_symbol: str, price: float, volume: float, offset: Optional[str] = ...) -> List[str]: ...
    def short(self, vt_symbol: str, price: float, volume: float, offset: Optional[str] = ...) -> List[str]: ...
    def cover(self, vt_symbol: str, price: float, volume: float, offset: Optional[str] = ...) -> List[str]: ...
    def buy_open(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def buy_close(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def short_open(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def sell_close(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...

    def send_stop_order(
        self,
        vt_symbol: str,
        direction: str,
        price: float,
        volume: float,
        stop_price: float,
        offset: Optional[str] = ...,
        order_type: str = ...,
    ) -> str: ...

    def cancel_order(self, vt_orderid: str) -> None: ...
    def cancel_stop_order(self, stop_orderid: str) -> None: ...
    def cancel_all(self) -> None: ...
    def reset(self) -> None: ...
    def restart(self) -> None: ...
    def put_event(self) -> None: ...

    # -- Historical data loading --

    def load_bar(self, days: int, interval: Optional[str] = ...) -> None: ...
    def load_tick(self, days: int) -> None: ...

    # -- Position queries --

    def get_pos_by_symbol(self, vt_symbol: str) -> float: ...
    def set_pos(self, vt_symbol: str, position: float) -> None: ...

    # -- Subscription management --

    def subscribe(self, vt_symbol: str) -> None: ...
    def unsubscribe(self, vt_symbol: str) -> None: ...

    # -- Timer management --

    def schedule_timer(self, timer_id: str, seconds: float, repeat: bool = ...) -> None: ...
    def cancel_timer(self, timer_id: str) -> None: ...

    # -- Parameter and variable access --

    def get_parameter(self, key: str, default: Optional[str] = ...) -> Optional[str]: ...
    def get_parameter_type(self, key: str) -> Optional[str]: ...
    def set_parameter(self, key: str, value: str) -> None: ...
    def insert_parameter(self, key: str, type_hint: str, value: str) -> None: ...
    def get_variable(self, key: str, default: Optional[str] = ...) -> Optional[str]: ...
    def set_variable(self, key: str, value: str) -> None: ...
    def insert_variable(self, key: str, value: str) -> None: ...
    def load_setting(self, setting: Dict[str, str]) -> None: ...

    # -- Utilities --

    def write_log(self, msg: str) -> None: ...
    def send_email(self, msg: str) -> None: ...
    def get_instrument(self, vt_symbol: str) -> Optional[PyInstrument]: ...


# Backward compatibility alias
CtaStrategy = Strategy


# ---------------------------------------------------------------------------
# Engine wrappers
# ---------------------------------------------------------------------------

class PythonEngineWrapper:
    """Python-facing wrapper for the trading engine."""

    def __init__(self) -> None: ...

    def add_strategy(self, strategy: Strategy) -> None: ...
    def set_strategy_engine(self, handle: StrategyEngineHandle) -> None: ...
    def init_strategy(self, strategy_name: str) -> None: ...
    def start_strategy(self, strategy_name: str) -> None: ...
    def stop_strategy(self, strategy_name: str) -> None: ...
    def reset_strategy(self, strategy_name: str) -> None: ...
    def restart_strategy(self, strategy_name: str) -> None: ...

    def on_tick(self, tick_dict: Dict[str, Any]) -> None: ...
    def on_bar(self, bar_dict: Dict[str, Any]) -> None: ...
    def on_trade(self, trade_dict: Dict[str, Any]) -> None: ...
    def on_order(self, order_dict: Dict[str, Any]) -> None: ...

    def buy(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def sell(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def short(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def cover(self, vt_symbol: str, price: float, volume: float) -> List[str]: ...
    def cancel_order(self, vt_orderid: str) -> None: ...
    def get_pos(self, vt_symbol: str) -> float: ...
    def write_log(self, msg: str) -> None: ...
    def send_email(self, msg: str) -> None: ...
    def send_order_typed(
        self,
        vt_symbol: str,
        direction_str: str,
        offset_str: str,
        price: float,
        volume: float,
        order_type_str: str,
    ) -> List[str]: ...

    def create_order_factory(self) -> OrderFactory: ...
    def get_instrument(self, vt_symbol: str) -> Optional[PyInstrument]: ...
    def subscribe(self, strategy_name: str, vt_symbol: str) -> None: ...
    def unsubscribe(self, strategy_name: str, vt_symbol: str) -> None: ...
    def schedule_timer(self, strategy_name: str, timer_id: str, seconds: float, repeat: bool = ...) -> None: ...
    def cancel_timer(self, strategy_name: str, timer_id: str) -> None: ...

    def get_active_toasts(self) -> List[PyAlertMessage]: ...
    def get_recent_toasts(self, limit: int = ...) -> List[PyAlertMessage]: ...

    def set_stp_mode(self, mode: str) -> None: ...
    def get_stp_mode(self) -> str: ...
    def offset_converter(self) -> PyOffsetConverter: ...
    def get_stop_order_engine(self) -> PyStopOrderEngine: ...
    def get_bracket_order_engine(self) -> PyBracketOrderEngine: ...
    def get_order_emulator(self) -> PyOrderEmulator: ...
    def get_message_bus(self) -> MessageBus: ...


class StrategyEngineHandle:
    """Handle to a live StrategyEngine for real-time strategy management."""

    def get_all_strategy_names(self) -> List[str]: ...
    def reset_strategy(self, strategy_name: str) -> None: ...
    def restart_strategy(self, strategy_name: str) -> None: ...


# ---------------------------------------------------------------------------
# Module-level functions
# ---------------------------------------------------------------------------

def create_main_engine() -> PythonEngineWrapper: ...
def run_event_loop() -> None: ...
def add_strategy_live(
    strategy: Strategy,
    strategy_engine: StrategyEngineHandle,
    setting: Optional[Dict[str, Any]] = ...,
) -> List[str]: ...


# ---------------------------------------------------------------------------
# Alert message
# ---------------------------------------------------------------------------

class PyAlertMessage:
    """Python-accessible alert message from the engine."""

    level: str
    title: str
    body: str
    source: str
    timestamp: str
    vt_symbol: Optional[str]

    def __repr__(self) -> str: ...


# ---------------------------------------------------------------------------
# Order factory
# ---------------------------------------------------------------------------

class PyOrder:
    """Typed order object created by OrderFactory."""

    vt_symbol: str
    direction: str
    offset: str
    order_type: str
    price: float
    volume: float
    reference: str
    post_only: bool
    reduce_only: bool

    def submit(self) -> List[str]: ...
    def cancel(self) -> None: ...


class OrderFactory:
    """Factory for creating typed orders bound to an engine."""

    def __init__(self, engine: Any, gateway_name: str = ...) -> None: ...

    def buy(self, vt_symbol: str, price: float, volume: float, offset: str = ...) -> PyOrder: ...
    def sell(self, vt_symbol: str, price: float, volume: float, offset: str = ...) -> PyOrder: ...
    def short(self, vt_symbol: str, price: float, volume: float, offset: str = ...) -> PyOrder: ...
    def cover(self, vt_symbol: str, price: float, volume: float, offset: str = ...) -> PyOrder: ...


# ---------------------------------------------------------------------------
# Instrument
# ---------------------------------------------------------------------------

class PyInstrument:
    """Instrument/contract metadata."""

    symbol: str
    exchange: str
    name: str
    product: str
    size: float
    pricetick: float
    min_volume: float
    max_volume: float
    margin_rate: float

    @property
    def vt_symbol(self) -> str: ...


# ---------------------------------------------------------------------------
# Strategy context
# ---------------------------------------------------------------------------

class PyStrategyContext:
    """Context providing market data access (tick/bar caches) to strategies."""

    def get_tick(self, vt_symbol: str) -> Optional[PyTickData]: ...
    def get_bar(self, vt_symbol: str) -> Optional[PyBarData]: ...
    def get_historical_bars(self, vt_symbol: str, length: int = ...) -> List[PyBarData]: ...


# ---------------------------------------------------------------------------
# Array manager
# ---------------------------------------------------------------------------

class PyArrayManager:
    """Numpy-like array manager for technical indicator calculations."""

    size: int
    count: int
    inited: bool

    def __init__(self, size: int = ...) -> None: ...

    def update_bar(self, bar: PyBarData) -> None: ...
    @property
    def open(self) -> List[float]: ...
    @property
    def high(self) -> List[float]: ...
    @property
    def low(self) -> List[float]: ...
    @property
    def close(self) -> List[float]: ...
    @property
    def volume(self) -> List[float]: ...
    @property
    def turnover(self) -> List[float]: ...
    @property
    def open_interest(self) -> List[float]: ...

    def sma(self, n: int, array: bool = ...) -> Union[float, List[float]]: ...
    def ema(self, n: int, array: bool = ...) -> Union[float, List[float]]: ...
    def rsi(self, n: int, array: bool = ...) -> Union[float, List[float]]: ...
    def macd(
        self,
        fast_period: int = ...,
        slow_period: int = ...,
        signal_period: int = ...,
        array: bool = ...,
    ) -> Tuple[Union[float, List[float]], Union[float, List[float]], Union[float, List[float]]]: ...
    def boll(
        self,
        n: int = ...,
        dev: float = ...,
        array: bool = ...,
    ) -> Tuple[Union[float, List[float]], Union[float, List[float]], Union[float, List[float]]]: ...
    def atr(self, n: int, array: bool = ...) -> Union[float, List[float]]: ...
    def kdj(
        self,
        n: int = ...,
        m1: int = ...,
        m2: int = ...,
        array: bool = ...,
    ) -> Tuple[Union[float, List[float]], Union[float, List[float]], Union[float, List[float]]]: ...


# ---------------------------------------------------------------------------
# Backtesting
# ---------------------------------------------------------------------------

class PyBarData:
    """Typed bar data for backtesting."""

    gateway_name: str
    symbol: str
    exchange: str
    open_price: float
    high_price: float
    low_price: float
    close_price: float
    volume: float
    turnover: float
    open_interest: float

    @property
    def datetime(self) -> datetime: ...

    @datetime.setter
    def datetime(self, value: Union[str, datetime]) -> None: ...

    def __init__(
        self,
        gateway_name: str = ...,
        symbol: str = ...,
        exchange: str = ...,
        datetime: str = ...,
        open_price: float = ...,
        high_price: float = ...,
        low_price: float = ...,
        close_price: float = ...,
        volume: float = ...,
        turnover: float = ...,
        open_interest: float = ...,
        interval: str = ...,
    ) -> None: ...

    def __getitem__(self, key: str) -> Any: ...
    def get(self, key: str, default_value: Any = ...) -> Any: ...


class PyBacktestingStatistics:
    """Backtesting performance statistics."""

    start_date: str
    end_date: str
    total_days: int
    profit_days: int
    loss_days: int
    capital: float
    end_balance: float
    total_return: float
    annual_return: float
    max_drawdown: float
    max_ddpercent: float
    sharpe_ratio: float
    return_drawdown_ratio: float
    total_trade_count: int
    daily_trade_count: float
    total_commission: float
    daily_commission: float


class PyBacktestingEngine:
    """Python wrapper for the Rust backtesting engine."""

    def __init__(self) -> None: ...

    def set_risk_manager(self, risk_manager: PyRiskManager) -> None: ...
    def clear_data(self) -> None: ...
    def set_fill_model(self, model_name: str) -> None: ...

    def set_parameters(
        self,
        vt_symbol: str,
        interval: str,
        start: str,
        end: str,
        rate: float,
        slippage: float,
        size: float,
        pricetick: float,
        capital: float,
        mode: Optional[str] = ...,
    ) -> None: ...

    def set_history_data(self, bars: List[PyBarData]) -> None: ...
    def add_strategy(self, strategy: Strategy, setting: Optional[Dict[str, str]] = ...) -> None: ...
    def load_data(self) -> None: ...
    def run_backtesting(self) -> None: ...
    def calculate_result(self) -> List[Dict[str, Any]]: ...
    def calculate_statistics(self) -> PyBacktestingStatistics: ...
    def show_chart(self) -> None: ...
    def get_all_results(self) -> List[Dict[str, Any]]: ...
    def get_daily_results(self) -> List[Dict[str, Any]]: ...
    def get_pnl_data(self) -> Dict[str, Any]: ...
    def get_strategy_state(self) -> str: ...
    def get_pos(self, vt_symbol: str) -> float: ...
    def get_order_count(self) -> int: ...
    def get_trade_count(self) -> int: ...


# ---------------------------------------------------------------------------
# Portfolio
# ---------------------------------------------------------------------------

class PyPosition:
    """Position tracking for a single symbol."""

    vt_symbol: str
    direction: str
    volume: float
    frozen: float
    price: float
    pnl: float

class PositionSnapshot:
    """Snapshot of portfolio positions."""
    positions: Dict[str, PyPosition]
    total_pnl: float
    timestamp: str

class PortfolioState:
    """Mutable portfolio state shared across strategies."""
    positions: Dict[str, PyPosition]

class PortfolioFacade:
    """Read-only facade for querying account/position state."""

    def get_position(self, vt_symbol: str) -> Optional[PyPosition]: ...
    def get_all_positions(self) -> List[PyPosition]: ...
    def get_account_balance(self) -> float: ...
    def get_total_pnl(self) -> float: ...

class PyPortfolioStatistics:
    """Portfolio-level performance statistics."""

    total_return: float
    sharpe_ratio: float
    max_drawdown: float
    win_rate: float
    total_trades: int


# ---------------------------------------------------------------------------
# Message bus
# ---------------------------------------------------------------------------

class PyMessage:
    """Message on the inter-strategy message bus."""

    topic: str
    data: Any
    sender: str
    timestamp: str

class MessageBus:
    """Pub/sub message bus for inter-strategy communication."""

    def publish(self, topic: str, data: Any) -> None: ...
    def subscribe(self, topic: str, handler: Any) -> None: ...
    def unsubscribe(self, topic: str) -> None: ...


# ---------------------------------------------------------------------------
# Risk manager
# ---------------------------------------------------------------------------

class PyRiskConfig:
    """Risk management configuration."""
    max_order_size: float
    max_position_size: float
    max_daily_loss: float
    max_order_rate: float

class PyRiskCheckResult:
    """Result of a risk check."""
    approved: bool
    reason: str

class PyRiskManager:
    """Python-facing risk manager."""

    def __init__(self, config: Optional[PyRiskConfig] = ...) -> None: ...
    def check_order(self, order: PyOrder) -> PyRiskCheckResult: ...
    def get_config(self) -> PyRiskConfig: ...


# ---------------------------------------------------------------------------
# Offset converter
# ---------------------------------------------------------------------------

class PyOrderRequest:
    """Typed order request with offset conversion support."""
    vt_symbol: str
    direction: str
    offset: str
    order_type: str
    price: float
    volume: float
    reference: str

class PyOffsetConverter:
    """Converts offset modes based on exchange rules."""

    def convert_offset(self, request: PyOrderRequest) -> PyOrderRequest: ...
    def requires_offset(self, vt_symbol: str) -> bool: ...
    def get_offsets(self, vt_symbol: str) -> List[str]: ...


# ---------------------------------------------------------------------------
# Stop order engine
# ---------------------------------------------------------------------------

class PyStopOrder:
    """Conditional (stop) order tracked by the engine."""
    stop_orderid: str
    vt_symbol: str
    direction: str
    offset: str
    price: float
    volume: float
    stop_price: float
    order_type: str
    status: str
    strategy_name: str

class PyStopOrderEngine:
    """Engine for managing conditional stop orders."""

    def add_stop_order(
        self,
        vt_symbol: str,
        direction: str,
        price: float,
        volume: float,
        stop_price: float,
        offset: str = ...,
        order_type: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def cancel_stop_order(self, stop_orderid: str) -> None: ...
    def get_stop_order(self, stop_orderid: str) -> Optional[PyStopOrder]: ...
    def get_all_stop_orders(self) -> List[PyStopOrder]: ...


# ---------------------------------------------------------------------------
# Bracket order engine
# ---------------------------------------------------------------------------

class PyChildOrderInfo:
    """Info about a child order in a bracket group."""
    role: str
    vt_orderid: Optional[str]
    request: PyOrderRequest
    filled_volume: float
    is_active: bool

class PyBracketOrderGroup:
    """A group of contingent orders (bracket/OCO/OTO)."""
    group_id: str
    group_type: str  # "bracket", "oco", "oto"
    state: str  # "pending", "entry_active", "secondary_active", "completed", "cancelled"
    entry_order: Optional[PyChildOrderInfo]
    take_profit_order: Optional[PyChildOrderInfo]
    stop_loss_order: Optional[PyChildOrderInfo]
    strategy_name: str

class PyBracketOrderEngine:
    """Engine for managing bracket/OCO/OTO order groups."""

    def add_bracket(
        self,
        vt_symbol: str,
        direction: str,
        entry_price: float,
        take_profit_price: float,
        stop_loss_price: float,
        volume: float,
        offset: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_oco(
        self,
        vt_symbol: str,
        price_1: float,
        price_2: float,
        volume: float,
        direction: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_oto(
        self,
        vt_symbol: str,
        direction: str,
        trigger_price: float,
        secondary_price: float,
        volume: float,
        strategy_name: str = ...,
    ) -> str: ...
    def cancel_bracket(self, group_id: str) -> None: ...
    def get_bracket(self, group_id: str) -> Optional[PyBracketOrderGroup]: ...
    def get_all_brackets(self) -> List[PyBracketOrderGroup]: ...


# ---------------------------------------------------------------------------
# Order emulator
# ---------------------------------------------------------------------------

class PyEmulatedOrder:
    """An emulated order tracked by the OrderEmulator."""
    orderid: str
    order_type: str  # "trailing_stop_pct", "trailing_stop_abs", "mit", "lit", "iceberg", "pegged_best"
    vt_symbol: str
    direction: str
    offset: str
    volume: float
    trigger_price: Optional[float]
    limit_price: Optional[float]
    trail_pct: Optional[float]
    trail_abs: Optional[float]
    is_active: bool

class PyOrderEmulator:
    """Engine for emulating advanced order types not natively supported by exchanges."""

    def add_trailing_stop_pct(
        self,
        vt_symbol: str,
        direction: str,
        volume: float,
        trail_pct: float,
        offset: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_trailing_stop_abs(
        self,
        vt_symbol: str,
        direction: str,
        volume: float,
        trail_abs: float,
        offset: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_mit(
        self,
        vt_symbol: str,
        direction: str,
        volume: float,
        trigger_price: float,
        offset: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_lit(
        self,
        vt_symbol: str,
        direction: str,
        volume: float,
        trigger_price: float,
        limit_price: float,
        offset: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_iceberg(
        self,
        vt_symbol: str,
        direction: str,
        volume: float,
        display_volume: float,
        offset: str = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def add_pegged_best(
        self,
        vt_symbol: str,
        direction: str,
        volume: float,
        offset: float = ...,
        strategy_name: str = ...,
    ) -> str: ...
    def cancel_emulated_order(self, orderid: str) -> None: ...
    def get_emulated_order(self, orderid: str) -> Optional[PyEmulatedOrder]: ...
    def get_all_emulated_orders(self) -> List[PyEmulatedOrder]: ...


# ---------------------------------------------------------------------------
# Sync bar generator
# ---------------------------------------------------------------------------

class PySyncBarGenerator:
    """Synchronized bar generator for multi-timeframe strategies."""

    def __init__(self, on_bar_func: Any, window: int = ..., on_window_bar_func: Any = ...) -> None: ...
    def update_tick(self, tick: PyTickData) -> None: ...
    def update_bar(self, bar: PyBarData) -> None: ...

class PySynchronizedBars:
    """Multi-timeframe synchronized bar manager."""

    def __init__(self) -> None: ...
    def add_symbol(self, vt_symbol: str, interval: str, on_bar: Any) -> None: ...
    def update_tick(self, tick: PyTickData) -> None: ...
    def update_bar(self, bar: PyBarData) -> None: ...
