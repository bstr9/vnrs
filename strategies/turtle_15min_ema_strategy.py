"""
Turtle 15Min EMA Signal Strategy

Migrated from vnpy_ctastrategy's new_strategy.py.
Uses dual EMA crossover for entry signals and ATR-based stop loss for exit,
with multiple stop modes, stop calculation modes, channel exit, and trailing stop.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- Replace talib.EMA with ArrayManager's ema() method for initial calculation
- bar.close_price -> bar["close_price"], bar.high_price -> bar["high_price"], etc.
- Order calls: self.buy(price, vol, True) -> self.buy(self.vt_symbol, price, vol)
- Direction enum imported from trade_engine
- stop=True (stop order) parameter removed; orders sent as limit orders
"""

import math
from trade_engine import CtaStrategy, Direction
from cta_utils import BarGenerator, ArrayManager


class Turtle15MinEmaSignalStrategy(CtaStrategy):
    """
    Dual EMA crossover turtle strategy variant.
    Entry signal: MA1 (short-term EMA) crosses MA2 (long-term EMA)
    - Long entry: MA1 crosses above MA2
    - Short entry: MA1 crosses below MA2

    Stop mode (stop_mode):
    - 1: No stop order during pyramid, wait for next bar after fill
    - 2: Stop uses market order for immediate execution
    - 3: Pyramid signal priority, no stop when pyramid signal present

    Stop calculation mode (stop_calc_mode):
    - 1: First entry price only
    - 2: Weighted average cost
    - 3: Last entry price
    - 4: Most conservative entry price (classic turtle)

    Channel exit (use_channel_exit):
    - True: Use Donchian channel + ATR stop (more conservative)
    - False: ATR stop only

    ATR stop multiplier (atr_multiplier):
    - Stop distance = entry price +/- atr_multiplier * ATR
    - Default 2.0, recommended range 1.5-3.0

    Max loss stop (max_loss_pct):
    - Force market exit when unrealized loss exceeds this percentage
    - Default 0 means disabled, recommended 3-10%

    Signal exit (use_signal_exit):
    - True: Exit on reverse crossover signal
    - False: Exit on stop orders only (default)

    Trailing stop (use_trailing_stop):
    - True: Move stop in favorable direction when profitable
    - trailing_pct: Percentage retracement from peak to trigger stop
    """

    author = "用Python的交易员"

    ma1_window: int = 7  # Short-term EMA period
    ma2_window: int = 25  # Long-term EMA period
    exit_window: int = 10
    atr_window: int = 20
    fixed_size: int = 1
    stop_mode: int = (
        1  # Stop mode: 1-no stop during pyramid 2-market stop 3-pyramid priority
    )
    stop_calc_mode: int = (
        2  # Stop calc mode: 1-first entry 2-avg cost 3-last entry 4-most conservative
    )
    use_channel_exit: bool = True  # Use channel exit (False = ATR stop only)
    atr_multiplier: float = 2.0  # ATR stop multiplier, recommended 1.5-3.0
    max_loss_pct: float = 15.0  # Max loss percentage for forced exit (0 = disabled)
    use_signal_exit: bool = False  # Exit on reverse signal (default off)
    use_trailing_stop: bool = False  # Use trailing stop
    trailing_pct: float = 2.0  # Trailing stop retracement percentage
    only_long: bool = True  # Long only, no shorting (default long only)
    max_units: int = 4  # Max pyramid units (limit position risk)

    ma1_value: float = 0  # Short-term EMA current value
    ma2_value: float = 0  # Long-term EMA current value
    ma1_last: float = 0  # Short-term EMA previous bar value
    ma2_last: float = 0  # Long-term EMA previous bar value
    exit_up: float = 0
    exit_down: float = 0
    atr_value: float = 0
    long_entry: float = 0
    short_entry: float = 0
    long_stop: float = 0
    short_stop: float = 0

    # Last placed stop price (for determining if cancel/update needed)
    last_long_stop: float = 0
    last_short_stop: float = 0

    # Stop calculation tracking variables
    first_entry_price: float = 0  # First entry price
    total_cost: float = 0  # Total cost (price * volume)
    total_volume: float = 0  # Total volume
    lowest_entry_price: float = 0  # Lowest entry price (for long)
    highest_entry_price: float = 0  # Highest entry price (for short)

    # Trailing stop tracking variables
    highest_price_since_entry: float = 0  # Highest price since entry (for long)
    lowest_price_since_entry: float = 0  # Lowest price since entry (for short)

    # EMA incremental calculation parameters
    ma1_alpha: float = 0  # MA1 smoothing coefficient
    ma2_alpha: float = 0  # MA2 smoothing coefficient
    ema_inited: bool = False  # Whether EMA has been initialized

    parameters = [
        "ma1_window",
        "ma2_window",
        "exit_window",
        "atr_window",
        "fixed_size",
        "stop_mode",
        "stop_calc_mode",
        "use_channel_exit",
        "atr_multiplier",
        "max_loss_pct",
        "use_signal_exit",
        "use_trailing_stop",
        "trailing_pct",
        "only_long",
        "max_units",
    ]
    variables = ["ma1_value", "ma2_value", "exit_up", "exit_down", "atr_value"]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

        # Calculate EMA smoothing coefficient: alpha = 2 / (N + 1)
        self.ma1_alpha = 2.0 / (self.ma1_window + 1)
        self.ma2_alpha = 2.0 / (self.ma2_window + 1)

        self.load_bar(max(self.ma1_window, self.ma2_window) + 10)

    def on_start(self) -> None:
        """
        Callback when strategy is started.
        """
        self.write_log("策略启动")

    def on_stop(self) -> None:
        """
        Callback when strategy is stopped.
        """
        self.write_log("策略停止")

    def on_tick(self, tick) -> None:
        """
        Callback of new tick data update.
        """
        self.bg.update_tick(tick)

    def on_bar(self, bar) -> None:
        """
        Callback of new bar data update.
        """
        self.am.update_bar(bar)
        if not self.am.inited:
            return

        # Initial EMA calculation using ArrayManager's ema() method
        if not self.ema_inited:
            ma1_array = self.am.ema(self.ma1_window, array=True)
            ma2_array = self.am.ema(self.ma2_window, array=True)

            self.ma1_last = ma1_array[-2]
            self.ma2_last = ma2_array[-2]
            self.ma1_value = ma1_array[-1]
            self.ma2_value = ma2_array[-1]

            if math.isnan(self.ma1_value) or math.isnan(self.ma2_value):
                return

            self.ema_inited = True
        else:
            # Incremental update: EMA_new = alpha * Price + (1-alpha) * EMA_old
            close_price = bar["close_price"]
            self.ma1_last = self.ma1_value
            self.ma2_last = self.ma2_value
            self.ma1_value = (
                self.ma1_alpha * close_price + (1 - self.ma1_alpha) * self.ma1_last
            )
            self.ma2_value = (
                self.ma2_alpha * close_price + (1 - self.ma2_alpha) * self.ma2_last
            )

        # Determine EMA crossover signals
        # Golden cross: MA1 crosses above MA2
        cross_above = (self.ma1_last <= self.ma2_last) and (
            self.ma1_value > self.ma2_value
        )
        # Death cross: MA1 crosses below MA2
        cross_below = (self.ma1_last >= self.ma2_last) and (
            self.ma1_value < self.ma2_value
        )

        # Calculate exit channel and ATR (update every bar to avoid stale values)
        self.exit_up, self.exit_down = self.am.donchian(self.exit_window)
        atr_temp = self.am.atr(self.atr_window)

        # Check if ATR is valid
        if not math.isnan(atr_temp) and atr_temp > 0:
            self.atr_value = atr_temp

        # Skip if ATR is invalid
        if self.atr_value == 0 or math.isnan(self.atr_value):
            return

        # Calculate current stop prices
        current_long_stop = 0.0
        current_short_stop = 0.0

        if self.pos > 0 and self.long_stop > 0:
            if self.use_channel_exit:
                current_long_stop = max(self.long_stop, self.exit_down)
            else:
                current_long_stop = self.long_stop
        elif self.pos < 0 and self.short_stop > 0:
            if self.use_channel_exit:
                current_short_stop = min(self.short_stop, self.exit_up)
            else:
                current_short_stop = self.short_stop

        # Determine if orders should be cancelled
        # Condition 1: Crossover signal appeared (need to place new entry/pyramid orders)
        # Condition 2: Stop price changed (need to update stop orders)
        should_cancel = False

        if cross_above or cross_below:
            should_cancel = True
        elif self.pos > 0 and current_long_stop != self.last_long_stop:
            should_cancel = True
        elif self.pos < 0 and current_short_stop != self.last_short_stop:
            should_cancel = True

        if should_cancel:
            self.cancel_all()

        # ========== Risk management checks (highest priority) ==========
        # Update trailing stop tracking prices
        if self.pos > 0:
            if (
                self.highest_price_since_entry == 0
                or bar["high_price"] > self.highest_price_since_entry
            ):
                self.highest_price_since_entry = bar["high_price"]
        elif self.pos < 0:
            if (
                self.lowest_price_since_entry == 0
                or bar["low_price"] < self.lowest_price_since_entry
            ):
                self.lowest_price_since_entry = bar["low_price"]

        # Check if forced exit is needed (three conditions)
        force_exit = False
        force_exit_reason = ""

        if self.pos > 0:
            # 1. Max loss stop
            if self.max_loss_pct > 0 and self.total_cost > 0 and self.total_volume > 0:
                avg_cost = self.total_cost / self.total_volume
                if avg_cost > 0:
                    loss_pct = (avg_cost - bar["close_price"]) / avg_cost * 100
                    if loss_pct >= self.max_loss_pct:
                        force_exit = True
                        force_exit_reason = f"最大亏损止损: 亏损{loss_pct:.2f}%"

            # 2. Reverse signal exit (death cross)
            if self.use_signal_exit and cross_below:
                force_exit = True
                force_exit_reason = "反向信号出场: 出现死叉"

            # 3. Trailing stop
            if self.use_trailing_stop and self.highest_price_since_entry > 0:
                trailing_stop_price = self.highest_price_since_entry * (
                    1 - self.trailing_pct / 100
                )
                if bar["close_price"] <= trailing_stop_price:
                    force_exit = True
                    force_exit_reason = f"移动止损: 从最高点{self.highest_price_since_entry:.2f}回撤超过{self.trailing_pct}%"

            # Force exit
            if force_exit:
                self.cancel_all()
                # NOTE: vnpy's stop=True (stop order) is not supported in vnrs.
                # Orders are sent as limit orders. The stop price logic is handled
                # by the strategy itself comparing bar prices to stop levels.
                self.sell(self.vt_symbol, bar["close_price"], abs(self.pos))
                self.write_log(force_exit_reason)
                self.put_event()
                return

        elif self.pos < 0:
            # 1. Max loss stop
            if self.max_loss_pct > 0 and self.total_cost > 0 and self.total_volume > 0:
                avg_cost = self.total_cost / self.total_volume
                if avg_cost > 0:
                    loss_pct = (bar["close_price"] - avg_cost) / avg_cost * 100
                    if loss_pct >= self.max_loss_pct:
                        force_exit = True
                        force_exit_reason = f"最大亏损止损: 亏损{loss_pct:.2f}%"

            # 2. Reverse signal exit (golden cross)
            if self.use_signal_exit and cross_above:
                force_exit = True
                force_exit_reason = "反向信号出场: 出现金叉"

            # 3. Trailing stop
            if self.use_trailing_stop and self.lowest_price_since_entry > 0:
                trailing_stop_price = self.lowest_price_since_entry * (
                    1 + self.trailing_pct / 100
                )
                if bar["close_price"] >= trailing_stop_price:
                    force_exit = True
                    force_exit_reason = f"移动止损: 从最低点{self.lowest_price_since_entry:.2f}反弹超过{self.trailing_pct}%"

            # Force exit
            if force_exit:
                self.cancel_all()
                # NOTE: vnpy's stop=True (stop order) is not supported in vnrs.
                # Orders are sent as limit orders. The stop price logic is handled
                # by the strategy itself comparing bar prices to stop levels.
                self.cover(self.vt_symbol, bar["close_price"], abs(self.pos))
                self.write_log(force_exit_reason)
                self.put_event()
                return

        # ========== Normal trading logic ==========
        if not self.pos:
            self.long_entry = 0
            self.short_entry = 0
            self.long_stop = 0
            self.short_stop = 0
            self.last_long_stop = 0
            self.last_short_stop = 0
            # Reset stop calculation tracking variables
            self.first_entry_price = 0
            self.total_cost = 0
            self.total_volume = 0
            self.lowest_entry_price = 0
            self.highest_entry_price = 0
            # Reset trailing stop tracking variables
            self.highest_price_since_entry = 0
            self.lowest_price_since_entry = 0

            # Golden cross -> open long
            if cross_above:
                self.send_buy_orders(bar["close_price"])
            # Death cross -> open short (only if shorting allowed)
            elif cross_below and not self.only_long:
                self.send_short_orders(bar["close_price"])

        elif self.pos > 0:
            # Holding long, golden cross -> pyramid
            if cross_above:
                self.send_buy_orders(bar["close_price"])

            # Handle stop orders based on stop mode
            if self.long_stop > 0:
                # Calculate stop price based on channel exit setting
                if self.use_channel_exit:
                    sell_price = max(self.long_stop, self.exit_down)
                else:
                    sell_price = self.long_stop

                # NOTE: vnpy's stop=True (stop order) is not supported in vnrs.
                # Orders are sent as limit orders. The stop price logic is handled
                # by the strategy itself comparing bar prices to stop levels.
                if self.stop_mode == 1:
                    # Mode 1: No stop order during pyramid, wait for next bar after fill
                    if not cross_above:
                        self.sell(self.vt_symbol, sell_price, abs(self.pos))
                        self.last_long_stop = sell_price

                elif self.stop_mode == 2:
                    # Mode 2: Stop uses market order
                    # Only send market order when price hits stop level
                    if bar["low_price"] <= sell_price:
                        self.sell(self.vt_symbol, bar["close_price"], abs(self.pos))
                    else:
                        # Not yet at stop level, place limit order
                        self.sell(self.vt_symbol, sell_price, abs(self.pos))
                        self.last_long_stop = sell_price

                elif self.stop_mode == 3:
                    # Mode 3: Pyramid signal priority, no stop when pyramid signal present
                    if not cross_above:
                        self.sell(self.vt_symbol, sell_price, abs(self.pos))
                        self.last_long_stop = sell_price

        elif self.pos < 0:
            # Holding short, death cross -> pyramid
            if cross_below:
                self.send_short_orders(bar["close_price"])

            # Handle stop orders based on stop mode
            if self.short_stop > 0:
                # Calculate stop price based on channel exit setting
                if self.use_channel_exit:
                    cover_price = min(self.short_stop, self.exit_up)
                else:
                    cover_price = self.short_stop

                # NOTE: vnpy's stop=True (stop order) is not supported in vnrs.
                # Orders are sent as limit orders. The stop price logic is handled
                # by the strategy itself comparing bar prices to stop levels.
                if self.stop_mode == 1:
                    # Mode 1: No stop order during pyramid, wait for next bar after fill
                    if not cross_below:
                        self.cover(self.vt_symbol, cover_price, abs(self.pos))
                        self.last_short_stop = cover_price

                elif self.stop_mode == 2:
                    # Mode 2: Stop uses market order
                    if bar["high_price"] >= cover_price:
                        self.cover(self.vt_symbol, bar["close_price"], abs(self.pos))
                    else:
                        # Not yet at stop level, place limit order
                        self.cover(self.vt_symbol, cover_price, abs(self.pos))
                        self.last_short_stop = cover_price

                elif self.stop_mode == 3:
                    # Mode 3: Pyramid signal priority, no stop when pyramid signal present
                    if not cross_below:
                        self.cover(self.vt_symbol, cover_price, abs(self.pos))
                        self.last_short_stop = cover_price

        self.put_event()

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        """
        # Extract trade fields - support both dict and object access
        if isinstance(trade, dict):
            trade_direction = trade.get("direction", "")
            trade_price = trade["price"]
            trade_volume = trade["volume"]
        else:
            trade_direction = trade.direction
            trade_price = trade.price
            trade_volume = trade.volume

        if trade_direction == Direction.LONG:
            # Update cost tracking variables
            if self.first_entry_price == 0:
                self.first_entry_price = trade_price

            self.total_cost += trade_price * trade_volume
            self.total_volume += trade_volume

            if self.lowest_entry_price == 0 or trade_price < self.lowest_entry_price:
                self.lowest_entry_price = trade_price

            # Calculate stop price based on stop calculation mode
            if self.stop_calc_mode == 1:
                # Mode 1: First entry price
                base_price = self.first_entry_price
            elif self.stop_calc_mode == 2:
                # Mode 2: Weighted average cost
                if self.total_volume > 0:
                    base_price = self.total_cost / self.total_volume
                else:
                    base_price = trade_price
            elif self.stop_calc_mode == 3:
                # Mode 3: Last entry price
                base_price = trade_price
            elif self.stop_calc_mode == 4:
                # Mode 4: Most conservative entry price (long uses lowest)
                base_price = self.lowest_entry_price
            else:
                base_price = trade_price

            self.long_entry = base_price
            # Only calculate stop price when ATR is valid
            if self.atr_value > 0:
                self.long_stop = self.long_entry - self.atr_multiplier * self.atr_value
            else:
                # ATR invalid, use fixed percentage stop (5%)
                self.long_stop = self.long_entry * 0.95

            # Initialize trailing stop tracking price
            if self.highest_price_since_entry == 0:
                self.highest_price_since_entry = trade_price

        else:  # Direction.SHORT
            # Update cost tracking variables
            if self.first_entry_price == 0:
                self.first_entry_price = trade_price

            self.total_cost += trade_price * trade_volume
            self.total_volume += trade_volume

            if self.highest_entry_price == 0 or trade_price > self.highest_entry_price:
                self.highest_entry_price = trade_price

            # Calculate stop price based on stop calculation mode
            if self.stop_calc_mode == 1:
                # Mode 1: First entry price
                base_price = self.first_entry_price
            elif self.stop_calc_mode == 2:
                # Mode 2: Weighted average cost
                if self.total_volume > 0:
                    base_price = self.total_cost / self.total_volume
                else:
                    base_price = trade_price
            elif self.stop_calc_mode == 3:
                # Mode 3: Last entry price
                base_price = trade_price
            elif self.stop_calc_mode == 4:
                # Mode 4: Most conservative entry price (short uses highest)
                base_price = self.highest_entry_price
            else:
                base_price = trade_price

            self.short_entry = base_price
            # Only calculate stop price when ATR is valid
            if self.atr_value > 0:
                self.short_stop = (
                    self.short_entry + self.atr_multiplier * self.atr_value
                )
            else:
                # ATR invalid, use fixed percentage stop (5%)
                self.short_stop = self.short_entry * 1.05

            # Initialize trailing stop tracking price
            if self.lowest_price_since_entry == 0:
                self.lowest_price_since_entry = trade_price

    def on_order(self, order) -> None:
        """
        Callback of new order data update.
        """
        pass

    def send_buy_orders(self, price: float) -> None:
        """Send buy entry/pyramid orders."""
        unit_count: int = int(abs(self.pos) / self.fixed_size)

        # Check max position limit
        if unit_count >= self.max_units:
            return

        # NOTE: vnpy's stop=True (stop order) is not supported in vnrs.
        # Orders are sent as limit orders. The stop price logic is handled
        # by the strategy itself comparing bar prices to stop levels.

        # Determine pyramid based on current position units
        if unit_count == 0:
            # First entry
            self.buy(self.vt_symbol, price, self.fixed_size)
        elif unit_count < self.max_units:
            # Pyramid conditions: price at least 0.5x ATR above previous entry
            if unit_count == 1:
                self.buy(self.vt_symbol, price + self.atr_value * 0.5, self.fixed_size)
            elif unit_count == 2:
                self.buy(self.vt_symbol, price + self.atr_value, self.fixed_size)
            elif unit_count == 3:
                self.buy(self.vt_symbol, price + self.atr_value * 1.5, self.fixed_size)

    def send_short_orders(self, price: float) -> None:
        """Send short entry/pyramid orders."""
        unit_count: int = int(abs(self.pos) / self.fixed_size)

        # Check max position limit
        if unit_count >= self.max_units:
            return

        # NOTE: vnpy's stop=True (stop order) is not supported in vnrs.
        # Orders are sent as limit orders. The stop price logic is handled
        # by the strategy itself comparing bar prices to stop levels.

        # Determine pyramid based on current position units
        if unit_count == 0:
            # First entry
            self.short(self.vt_symbol, price, self.fixed_size)
        elif unit_count < self.max_units:
            # Pyramid conditions: price at least 0.5x ATR below previous entry
            if unit_count == 1:
                self.short(
                    self.vt_symbol, price - self.atr_value * 0.5, self.fixed_size
                )
            elif unit_count == 2:
                self.short(self.vt_symbol, price - self.atr_value, self.fixed_size)
            elif unit_count == 3:
                self.short(
                    self.vt_symbol, price - self.atr_value * 1.5, self.fixed_size
                )
