"""
CTA Strategy utilities for vnrs

Provides Python implementations of BarGenerator and ArrayManager
that are compatible with vnpy's CTA strategy interface.

These utilities allow vnpy-style strategies to work with vnrs
without depending on vnpy_ctastrategy.
"""

import math
from collections import deque
from typing import Callable, Optional


class BarGenerator:
    """
    Bar generator that synthesizes X-minute bars from 1-minute bars or ticks.

    Compatible with vnpy's BarGenerator interface:
        bg = BarGenerator(self.on_bar, window=15, on_window_bar=self.on_15min_bar)
    """

    def __init__(
        self,
        on_bar: Callable,
        window: int = 0,
        on_window_bar: Optional[Callable] = None,
    ):
        self.on_bar = on_bar
        self.window = window
        self.on_window_bar = on_window_bar

        self._window_bar = None
        self._window_count = 0

    def update_tick(self, tick) -> None:
        """Update with tick data (vnpy compat stub)."""
        # In vnrs, ticks are handled by the engine which produces 1-min bars
        # This is a no-op for backtesting; live trading may need implementation
        pass

    def update_bar(self, bar: dict) -> None:
        """
        Update with a new 1-minute bar.

        If no window is set, passes bar directly to on_bar callback.
        If window is set, accumulates bars and calls on_window_bar when complete.
        """
        if self.window == 0:
            self.on_bar(bar)
            return

        # Accumulate window bars
        if self._window_bar is None:
            self._window_bar = dict(bar)  # shallow copy
        else:
            # Merge OHLCV
            self._window_bar["high_price"] = max(
                self._window_bar["high_price"], bar["high_price"]
            )
            self._window_bar["low_price"] = min(
                self._window_bar["low_price"], bar["low_price"]
            )
            self._window_bar["close_price"] = bar["close_price"]
            self._window_bar["volume"] = self._window_bar.get("volume", 0) + bar.get(
                "volume", 0
            )
            self._window_bar["turnover"] = self._window_bar.get(
                "turnover", 0
            ) + bar.get("turnover", 0)
            self._window_bar["open_interest"] = bar.get("open_interest", 0)

        self._window_count += 1

        if self._window_count >= self.window:
            if self.on_window_bar:
                self.on_window_bar(self._window_bar)
            self._window_bar = None
            self._window_count = 0


class ArrayManager:
    """
    Time series container for bar data with technical indicator calculations.

    Compatible with vnpy's ArrayManager interface:
        am = ArrayManager(size=100)
        am.update_bar(bar)
        if am.inited:
            atr_value = am.atr(22)
            sma_array = am.sma(10, array=True)
    """

    def __init__(self, size: int = 100):
        self.size = size
        self.count = 0
        self.inited = False

        self._open = deque(maxlen=size)
        self._high = deque(maxlen=size)
        self._low = deque(maxlen=size)
        self._close = deque(maxlen=size)
        self._volume = deque(maxlen=size)
        self._turnover = deque(maxlen=size)
        self._open_interest = deque(maxlen=size)

        # Pre-fill with zeros
        for _ in range(size):
            self._open.append(0.0)
            self._high.append(0.0)
            self._low.append(0.0)
            self._close.append(0.0)
            self._volume.append(0.0)
            self._turnover.append(0.0)
            self._open_interest.append(0.0)

    def update_bar(self, bar: dict) -> None:
        """Add new bar data."""
        self.count += 1
        if self.count >= self.size:
            self.inited = True

        self._open.append(bar.get("open_price", bar.get("open", 0.0)))
        self._high.append(bar.get("high_price", bar.get("high", 0.0)))
        self._low.append(bar.get("low_price", bar.get("low", 0.0)))
        self._close.append(bar.get("close_price", bar.get("close", 0.0)))
        self._volume.append(bar.get("volume", 0.0))
        self._turnover.append(bar.get("turnover", 0.0))
        self._open_interest.append(bar.get("open_interest", 0.0))

    @property
    def open(self):
        return list(self._open)

    @property
    def high(self):
        return list(self._high)

    @property
    def low(self):
        return list(self._low)

    @property
    def close(self):
        return list(self._close)

    @property
    def volume(self):
        return list(self._volume)

    # ==================== Moving Averages ====================

    def sma(self, n: int, array: bool = False):
        """Simple Moving Average."""
        data = list(self._close)
        result = self._calc_sma(data, n)
        if array:
            return result
        return result[-1] if result else 0.0

    def ema(self, n: int, array: bool = False):
        """Exponential Moving Average."""
        data = list(self._close)
        result = self._calc_ema(data, n)
        if array:
            return result
        return result[-1] if result else 0.0

    def _calc_sma(self, data: list, n: int) -> list:
        """Calculate SMA for entire array."""
        if n <= 0 or len(data) < n:
            return [0.0] * len(data)
        result = []
        for i in range(len(data)):
            if i < n - 1:
                result.append(0.0)
            else:
                window = data[i - n + 1 : i + 1]
                result.append(sum(window) / n)
        return result

    def _calc_ema(self, data: list, n: int) -> list:
        """Calculate EMA for entire array."""
        if n <= 0 or len(data) == 0:
            return [0.0] * len(data)
        alpha = 2.0 / (n + 1)
        result = [0.0] * len(data)
        # Start EMA with first value
        result[0] = data[0]
        for i in range(1, len(data)):
            result[i] = alpha * data[i] + (1 - alpha) * result[i - 1]
        return result

    # ==================== Volatility Indicators ====================

    def atr(self, n: int, array: bool = False):
        """Average True Range."""
        highs = list(self._high)
        lows = list(self._low)
        closes = list(self._close)

        if len(highs) < 2 or n <= 0:
            return [0.0] * len(highs) if array else 0.0

        # Calculate True Range
        tr = [0.0] * len(highs)
        tr[0] = highs[0] - lows[0]
        for i in range(1, len(highs)):
            tr[i] = max(
                highs[i] - lows[i],
                abs(highs[i] - closes[i - 1]),
                abs(lows[i] - closes[i - 1]),
            )

        result = self._calc_ema(tr, n)
        if array:
            return result
        return result[-1] if result else 0.0

    # ==================== Momentum Indicators ====================

    def rsi(self, n: int, array: bool = False):
        """Relative Strength Index."""
        data = list(self._close)
        if n <= 0 or len(data) < n + 1:
            return [0.0] * len(data) if array else 0.0

        result = [0.0] * len(data)
        gains = [0.0] * len(data)
        losses = [0.0] * len(data)

        for i in range(1, len(data)):
            change = data[i] - data[i - 1]
            gains[i] = max(change, 0.0)
            losses[i] = max(-change, 0.0)

        # Use Wilder's smoothing
        avg_gain = sum(gains[1 : n + 1]) / n
        avg_loss = sum(losses[1 : n + 1]) / n

        for i in range(n + 1):
            result[i] = 0.0

        if avg_loss == 0:
            result[n] = 100.0
        else:
            rs = avg_gain / avg_loss
            result[n] = 100.0 - (100.0 / (1.0 + rs))

        for i in range(n + 1, len(data)):
            avg_gain = (avg_gain * (n - 1) + gains[i]) / n
            avg_loss = (avg_loss * (n - 1) + losses[i]) / n
            if avg_loss == 0:
                result[i] = 100.0
            else:
                rs = avg_gain / avg_loss
                result[i] = 100.0 - (100.0 / (1.0 + rs))

        if array:
            return result
        return result[-1] if result else 0.0

    # ==================== Channel Indicators ====================

    def boll(self, n: int, dev: float, array: bool = False):
        """
        Bollinger Bands.
        Returns (upper, lower) or arrays.
        """
        sma_arr = self._calc_sma(list(self._close), n)
        std_arr = self._calc_std(list(self._close), n)

        upper = [s + dev * d for s, d in zip(sma_arr, std_arr)]
        lower = [s - dev * d for s, d in zip(sma_arr, std_arr)]

        if array:
            return upper, lower
        return upper[-1], lower[-1]

    def keltner(self, n: int, dev: float, array: bool = False):
        """
        Keltner Channel.
        Returns (upper, lower) or arrays.
        """
        ema_arr = self._calc_ema(list(self._close), n)
        atr_arr = self.atr(n, array=True)

        upper = [e + dev * a for e, a in zip(ema_arr, atr_arr)]
        lower = [e - dev * a for e, a in zip(ema_arr, atr_arr)]

        if array:
            return upper, lower
        return upper[-1], lower[-1]

    def donchian(self, n: int, array: bool = False):
        """
        Donchian Channel.
        Returns (upper, lower) or arrays.
        """
        highs = list(self._high)
        lows = list(self._low)

        if n <= 0 or len(highs) < n:
            return ([0.0] * len(highs), [0.0] * len(lows)) if array else (0.0, 0.0)

        upper = [0.0] * len(highs)
        lower = [0.0] * len(lows)

        for i in range(len(highs)):
            if i < n - 1:
                upper[i] = 0.0
                lower[i] = 0.0
            else:
                upper[i] = max(highs[i - n + 1 : i + 1])
                lower[i] = min(lows[i - n + 1 : i + 1])

        if array:
            return upper, lower
        return upper[-1], lower[-1]

    # ==================== Oscillators ====================

    def cci(self, n: int, array: bool = False):
        """Commodity Channel Index."""
        highs = list(self._high)
        lows = list(self._low)
        closes = list(self._close)

        if n <= 0 or len(closes) < n:
            return [0.0] * len(closes) if array else 0.0

        tp = [(h + l + c) / 3.0 for h, l, c in zip(highs, lows, closes)]
        result = [0.0] * len(closes)

        for i in range(n - 1, len(closes)):
            tp_slice = tp[i - n + 1 : i + 1]
            tp_mean = sum(tp_slice) / n
            mean_dev = sum(abs(t - tp_mean) for t in tp_slice) / n
            if mean_dev > 0:
                result[i] = (tp[i] - tp_mean) / (0.015 * mean_dev)

        if array:
            return result
        return result[-1]

    # ==================== Helper ====================

    def _calc_std(self, data: list, n: int) -> list:
        """Calculate rolling standard deviation."""
        result = [0.0] * len(data)
        if n <= 0 or len(data) < n:
            return result
        for i in range(n - 1, len(data)):
            window = data[i - n + 1 : i + 1]
            mean = sum(window) / n
            variance = sum((x - mean) ** 2 for x in window) / n
            result[i] = math.sqrt(variance)
        return result


class CtaSignal:
    """
    Base class for signal components used in multi-signal CTA strategies.

    Compatible with vnpy's CtaSignal interface. Each signal maintains
    its own signal position (1=long, -1=short, 0=neutral) which can
    be combined by a parent strategy.

    Usage:
        class MySignal(CtaSignal):
            def on_bar(self, bar):
                if condition:
                    self.set_signal_pos(1)
                else:
                    self.set_signal_pos(0)
    """

    def __init__(self):
        self._signal_pos = 0

    def set_signal_pos(self, pos: int) -> None:
        """Set the signal position (1=long, -1=short, 0=neutral)."""
        self._signal_pos = pos

    def get_signal_pos(self) -> int:
        """Return current signal position."""
        return self._signal_pos

    def on_tick(self, tick) -> None:
        """Handle tick data. Override in subclass."""
        pass

    def on_bar(self, bar) -> None:
        """Handle bar data. Override in subclass."""
        pass


from trade_engine import CtaStrategy


class TargetPosTemplate(CtaStrategy):
    """
    Strategy base class that manages position by target position.

    Compatible with vnpy's TargetPosTemplate. Instead of manually
    sending buy/sell/short/cover orders, strategies set a target
    position and the template handles order execution automatically.

    Usage:
        class MyTargetStrategy(TargetPosTemplate):
            def on_bar(self, bar):
                super().on_bar(bar)
                if condition:
                    self.set_target_pos(1)
                else:
                    self.set_target_pos(0)
    """

    def __init__(self, engine, strategy_name: str, vt_symbol: str, setting: dict):
        super().__init__(engine, strategy_name, vt_symbol, setting)
        self._target_pos = 0.0
        self._last_price = 0.0

    def set_target_pos(self, target_pos: float) -> None:
        """Set the desired target position for self.vt_symbol."""
        self._target_pos = target_pos

    def get_target_pos(self) -> float:
        """Return current target position."""
        return self._target_pos

    def on_tick(self, tick) -> None:
        """Handle tick: delegate to parent then check position."""
        super().on_tick(tick)
        # Track last price from tick
        if hasattr(tick, "last_price"):
            self._last_price = tick.last_price
        elif isinstance(tick, dict):
            self._last_price = tick.get("last_price", self._last_price)
        self._check_position()

    def on_bar(self, bar) -> None:
        """Handle bar: delegate to parent then check position."""
        super().on_bar(bar)
        # Track last price from bar
        if hasattr(bar, "close_price"):
            self._last_price = bar.close_price
        elif isinstance(bar, dict):
            self._last_price = bar.get("close_price", bar.get("close", 0))
        self._check_position()

    def _check_position(self) -> None:
        """Compare current position with target and send orders to align."""
        pos = self.pos
        target = self._target_pos
        vt_symbol = self.vt_symbol

        if pos == target:
            return

        if target > 0:
            if pos < 0:
                self.cover(vt_symbol, self._last_price, abs(pos))
            if pos < target:
                self.buy(vt_symbol, self._last_price, abs(target - pos))
        elif target < 0:
            if pos > 0:
                self.sell(vt_symbol, self._last_price, abs(pos))
            if pos > target:
                self.short(vt_symbol, self._last_price, abs(target - pos))
        else:
            if pos > 0:
                self.sell(vt_symbol, self._last_price, abs(pos))
            elif pos < 0:
                self.cover(vt_symbol, self._last_price, abs(pos))
