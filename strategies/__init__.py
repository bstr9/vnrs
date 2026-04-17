"""
CTA Strategies for vnrs

This package contains CTA strategies migrated from vnpy_ctastrategy
and adapted for the vnrs trading engine.

Usage:
    from strategies.double_ma_strategy import DoubleMaStrategy
    from strategies.atr_rsi_strategy import AtrRsiStrategy
    from strategies.turtle_signal_strategy import TurtleSignalStrategy

All strategies inherit from trade_engine.CtaStrategy, which provides
vnpy CtaTemplate compatibility.

Available strategies:
- DoubleMaStrategy: Dual moving average crossover
- AtrRsiStrategy: ATR volatility filter + RSI entry signals
- DualThrustStrategy: Daily breakout system
- TurtleSignalStrategy: Donchian channel breakout with ATR stop
- TestStrategy: Test order operations
"""

from .double_ma_strategy import DoubleMaStrategy
from .atr_rsi_strategy import AtrRsiStrategy
from .dual_thrust_strategy import DualThrustStrategy
from .turtle_signal_strategy import TurtleSignalStrategy
from .test_strategy import TestStrategy

__all__ = [
    "DoubleMaStrategy",
    "AtrRsiStrategy",
    "DualThrustStrategy",
    "TurtleSignalStrategy",
    "TestStrategy",
    "BarGenerator",
    "ArrayManager",
]

# Re-export utilities for convenience
from .cta_utils import BarGenerator, ArrayManager
