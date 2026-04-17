"""
Bitcoin Spot Strategy

Migrated from vnpy_ctastrategy. Double EMA crossover strategy with
comprehensive risk management for spot Bitcoin trading.

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- Remove talib dependency, use ArrayManager's built-in EMA
- API adapted: self.buy(price, vol, stop=True) -> self.buy(vt_symbol, price, vol)
- Bar access: bar.close_price -> bar["close_price"] (dict-style)
- Direction imported from trade_engine
"""

import math
import datetime
from pathlib import Path

from trade_engine import CtaStrategy, Direction
from cta_utils import BarGenerator, ArrayManager


class BitcoinSpotStrategy(CtaStrategy):
    """
    Bitcoin spot trading strategy.
    Based on double EMA crossover signals, long-only.
    """

    author = "Modified by AI"

    # Parameter definitions
    ma1_window: int = 7  # Short-term EMA period
    ma2_window: int = 25  # Long-term EMA period
    exit_window: int = 10
    atr_window: int = 20
    fixed_size: float = 0.01  # Fixed order size (Bitcoin)
    atr_multiplier: float = 2.0  # ATR stop-loss multiplier
    max_loss_pct: float = 10.0  # Max loss percentage
    use_trailing_stop: bool = True  # Whether to use trailing stop
    trailing_pct: float = 2.0  # Trailing stop drawdown percentage
    max_units: int = 4  # Max pyramid units
    risk_per_unit: float = 0.02  # Risk percentage per unit
    max_drawdown_pct: float = 20.0  # Max drawdown percentage
    min_notional: float = 10.0  # Minimum notional (Binance spot requirement)
    use_channel_exit: bool = False  # Spot trading doesn't use channel exit
    stop_calc_mode: int = 2  # Stop calculation mode: only use average cost
    stop_mode: int = 1  # Stop mode: only use limit orders

    ma1_value: float = 0  # Short-term EMA current value
    ma2_value: float = 0  # Long-term EMA current value
    ma1_last: float = 0  # Short-term EMA previous bar value
    ma2_last: float = 0  # Long-term EMA previous bar value
    exit_up: float = 0
    exit_down: float = 0
    atr_value: float = 0
    long_entry: float = 0
    long_stop: float = 0

    # Last stop-loss order price (for determining if update needed)
    last_long_stop: float = 0

    # Stop-loss calculation variables
    first_entry_price: float = 0  # First entry price
    total_cost: float = 0  # Total cost (price * volume)
    total_volume: float = 0  # Total volume
    lowest_entry_price: float = 0  # Lowest entry price

    # Trailing stop variables
    highest_price_since_entry: float = 0  # Highest price since entry

    # EMA incremental calculation parameters
    ma1_alpha: float = 0  # MA1 smoothing coefficient
    ma2_alpha: float = 0  # MA2 smoothing coefficient
    ema_inited: bool = False  # Whether EMA is initialized

    # Pyramid condition enhancement
    last_cross_bar: int = 0  # Last cross signal bar index
    current_unit: int = 0  # Current position unit count

    # Order status tracking
    current_orders: dict = {}  # Current orders dict, key is order ID, value is order info

    # Capital management
    current_capital: float = 0  # Current capital
    daily_pnl: float = 0  # Daily PnL
    max_equity: float = 0  # Maximum equity
    current_drawdown: float = 0  # Current drawdown
    slippage: float = 0.001  # Slippage, default 0.1%
    log_file_path: str = ""  # Log file path

    parameters = [
        "ma1_window",
        "ma2_window",
        "exit_window",
        "atr_window",
        "fixed_size",
        "atr_multiplier",
        "max_loss_pct",
        "use_trailing_stop",
        "trailing_pct",
        "max_units",
        "risk_per_unit",
        "max_drawdown_pct",
        "min_notional",
        "use_channel_exit",
        "stop_calc_mode",
        "stop_mode",
    ]
    variables = [
        "ma1_value",
        "ma2_value",
        "exit_up",
        "exit_down",
        "atr_value",
        "current_unit",
        "current_capital",
        "current_drawdown",
    ]

    def get_capital(self):
        """Get initial capital (vnrs compat stub - returns 0, use configurable default)."""
        try:
            if self.engine:
                result = self.engine.call_method("get_capital")
                if result > 0:
                    return result
        except Exception:
            pass
        return 0

    def get_slippage(self):
        """Get slippage (vnrs compat stub - returns 0)."""
        try:
            if self.engine:
                return self.engine.call_method("get_slippage")
        except Exception:
            pass
        return 0

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self._init_log_file()
        self.write_log("Strategy initialized")

        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

        # Calculate EMA smoothing coefficient: alpha = 2 / (N + 1)
        self.ma1_alpha = 2.0 / (self.ma1_window + 1)
        self.ma2_alpha = 2.0 / (self.ma2_window + 1)

        # Boundary checking
        self.validate_parameters()

        # Key parameter check
        if self.fixed_size <= 0:
            self.write_log(
                "Critical error: fixed_size must be greater than 0, strategy cannot run"
            )
            raise ValueError("fixed_size must be greater than 0")

        # Get initial capital and slippage from engine
        initial_capital = self.get_capital()
        slippage = self.get_slippage()

        # Initialize capital and slippage
        self.current_capital = initial_capital if initial_capital > 0 else 100000.0
        self.max_equity = self.current_capital
        self.slippage = slippage if slippage > 0 else 0.001  # Default 0.1% slippage

        self.load_bar(max(self.ma1_window, self.ma2_window) + 10)

    def on_start(self) -> None:
        """
        Callback when strategy is started.
        """
        self.write_log("Strategy started")

    def on_stop(self) -> None:
        """
        Callback when strategy is stopped.
        """
        self.write_log("Strategy stopped")

    def _init_log_file(self) -> None:
        """Initialize strategy log file"""
        try:
            if not self.log_file_path:
                log_dir: Path = Path.cwd() / "logs"
                log_dir.mkdir(parents=True, exist_ok=True)
                timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
                filename = f"{self.__class__.__name__}_{self.strategy_name}_{self.vt_symbol}_{timestamp}.log"
                self.log_file_path = str(log_dir / filename)
                with open(self.log_file_path, "a", encoding="utf-8") as f:
                    f.write(f"# Strategy: {self.__class__.__name__}\n")
                    f.write(f"# Name: {self.strategy_name}\n")
                    f.write(f"# Symbol: {self.vt_symbol}\n")
                    f.write(
                        f"# Created: {datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n\n"
                    )
        except Exception:
            # Ignore initialization failure, keep regular log channel
            pass

    def _emit_log(self, msg: str) -> None:
        """Base log writing, shared by regular and structured logging"""
        try:
            if not self.log_file_path:
                self._init_log_file()
            ts = datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")
            line = f"{ts} | {msg}\n"
            if self.log_file_path:
                with open(self.log_file_path, "a", encoding="utf-8") as f:
                    f.write(line)
        except Exception:
            pass

        try:
            super().write_log(msg)
        except Exception:
            pass

    def write_log(self, msg: str) -> None:
        """Override write_log: write to both file and regular log channel"""
        self._emit_log(msg)

    def log_tag(self, tag: str, msg: str) -> None:
        """Structured log for post-processing/LLM analysis"""
        self._emit_log(f"[{tag}] {msg}")

    def validate_parameters(self):
        """Validate parameter boundaries"""
        if self.atr_multiplier < 1.5 or self.atr_multiplier > 3.0:
            self.atr_multiplier = 2.0
            self.write_log(
                "Warning: atr_multiplier should be in 1.5-3.0 range, reset to 2.0"
            )

        if self.max_units <= 0 or self.max_units > 6:
            self.max_units = 4
            self.write_log(
                "Warning: max_units out of reasonable range (1-6), reset to 4"
            )

        if self.risk_per_unit <= 0 or self.risk_per_unit > 0.1:
            self.risk_per_unit = 0.02
            self.write_log(
                "Warning: risk_per_unit should be in (0-0.1) range, reset to 0.02"
            )

        if self.max_loss_pct > 20:
            self.max_loss_pct = 10.0
            self.write_log("Warning: max_loss_pct too high, reset to 10.0")

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

        # Get bar values with dict-style access
        bar_datetime = bar.get("datetime", "")
        bar_open = bar["open_price"]
        bar_high = bar["high_price"]
        bar_low = bar["low_price"]
        bar_close = bar["close_price"]

        # [Log 1] Record basic state at bar start
        self.log_tag(
            "STATE",
            f"bar={bar_datetime} O={bar_open:.2f} H={bar_high:.2f} L={bar_low:.2f} C={bar_close:.2f} "
            f"pos={self.pos} units={self.current_unit} capital={self.current_capital:.2f} "
            f"equity={self.current_capital + self.pos * bar_close:.2f} dd={self.current_drawdown:.2f}% max_eq={self.max_equity:.2f}",
        )

        # First initialization: use ArrayManager's EMA
        if not self.ema_inited:
            close_array = self.am.close
            ma1_array = self.am.ema(self.ma1_window, array=True)
            ma2_array = self.am.ema(self.ma2_window, array=True)

            # Check array length and validity
            if len(ma1_array) < 2 or len(ma2_array) < 2:
                return  # Insufficient data, skip this time

            self.ma1_last = ma1_array[-2] if not math.isnan(ma1_array[-2]) else 0
            self.ma2_last = ma2_array[-2] if not math.isnan(ma2_array[-2]) else 0
            self.ma1_value = ma1_array[-1] if not math.isnan(ma1_array[-1]) else 0
            self.ma2_value = ma2_array[-1] if not math.isnan(ma2_array[-1]) else 0

            # Check for NaN (ArrayManager returns NaN when data is insufficient)
            if (
                math.isnan(self.ma1_value)
                or math.isnan(self.ma2_value)
                or self.ma1_value == 0
                or self.ma2_value == 0
            ):
                return  # Insufficient data, skip this time

            self.ema_inited = True
        else:
            # Incremental update: EMA_new = alpha * Price + (1-alpha) * EMA_old
            # Significantly improves backtesting speed (avoids recalculating entire array each time)
            close_price = bar_close
            self.ma1_last = self.ma1_value
            self.ma2_last = self.ma2_value
            self.ma1_value = (
                self.ma1_alpha * close_price + (1 - self.ma1_alpha) * self.ma1_last
            )
            self.ma2_value = (
                self.ma2_alpha * close_price + (1 - self.ma2_alpha) * self.ma2_last
            )

        # Determine EMA cross signal
        # Golden cross: MA1 crosses above MA2
        cross_above = (self.ma1_last <= self.ma2_last) and (
            self.ma1_value > self.ma2_value
        )

        # Calculate exit channel and ATR (update each time to avoid stale values)
        self.exit_up, self.exit_down = self.am.donchian(self.exit_window)
        atr_temp = self.am.atr(self.atr_window)

        # Handle ATR initial value: don't skip, set default value
        if math.isnan(atr_temp) or atr_temp <= 0:
            # If ATR invalid, try using historical ATR value or estimate
            if self.atr_value <= 0:
                # If historical ATR also invalid, estimate based on price percentage
                atr_temp = bar_close * 0.005  # Use 0.5% of price as estimate
                self.write_log(f"ATR invalid, using estimate: {atr_temp:.2f}")
            else:
                atr_temp = self.atr_value  # Use historical ATR value
                self.write_log(f"ATR invalid, using historical value: {atr_temp:.2f}")
        self.atr_value = atr_temp

        # [Log 2] ATR and channel state
        self.log_tag(
            "VOL",
            f"atr={self.atr_value:.2f} exit_up={self.exit_up:.2f} exit_down={self.exit_down:.2f}",
        )

        # ========== Advanced risk control checks (highest priority) ==========
        # 1. Maximum drawdown control
        current_equity = self.current_capital + self.pos * bar_close
        if current_equity > self.max_equity:
            self.max_equity = current_equity

        self.current_drawdown = (
            (self.max_equity - current_equity) / self.max_equity * 100
        )

        if self.current_drawdown >= self.max_drawdown_pct:
            self.cancel_all()
            # [Log 3] Max drawdown stop details
            avg_cost = (
                self.total_cost / self.total_volume if self.total_volume > 0 else 0
            )
            unrealized_pnl = (
                (bar_close - avg_cost) * self.pos if self.total_volume > 0 else 0
            )
            self.log_tag(
                "FORCE_EXIT",
                f"max_drawdown hit: dd={self.current_drawdown:.2f}% limit={self.max_drawdown_pct}% equity={current_equity:.2f}/{self.max_equity:.2f} "
                f"pos={self.pos} price={bar_close:.2f} avg_cost={avg_cost:.2f} unrealized={unrealized_pnl:.2f}",
            )

            if self.pos > 0:
                # Simulate market sell order
                orders = self.sell(self.vt_symbol, 0, abs(self.pos))
            self.log_tag("FORCE_EXIT", "max_drawdown exit orders sent")
            self.put_event()
            return

        # Determine if cancellation needed
        should_cancel = False
        current_long_stop = 0.0

        if self.pos > 0 and self.long_stop > 0:
            current_long_stop = self.long_stop

        if cross_above:
            should_cancel = True
        elif self.pos > 0 and current_long_stop != self.last_long_stop:
            should_cancel = True

        if should_cancel:
            self.cancel_all()

        # ========== Basic risk control checks ==========
        # Update trailing stop tracking price
        if self.pos > 0:
            if (
                self.highest_price_since_entry == 0
                or bar_high > self.highest_price_since_entry
            ):
                self.highest_price_since_entry = bar_high

        # Check if forced liquidation needed
        force_exit = False
        force_exit_reason = ""

        if self.pos > 0:
            # 1. Max loss stop
            if self.max_loss_pct > 0 and self.total_cost > 0 and self.total_volume > 0:
                avg_cost = self.total_cost / self.total_volume
                # Calculate loss percentage from average cost
                loss_pct = (avg_cost - bar_close) / avg_cost * 100
                if loss_pct >= self.max_loss_pct:
                    force_exit = True
                    force_exit_reason = f"Max loss stop: loss {loss_pct:.2f}%"

            # 2. Trailing stop
            if self.use_trailing_stop and self.highest_price_since_entry > 0:
                trailing_stop_price = self.highest_price_since_entry * (
                    1 - self.trailing_pct / 100
                )
                if bar_low <= trailing_stop_price:
                    force_exit = True
                    force_exit_reason = f"Trailing stop: drawdown over {self.trailing_pct}% from high {self.highest_price_since_entry:.2f}"

            # Forced liquidation
            if force_exit:
                self.cancel_all()
                avg_cost = (
                    self.total_cost / self.total_volume
                    if self.total_volume > 0
                    else bar_close
                )
                pnl = (bar_close - avg_cost) * self.pos
                pnl_pct = (bar_close - avg_cost) / avg_cost * 100 if avg_cost else 0
                self.log_tag(
                    "FORCE_EXIT",
                    f"long reason={force_exit_reason} pos={self.pos} px={bar_close:.2f} avg={avg_cost:.2f} pnl={pnl:.2f} ({pnl_pct:.2f}%) "
                    f"trail_high={self.highest_price_since_entry:.2f}",
                )
                # Simulate market sell order
                orders = self.sell(self.vt_symbol, 0, abs(self.pos))
                self.put_event()
                return

        # ========== Normal trading logic ==========
        # Clean up filled orders
        self.clean_finished_orders()

        if not self.pos:
            self.long_entry = 0
            self.long_stop = 0
            self.last_long_stop = 0
            # Reset stop calculation variables
            self.first_entry_price = 0
            self.total_cost = 0
            self.total_volume = 0
            self.lowest_entry_price = 0
            # Reset trailing stop tracking variables
            self.highest_price_since_entry = 0
            # Reset pyramid variables
            self.current_unit = 0
            self.last_cross_bar = 0
            # Reset order tracking
            self.current_orders = {}

            # Golden cross open long
            if cross_above:
                self.last_cross_bar = bar_datetime
                self.send_buy_orders(bar_close, bar)

        elif self.pos > 0:
            # When holding long, golden cross pyramid (simplified pyramid logic)
            if cross_above and self.current_unit < self.max_units:
                self.last_cross_bar = bar_datetime
                self.send_buy_orders(bar_close, bar)

            # Place stop-loss order
            if self.long_stop > 0:
                sell_price = self.long_stop
                # Apply slippage
                sell_price_with_slippage = sell_price * (1 - self.slippage)

                # Check if need to update stop-loss order
                has_active_stop_loss = self.has_active_stop_loss_order("sell")
                stop_price_changed = (
                    abs(sell_price_with_slippage - self.last_long_stop) > 0.001
                )

                if stop_price_changed or not has_active_stop_loss:
                    # Mode: always place stop-loss
                    orders = self.sell(
                        self.vt_symbol, sell_price_with_slippage, abs(self.pos)
                    )
                    if orders:
                        for order in orders:
                            self.current_orders[order] = {
                                "direction": "sell",
                                "type": "stop_loss",
                                "price": sell_price_with_slippage,
                                "status": "not_traded",
                            }
                    self.last_long_stop = sell_price_with_slippage
                    self.log_tag(
                        "STOP_SET",
                        f"long stop price={sell_price_with_slippage:.4f} pos={self.pos}",
                    )

        self.put_event()

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        """
        # Get direction safely
        try:
            trade_direction = (
                trade.direction
                if hasattr(trade, "direction")
                else trade.get("direction", None)
            )
            trade_price = (
                trade.price if hasattr(trade, "price") else trade.get("price", 0)
            )
            trade_volume = (
                trade.volume if hasattr(trade, "volume") else trade.get("volume", 0)
            )
        except Exception:
            return

        # [Log 4] Trade details
        self.log_tag(
            "TRADE",
            f"dir={trade_direction} price={trade_price:.6f} volume={trade_volume} value={trade_price * trade_volume:.2f}",
        )

        # Update capital (spot trading logic)
        trade_value = trade_price * trade_volume

        # For spot: LONG direction means buy, SHORT direction means sell
        # vnpy uses offset.value for open/close, but in vnrs spot we simplify
        if trade_direction == Direction.LONG:
            # Buy: pay capital
            self.current_capital -= trade_value
            self.log_tag(
                "CAPITAL",
                f"Buy: spent {trade_value:.2f}, remaining capital {self.current_capital:.2f}",
            )
        elif trade_direction != Direction.LONG:
            # Sell: receive capital
            self.current_capital += trade_value
            self.log_tag(
                "CAPITAL",
                f"Sell: received {trade_value:.2f}, current capital {self.current_capital:.2f}",
            )

        # Update daily PnL
        if hasattr(trade, "pnl") and trade.pnl is not None:
            self.daily_pnl += trade.pnl

        if trade_direction == Direction.LONG:
            # Update cost tracking variables
            if self.first_entry_price == 0:
                self.first_entry_price = trade_price

            self.total_cost += trade_price * trade_volume
            self.total_volume += trade_volume

            if self.lowest_entry_price == 0 or trade_price < self.lowest_entry_price:
                self.lowest_entry_price = trade_price

            # Update current position unit count
            self.current_unit = (
                int(self.pos / self.fixed_size) if self.fixed_size > 0 else 0
            )

            # Calculate stop-loss price based on stop calculation mode
            base_price = (
                self.total_cost / self.total_volume
                if self.total_volume > 0
                else trade_price
            )
            self.long_entry = base_price
            # Only calculate stop-loss when ATR is valid
            if self.atr_value > 0:
                self.long_stop = self.long_entry - self.atr_multiplier * self.atr_value
            else:
                # When ATR invalid, use fixed percentage stop-loss (5%)
                self.long_stop = self.long_entry * 0.95

            # Initialize trailing stop tracking price
            if self.highest_price_since_entry == 0:
                self.highest_price_since_entry = trade_price

            # [Log 5] Long position update
            self.log_tag(
                "POSITION",
                f"long units={self.current_unit} pos={self.pos} entry_avg={base_price:.2f} entry_low={self.lowest_entry_price:.2f} stop={self.long_stop:.2f} cost={self.total_cost:.2f} qty={self.total_volume}",
            )

    def on_order(self, order) -> None:
        """
        Callback of new order data update.
        """
        # Handle both object and dict access patterns
        try:
            status = (
                order.status.value
                if hasattr(order.status, "value")
                else str(order.status)
                if hasattr(order, "status")
                else order.get("status", "")
            )
            orderid = (
                order.orderid if hasattr(order, "orderid") else order.get("orderid", "")
            )
        except Exception:
            return

        # Order status tracking
        if orderid in self.current_orders:
            order_info = self.current_orders[orderid]

            if status == "cancelled":
                # Remove cancelled orders from current orders list
                if orderid in self.current_orders:
                    del self.current_orders[orderid]

            elif status == "all_traded":
                # Order fully filled
                order_info["status"] = "all_traded"
                # Immediately remove fully filled orders from tracking list
                if orderid in self.current_orders:
                    del self.current_orders[orderid]

    def clean_finished_orders(self) -> None:
        """
        Clean up filled orders to avoid order pile-up
        """
        # Check and remove orders with abnormal status
        orders_to_remove = []
        for order_id, order_info in self.current_orders.items():
            if (
                isinstance(order_info, dict)
                and order_info.get("status") == "all_traded"
            ):
                orders_to_remove.append(order_id)

        # Remove filled orders
        for order_id in orders_to_remove:
            if order_id in self.current_orders:
                del self.current_orders[order_id]

    def has_active_stop_loss_order(self, direction: str) -> bool:
        """
        Check if there's a valid stop-loss order for the specified direction
        :param direction: 'sell' (long stop-loss)
        :return: True means there's a valid stop-loss order
        """
        for order_id, order_info in self.current_orders.items():
            if isinstance(order_info, dict):
                if (
                    order_info.get("direction") == direction
                    and order_info.get("type") == "stop_loss"
                    and order_info.get("status") in ["not_traded", "part_traded"]
                ):
                    return True
        return False

    def send_buy_orders(self, price: float, bar) -> None:
        """Send buy open/pyramid orders"""
        # Prevent division by zero
        if self.fixed_size <= 0:
            self.write_log("Error: fixed_size must be greater than 0")
            return

        # Calculate current position unit count
        current_unit = self.current_unit

        # Check if exceeded max position limit
        if current_unit >= self.max_units:
            self.write_log(
                f"Reached max position limit: {current_unit}/{self.max_units}"
            )
            return

        bar_close = bar["close_price"]

        # Dynamic position management: calculate position size based on current capital
        position_size = self.calculate_position_size(bar_close, self.atr_value)

        # [Log 6] Open long decision
        self.log_tag(
            "SIZE",
            f"long intent units={current_unit}/{self.max_units} calc={position_size:.6f} fixed={self.fixed_size} risk_pct={self.risk_per_unit * 100:.2f}% capital={self.current_capital:.2f}",
        )

        # Apply slippage
        price_with_slippage = price * (1 + self.slippage)
        self.log_tag(
            "SLIPPAGE",
            f"long px={price:.6f} -> {price_with_slippage:.6f} slippage={self.slippage}",
        )

        # Spot anti-overbuy check: calculate affordable quantity at slippage-adjusted price
        # Check if minimum notional requirement is met
        min_qty = (
            self.min_notional / price_with_slippage if price_with_slippage > 0 else 0
        )
        if min_qty > 0 and position_size < min_qty:
            self.log_tag(
                "AFFORD",
                f"adjust to min_qty: want={position_size:.6f} -> {min_qty:.6f} px={price_with_slippage:.6f} min_notional={self.min_notional}",
            )
            position_size = min_qty

        affordable_size = (
            self.current_capital / price_with_slippage if price_with_slippage > 0 else 0
        )
        if affordable_size <= 0:
            self.log_tag(
                "AFFORD",
                f"reject long want={position_size:.6f} need={price_with_slippage * position_size:.2f} capital={self.current_capital:.2f}",
            )
            return

        if position_size > affordable_size:
            self.log_tag(
                "AFFORD",
                f"adjust long want={position_size:.6f} -> {affordable_size:.6f} capital={self.current_capital:.2f} need={price_with_slippage * affordable_size:.2f}",
            )
            position_size = min(affordable_size, position_size)
        else:
            self.log_tag(
                "AFFORD",
                f"ok long want={position_size:.6f} affordable={affordable_size:.6f} capital={self.current_capital:.2f} need={price_with_slippage * position_size:.2f}",
            )

        # Ensure order size is not less than minimum order size
        if position_size < self.fixed_size:
            position_size = self.fixed_size

        # Open or pyramid
        if current_unit == 0:
            # Open position
            orders = self.buy(self.vt_symbol, price_with_slippage, position_size)
            if orders:
                for order in orders:
                    self.current_orders[order] = {
                        "direction": "buy",
                        "type": "entry",
                        "level": 0,
                        "price": price_with_slippage,
                        "status": "not_traded",
                    }
                self.log_tag(
                    "ORDER",
                    f"long entry limit_px={price_with_slippage:.6f} size={position_size:.6f} vt_ids={orders}",
                )
            else:
                self.log_tag(
                    "ORDER",
                    f"long entry limit_px={price_with_slippage:.6f} size={position_size:.6f} rejected",
                )
        else:
            # Pyramid: only add when trend confirmed
            if (
                self.atr_value > 0
                and price > self.ma2_value
                and self.ma1_value > self.ma2_value
                and self.ma1_value > self.ma1_last
            ):  # Ensure short-term EMA is rising
                order_price = price_with_slippage
                orders = self.buy(self.vt_symbol, order_price, position_size)
                if orders:
                    for order in orders:
                        self.current_orders[order] = {
                            "direction": "buy",
                            "type": "pyramid",
                            "level": current_unit,
                            "price": order_price,
                            "status": "not_traded",
                        }
                    self.log_tag(
                        "ORDER",
                        f"long pyramid limit_px={order_price:.6f} size={position_size:.6f} level={current_unit} vt_ids={orders}",
                    )
                else:
                    self.log_tag(
                        "ORDER",
                        f"long pyramid limit_px={order_price:.6f} size={position_size:.6f} level={current_unit} rejected",
                    )
            else:
                self.log_tag(
                    "ORDER",
                    f"skip long pyramid: atr={self.atr_value:.6f} price={price:.6f} ma2={self.ma2_value:.6f} ma1={self.ma1_value:.6f} trend_ok={self.ma1_value > self.ma2_value and self.ma1_value > self.ma1_last}",
                )

    def calculate_position_size(self, entry_price: float, atr: float) -> float:
        """
        Calculate position size based on current capital and risk percentage
        """
        if atr <= 0:
            self.write_log(f"ATR invalid({atr}), using fixed size: {self.fixed_size}")
            return self.fixed_size  # If ATR invalid, use fixed size

        # Use current capital for calculation
        account_balance = self.current_capital

        # Calculate risk amount based on account capital
        risk_amount = account_balance * self.risk_per_unit

        # Calculate risk per unit based on ATR
        risk_per_share = atr

        # Calculate position size - use risk amount divided by (price * ATR multiplier) to determine Bitcoin quantity
        # This formula ensures risk is inversely proportional to price, higher price means lower quantity
        if risk_per_share > 0 and entry_price > 0:
            position_size = risk_amount / (risk_per_share * entry_price)
        else:
            # If ATR or price invalid, use fixed size
            position_size = self.fixed_size

        # [Log 7] Position calculation details
        self.log_tag(
            "SIZE_CALC",
            f"account={account_balance:.2f} risk_amt={risk_amount:.2f} atr={risk_per_share:.4f} px={entry_price:.2f} calc_size={position_size:.6f}",
        )

        # Ensure position size is not less than minimum order size, and doesn't exceed available capital
        affordable_size = account_balance / entry_price if entry_price > 0 else 0
        position_size = min(position_size, affordable_size)

        if position_size < self.fixed_size:
            # Check if enough capital to buy one minimum unit
            min_cost = self.fixed_size * entry_price
            if account_balance >= min_cost:
                position_size = self.fixed_size
                self.log_tag("SIZE_CALC", f"adjust to fixed_size: {position_size:.6f}")
            else:
                self.log_tag(
                    "SIZE_CALC",
                    f"insufficient funds: need {min_cost:.2f}, have {account_balance:.2f}",
                )
                position_size = 0  # Insufficient funds, no trade

        # Ensure minimum notional requirement is met
        if position_size > 0 and position_size * entry_price < self.min_notional:
            required_size = self.min_notional / entry_price
            position_size = max(position_size, required_size)
            self.log_tag("SIZE_CALC", f"adjust to min_notional: {position_size:.6f}")

        return position_size
