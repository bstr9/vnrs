"""
Turtle 15min EMA Signal Strategy (Comprehensive)

Migrated from vnpy_ctastrategy. Dual-EMA crossover turtle strategy
with comprehensive risk management for spot trading (long-only).

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- Use cta_utils.BarGenerator / ArrayManager instead of vnpy's
- bar.high_price -> bar["high_price"], bar.low_price -> bar["low_price"], etc.
- bar.datetime -> bar.get("datetime", "")
- stop=True/False parameter removed from buy/sell calls
- self.buy(price, vol, stop) -> self.buy(self.vt_symbol, price, vol)
- self.sell(price, vol, stop) -> self.sell(self.vt_symbol, price, vol)
- talib.EMA replaced with self.am.ema(window, array=True) for seed,
  then incremental calculation
- on_order uses safe attribute access for status/orderid
- get_capital/get_slippage/get_pricetick use engine.call_method fallback stubs
"""

import datetime
import math
from pathlib import Path

from trade_engine import CtaStrategy
from cta_utils import BarGenerator, ArrayManager


class Turtle15MinEmaSignalStrategy(CtaStrategy):
    """
    Dual-EMA crossover improved turtle strategy (spot version - long only).
    Entry signal: MA1 (short EMA) crosses above MA2 (long EMA).
    - Long entry: MA1 crosses above MA2
    - Long exit: stop loss trigger / trailing stop / max loss

    Core features:
    1. Spot trading only, long-only
    2. Simplified capital management: buy deducts, sell receives
    3. Enhanced risk control with multiple stop loss mechanisms
    4. Optimized performance using incremental EMA calculation
    5. Comprehensive order management handling complex scenarios
    """

    author = "用Python的交易员"

    # 参数定义与边界检查
    ma1_window: int = 7  # 短期EMA周期
    ma2_window: int = 25  # 长期EMA周期
    exit_window: int = 10
    atr_window: int = 20
    fixed_size: int = 1
    min_volume: float = 0.0001  # 最小下单量（现货可设为0.0001）
    stop_calc_mode: int = (
        2  # 止损计算模式：1-首次入场 2-平均成本 3-最后入场 4-最保守入场
    )
    use_channel_exit: bool = True  # 是否使用通道出场（False时只用ATR止损）
    atr_multiplier: float = 2.0  # ATR止损倍数，建议1.5-3.0
    max_loss_pct: float = 10.0  # 最大亏损百分比，超过则强制市价清仓（0表示不启用）
    use_signal_exit: bool = False  # 是否使用反向信号出场（默认关闭）
    use_trailing_stop: bool = True  # 是否使用移动止损
    trailing_pct: float = 2.0  # 移动止损回撤百分比
    max_units: int = 4  # 最大加仓单位数
    risk_per_unit: float = 0.02  # 每个加仓单位的风险百分比
    max_daily_loss_pct: float = 5.0  # 最大日亏损百分比
    max_drawdown_pct: float = 20.0  # 最大回撤百分比

    ma1_value: float = 0  # 短期EMA当前值
    ma2_value: float = 0  # 长期EMA当前值
    ma1_last: float = 0  # 短期EMA上一根K线值
    ma2_last: float = 0  # 长期EMA上一根K线值
    exit_down: float = 0  # 出场通道下轨
    atr_value: float = 0
    long_entry: float = 0  # 多头入场价
    long_stop: float = 0  # 多头止损价

    # 上次挂单的止损价（用于判断是否需要撤单更新）
    last_long_stop: float = 0

    # 止损计算相关变量
    first_entry_price: float = 0  # 首次入场价
    total_cost: float = 0  # 总成本（价格 * 数量）
    total_volume: float = 0  # 总成交量
    lowest_entry_price: float = 0  # 最低入场价

    # 移动止损相关变量
    highest_price_since_entry: float = 0  # 入场后最高价

    # EMA增量计算参数
    ma1_alpha: float = 0  # MA1的平滑系数
    ma2_alpha: float = 0  # MA2的平滑系数
    ema_inited: bool = False  # EMA是否已初始化

    # 加仓条件增强
    last_cross_bar: int = 0  # 上次交叉信号的K线索引
    current_unit: int = 0  # 当前仓位单位数

    # 订单状态跟踪
    current_orders: dict = {}  # 当前订单字典，键为订单ID，值为订单信息

    # 资金管理相关
    current_capital: float = 0  # 当前可用资金（USDT）
    current_holdings: float = 0  # 当前持仓价值（按成本计）
    daily_pnl: float = 0  # 当日盈亏
    max_equity: float = 0  # 最高权益
    current_drawdown: float = 0  # 当前回撤
    slippage: float = 0  # 滑点（从引擎获取）
    log_file_path: str = ""  # 日志文件路径

    # 交易时间控制
    trade_start_time: datetime.time = datetime.time(9, 0)  # 交易开始时间
    trade_end_time: datetime.time = datetime.time(15, 0)  # 交易结束时间

    parameters = [
        "ma1_window",
        "ma2_window",
        "exit_window",
        "atr_window",
        "fixed_size",
        "min_volume",
        "stop_calc_mode",
        "use_channel_exit",
        "atr_multiplier",
        "max_loss_pct",
        "use_signal_exit",
        "use_trailing_stop",
        "trailing_pct",
        "max_units",
        "risk_per_unit",
        "max_daily_loss_pct",
        "max_drawdown_pct",
    ]
    variables = [
        "ma1_value",
        "ma2_value",
        "exit_down",
        "atr_value",
        "current_unit",
        "current_capital",
        "current_holdings",
        "current_drawdown",
    ]

    def get_capital(self):
        """Get initial capital (vnrs compat stub)."""
        try:
            if self.engine:
                result = self.engine.call_method("get_capital")
                if result > 0:
                    return result
        except Exception:
            pass
        return 0

    def get_slippage(self):
        """Get slippage (vnrs compat stub)."""
        try:
            if self.engine:
                return self.engine.call_method("get_slippage")
        except Exception:
            pass
        return 0

    def get_pricetick(self):
        """Get price tick (vnrs compat stub)."""
        try:
            if self.engine:
                return self.engine.call_method("get_pricetick")
        except Exception:
            pass
        return 0

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self._init_log_file()
        self.write_log("策略初始化")

        self.bg = BarGenerator(self.on_bar)
        self.am = ArrayManager()

        # 计算EMA平滑系数: α = 2 / (N + 1)
        self.ma1_alpha = 2.0 / (self.ma1_window + 1)
        self.ma2_alpha = 2.0 / (self.ma2_window + 1)

        # 边界检查
        self.validate_parameters()

        # 关键参数检查
        if self.fixed_size <= 0:
            self.write_log("严重错误: fixed_size必须大于0，策略无法运行")
            raise ValueError("fixed_size必须大于0")

        # 从引擎获取初始资金和滑点
        initial_capital = self.get_capital()
        slippage = self.get_slippage()

        # 初始化资金与滑点（现货：资金单位为USDT）
        self.current_capital = initial_capital if initial_capital > 0 else 100000.0
        self.current_holdings = 0  # 初始无持仓
        self.max_equity = self.current_capital
        self.slippage = slippage

        self.load_bar(max(self.ma1_window, self.ma2_window) + 10)

    def on_start(self) -> None:
        """
        Callback when strategy is started.
        """
        self.write_log("策略启动")
        try:
            self.write_log(f"pricetick(runtime)={self.get_pricetick()}")
        except Exception:
            pass

    def on_stop(self) -> None:
        """
        Callback when strategy is stopped.
        """
        self.write_log("策略停止")

    def _init_log_file(self) -> None:
        """初始化策略埋点日志文件"""
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
            # 初始化失败时忽略，仍保留常规日志通道
            pass

    def _emit_log(self, msg: str) -> None:
        """底层日志写入，供普通日志和结构化日志共享"""
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
        """覆盖写日志：同时写入文件与常规日志通道"""
        self._emit_log(msg)

    def log_tag(self, tag: str, msg: str) -> None:
        """结构化埋点日志，方便后处理/LLM分析"""
        self._emit_log(f"[{tag}] {msg}")

    def validate_parameters(self):
        """验证参数边界"""
        if self.atr_multiplier < 1.5 or self.atr_multiplier > 3.0:
            self.atr_multiplier = 2.0
            self.write_log("警告: atr_multiplier 应在1.5-3.0范围内，已重置为2.0")

        if self.max_units <= 0 or self.max_units > 6:
            self.max_units = 4
            self.write_log("警告: max_units 超出合理范围(1-6)，已重置为4")

        if self.risk_per_unit <= 0 or self.risk_per_unit > 0.1:
            self.risk_per_unit = 0.02
            self.write_log("警告: risk_per_unit 应在(0-0.1)范围内，已重置为0.02")

        if self.min_volume <= 0:
            self.min_volume = 0.0001
            self.write_log("警告: min_volume 必须大于0，已重置为0.0001")

        if self.max_loss_pct > 20:
            self.max_loss_pct = 10.0
            self.write_log("警告: max_loss_pct 过高，已重置为10.0")

    def is_trading_time(self, bar_time: datetime.datetime) -> bool:
        """检查是否为交易时间"""
        time_only = bar_time.time()
        return self.trade_start_time <= time_only <= self.trade_end_time

    def on_tick(self, tick) -> None:
        """Callback of new tick data update."""
        self.bg.update_tick(tick)

    def on_bar(self, bar) -> None:
        """Callback of new bar data update."""
        # Check trading time
        bar_dt = bar.get("datetime", "")
        if bar_dt:
            try:
                if isinstance(bar_dt, str):
                    bar_dt = datetime.datetime.fromisoformat(bar_dt)
                if not self.is_trading_time(bar_dt):
                    return
            except Exception:
                pass

        self.am.update_bar(bar)
        if not self.am.inited:
            return

        close_price = bar["close_price"]
        high_price = bar["high_price"]
        low_price = bar["low_price"]
        open_price = bar["open_price"]

        # [Telemetry 1] Record basic state at bar start
        self.log_tag(
            "STATE",
            f"bar={bar_dt} O={open_price:.2f} H={high_price:.2f} L={low_price:.2f} C={close_price:.2f} "
            f"pos={self.pos} units={self.current_unit} capital={self.current_capital:.2f} "
            f"equity={self.current_capital + self.pos * close_price:.2f} dd={self.current_drawdown:.2f}% max_eq={self.max_equity:.2f}",
        )
        try:
            active_limits = len(getattr(self.cta_engine, "active_limit_orders", {}))
            self.log_tag("STATE", f"active_limit_orders={active_limits}")
        except Exception:
            pass

        # First initialization: use ArrayManager EMA calculation
        if not self.ema_inited:
            ma1_array = self.am.ema(self.ma1_window, array=True)
            ma2_array = self.am.ema(self.ma2_window, array=True)

            # 检查数组长度和有效性
            if len(ma1_array) < 2 or len(ma2_array) < 2:
                return  # 数据不足，跳过本次

            self.ma1_last = ma1_array[-2] if not math.isnan(ma1_array[-2]) else 0
            self.ma2_last = ma2_array[-2] if not math.isnan(ma2_array[-2]) else 0
            self.ma1_value = ma1_array[-1] if not math.isnan(ma1_array[-1]) else 0
            self.ma2_value = ma2_array[-1] if not math.isnan(ma2_array[-1]) else 0

            # Check for NaN (insufficient data returns NaN)
            if (
                math.isnan(self.ma1_value)
                or math.isnan(self.ma2_value)
                or self.ma1_value == 0
                or self.ma2_value == 0
            ):
                return  # 数据不足，跳过本次

            self.ema_inited = True
        else:
            # Incremental update: EMA_new = alpha * Price + (1-alpha) * EMA_old
            # Significantly improves backtest speed (avoids recalculating entire array)
            self.ma1_last = self.ma1_value
            self.ma2_last = self.ma2_value
            self.ma1_value = (
                self.ma1_alpha * close_price + (1 - self.ma1_alpha) * self.ma1_last
            )
            self.ma2_value = (
                self.ma2_alpha * close_price + (1 - self.ma2_alpha) * self.ma2_last
            )

        # 判断EMA交叉信号
        # 金叉：MA1从下方穿越MA2
        cross_above = (self.ma1_last <= self.ma2_last) and (
            self.ma1_value > self.ma2_value
        )
        # 死叉：MA1从上方穿越MA2
        cross_below = (self.ma1_last >= self.ma2_last) and (
            self.ma1_value < self.ma2_value
        )

        # 【埋点2】EMA状态
        cross_flag = "golden" if cross_above else "death" if cross_below else "none"
        self.log_tag(
            "SIGNAL",
            f"ma1={self.ma1_value:.2f}/{self.ma1_last:.2f} ma2={self.ma2_value:.2f}/{self.ma2_last:.2f} cross={cross_flag}",
        )

        # 计算出场通道和 ATR（每次都更新，避免使用过期值）
        exit_up, self.exit_down = self.am.donchian(self.exit_window)  # 只使用下轨
        atr_temp = self.am.atr(self.atr_window)

        # 修复ATR初始值处理：不再直接跳过，而是设置默认值
        if math.isnan(atr_temp) or atr_temp <= 0:
            # 如果ATR无效，尝试使用历史ATR值或估算
            if self.atr_value <= 0:
                # 如果历史ATR也无效，可以基于当前价格的一定比例估算
                # 这样不会错过交易机会
                atr_temp = close_price * 0.005  # Use 0.5% of price as estimate
                self.write_log(f"ATR无效，使用估算值: {atr_temp:.2f}")
            else:
                atr_temp = self.atr_value  # 使用历史ATR值
                self.write_log(f"ATR无效，使用历史值: {atr_temp:.2f}")
        self.atr_value = atr_temp

        # 【埋点3】ATR和通道状态
        self.log_tag("VOL", f"atr={self.atr_value:.2f} exit_down={self.exit_down:.2f}")

        # ========== 高级风控检查（最高优先级） ==========
        # 1. 最大回撤控制
        current_equity = self.current_capital + self.pos * close_price
        if current_equity > self.max_equity:
            self.max_equity = current_equity

        self.current_drawdown = (
            (self.max_equity - current_equity) / self.max_equity * 100
        )

        # 当已经空仓且触发过最大回撤时，允许重置基准以恢复交易能力
        if self.pos == 0 and self.current_drawdown >= self.max_drawdown_pct:
            self.log_tag(
                "FORCE_EXIT",
                f"dd reset after flat: dd={self.current_drawdown:.2f}% -> 0, base_eq {current_equity:.2f}",
            )
            self.max_equity = current_equity
            self.current_drawdown = 0

        if self.current_drawdown >= self.max_drawdown_pct:
            self.cancel_all()
            # 【埋点4】最大回撤止损详细信息
            avg_cost = (
                self.total_cost / self.total_volume if self.total_volume > 0 else 0
            )
            unrealized_pnl = (
                (close_price - avg_cost) * self.pos if self.total_volume > 0 else 0
            )
            self.log_tag(
                "FORCE_EXIT",
                f"max_drawdown hit: dd={self.current_drawdown:.2f}% limit={self.max_drawdown_pct}% equity={current_equity:.2f}/{self.max_equity:.2f} "
                f"pos={self.pos} price={close_price:.2f} avg_cost={avg_cost:.2f} unrealized={unrealized_pnl:.2f}",
            )

            if self.pos > 0:
                aggressive_sell_price = close_price * 0.95
                orders = self.sell(self.vt_symbol, aggressive_sell_price, abs(self.pos))
                if orders:
                    for order in orders:
                        self.current_orders[order] = {
                            "direction": "sell",
                            "type": "max_drawdown_exit",
                            "price": aggressive_sell_price,
                            "status": "not_traded",
                        }
                self.log_tag(
                    "FORCE_EXIT",
                    f"max_drawdown exit orders sent at price={aggressive_sell_price:.2f}",
                )
            self.put_event()
            return

        # 判断是否需要撤单
        # 条件1：出现交叉信号（需要挂新的开仓/加仓单）
        # 条件2：止损价发生变化（需要更新止损单）
        should_cancel = False

        # 计算当前应该使用的止损价
        current_long_stop = 0.0

        if self.pos > 0 and self.long_stop > 0:
            if self.use_channel_exit:
                current_long_stop = max(self.long_stop, self.exit_down)
            else:
                current_long_stop = self.long_stop

        if cross_above:
            should_cancel = True
        elif self.pos > 0 and current_long_stop != self.last_long_stop:
            should_cancel = True

        if should_cancel:
            self.cancel_all()

        # ========== 基础风控检查 ==========
        # 更新移动止损追踪价格（仅多头）
        if self.pos > 0:
            if (
                self.highest_price_since_entry == 0
                or high_price > self.highest_price_since_entry
            ):
                self.highest_price_since_entry = high_price

        # 检查是否需要强制清仓（三种情况）
        force_exit = False
        force_exit_reason = ""

        if self.pos > 0:
            # 1. 最大亏损止损
            if self.max_loss_pct > 0 and self.total_cost > 0 and self.total_volume > 0:
                avg_cost = self.total_cost / self.total_volume
                # 计算从平均成本开始的亏损百分比
                loss_pct = (avg_cost - close_price) / avg_cost * 100
                if loss_pct >= self.max_loss_pct:
                    force_exit = True
                    force_exit_reason = f"最大亏损止损: 亏损{loss_pct:.2f}%"

            # 2. 反向信号出场（死叉）
            if self.use_signal_exit and cross_below:
                force_exit = True
                force_exit_reason = "反向信号出场: 出现死叉"

            # 3. 移动止损
            if self.use_trailing_stop and self.highest_price_since_entry > 0:
                trailing_stop_price = self.highest_price_since_entry * (
                    1 - self.trailing_pct / 100
                )
                if low_price <= trailing_stop_price:
                    force_exit = True
                    force_exit_reason = f"移动止损: 从最高点{self.highest_price_since_entry:.2f}回撤超过{self.trailing_pct}%"

            # 强制清仓
            if force_exit:
                self.cancel_all()
                avg_cost = (
                    self.total_cost / self.total_volume
                    if self.total_volume > 0
                    else close_price
                )
                pnl = (close_price - avg_cost) * self.pos
                pnl_pct = (close_price - avg_cost) / avg_cost * 100 if avg_cost else 0
                self.log_tag(
                    "FORCE_EXIT",
                    f"long reason={force_exit_reason} pos={self.pos} px={close_price:.2f} avg={avg_cost:.2f} pnl={pnl:.2f} ({pnl_pct:.2f}%) "
                    f"trail_high={self.highest_price_since_entry:.2f}",
                )
                aggressive_sell_price = close_price * 0.95
                orders = self.sell(
                    self.vt_symbol, aggressive_sell_price, abs(self.pos)
                )  # 限价单，不是停止单
                if orders:
                    for order in orders:
                        self.current_orders[order] = {
                            "direction": "sell",
                            "type": "force_exit",
                            "price": aggressive_sell_price,
                            "status": "not_traded",
                        }
                    self.log_tag(
                        "FORCE_EXIT",
                        f"orders sent: {orders} at price={aggressive_sell_price:.2f}",
                    )
                else:
                    self.log_tag("FORCE_EXIT", "WARNING: sell order rejected!")
                self.put_event()
                return

        # ========== 正常交易逻辑 ==========
        # 清理已成交的订单
        self.clean_finished_orders()

        if not self.pos:
            self.long_entry = 0
            self.long_stop = 0
            self.last_long_stop = 0
            # 重置止损计算相关变量
            self.first_entry_price = 0
            self.total_cost = 0
            self.total_volume = 0
            self.lowest_entry_price = 0
            # 重置移动止损追踪变量
            self.highest_price_since_entry = 0
            # 重置加仓相关变量
            self.current_unit = 0
            self.last_cross_bar = 0
            # 重置订单跟踪
            self.current_orders = {}

            # 金叉开多（现货只能做多）
            if cross_above:
                self.last_cross_bar = bar_dt
                self.send_buy_orders(close_price, bar)

        elif self.pos > 0:
            # 持有多头时，金叉加仓（简化加仓逻辑）
            if cross_above and self.current_unit < self.max_units:
                self.last_cross_bar = bar_dt
                self.send_buy_orders(close_price, bar)

            # 根据止损模式处理止损单（仅在需要时挂单）
            if self.long_stop > 0:
                # 根据是否使用通道出场计算止损价
                if self.use_channel_exit:
                    sell_price = max(self.long_stop, self.exit_down)
                else:
                    sell_price = self.long_stop

                # 检查是否已经触发止损（当前价格低于止损价）
                if close_price <= sell_price:
                    # 已触发止损，立即以激进价格卖出
                    self.cancel_all()
                    aggressive_sell_price = close_price * 0.98  # 比当前价低 2%
                    orders = self.sell(
                        self.vt_symbol, aggressive_sell_price, abs(self.pos)
                    )  # 限价单
                    if orders:
                        for order in orders:
                            self.current_orders[order] = {
                                "direction": "sell",
                                "type": "stop_loss_triggered",
                                "price": aggressive_sell_price,
                                "status": "not_traded",
                            }
                    self.log_tag(
                        "STOP_TRIGGERED",
                        f"price={close_price:.4f} <= stop={sell_price:.4f}, sell at {aggressive_sell_price:.4f}",
                    )
                else:
                    # 未触发止损，检查是否需要更新止损单
                    # 卖出止损单应该使用稍低于止损价的价格，确保能成交
                    sell_price_with_buffer = sell_price * 0.995  # 比止损价低 0.5%

                    has_active_stop_loss = self.has_active_stop_loss_order("sell")
                    stop_price_changed = (
                        abs(sell_price_with_buffer - self.last_long_stop) > 0.001
                    )

                    if stop_price_changed or not has_active_stop_loss:
                        # 使用限价单而非停止单，直接挂在交易所
                        orders = self.sell(
                            self.vt_symbol, sell_price_with_buffer, abs(self.pos)
                        )  # 改为限价单
                        if orders:
                            for order in orders:
                                self.current_orders[order] = {
                                    "direction": "sell",
                                    "type": "stop_loss",
                                    "price": sell_price_with_buffer,
                                    "status": "not_traded",
                                }
                        self.last_long_stop = sell_price_with_buffer
                        self.log_tag(
                            "STOP_SET",
                            f"limit order at price={sell_price_with_buffer:.4f} pos={self.pos}",
                        )

        self.put_event()

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        Spot trading capital management: buy deducts, sell receives.
        """
        # Handle both dict and object trade formats
        if isinstance(trade, dict):
            trade_direction = trade.get("direction", "")
            trade_price = trade["price"]
            trade_volume = trade["volume"]
        else:
            trade_direction = trade.direction
            trade_price = trade.price
            trade_volume = trade.volume

        # Normalize direction for comparison (string-based, vnrs compat)
        if isinstance(trade_direction, str):
            is_long = trade_direction in ("long", "LONG", "Direction.LONG")
        else:
            # Handle enum-like objects by comparing value
            is_long = (
                getattr(trade_direction, "value", str(trade_direction))
                in ("long", "LONG")
                if hasattr(trade_direction, "value")
                else str(trade_direction) in ("long", "LONG", "Direction.LONG")
            )

        # [Telemetry 7] Trade execution details
        trade_value = trade_price * trade_volume
        dir_str = "long" if is_long else "short"
        self.log_tag(
            "TRADE",
            f"dir={dir_str} price={trade_price:.6f} volume={trade_volume} value={trade_value:.2f}",
        )

        # Spot capital management: simplified to buy/sell
        if is_long:
            # Buy: deduct capital
            self.current_capital -= trade_value
            self.current_holdings += trade_value  # Increase holdings cost
        else:
            # Sell: receive capital
            self.current_capital += trade_value
            # Reduce holdings cost (proportionally)
            if self.total_volume > 0:
                cost_ratio = trade_volume / self.total_volume
                self.current_holdings -= self.total_cost * cost_ratio
                self.total_cost -= self.total_cost * cost_ratio
                self.total_volume -= trade_volume

        # [Telemetry 8] Capital change
        self.log_tag(
            "CAPITAL",
            f"capital={self.current_capital:.2f} holdings={self.current_holdings:.2f} total_equity={self.current_capital + self.current_holdings:.2f}",
        )

        # Only process long buys (spot can only go long)
        if is_long:
            # Update cost tracking variables
            if self.first_entry_price == 0:
                self.first_entry_price = trade_price

            self.total_cost += trade_price * trade_volume
            self.total_volume += trade_volume

            if self.lowest_entry_price == 0 or trade_price < self.lowest_entry_price:
                self.lowest_entry_price = trade_price

            # Update current position unit count (use int not round, avoid rounding issues)
            self.current_unit = (
                int(self.pos / self.fixed_size) if self.fixed_size > 0 else 0
            )

            # Calculate stop loss price based on stop calc mode
            if self.stop_calc_mode == 1:
                # Mode 1: First entry price
                base_price = self.first_entry_price
            elif self.stop_calc_mode == 2:
                # Mode 2: Weighted average cost
                base_price = (
                    self.total_cost / self.total_volume
                    if self.total_volume > 0
                    else trade_price
                )
            elif self.stop_calc_mode == 3:
                # Mode 3: Last entry price
                base_price = trade_price
            elif self.stop_calc_mode == 4:
                # Mode 4: Most conservative entry price (long uses lowest)
                base_price = self.lowest_entry_price
            else:
                base_price = trade_price

            self.long_entry = base_price
            # Ensure ATR is valid before calculating stop price
            if self.atr_value > 0:
                self.long_stop = self.long_entry - self.atr_multiplier * self.atr_value
            else:
                # ATR invalid, use fixed percentage stop (5%)
                self.long_stop = self.long_entry * 0.95

            # Initialize trailing stop tracking price
            if self.highest_price_since_entry == 0:
                self.highest_price_since_entry = trade_price

            # [Telemetry 9] Long position update
            self.log_tag(
                "POSITION",
                f"long units={self.current_unit} pos={self.pos} entry_first={self.first_entry_price:.2f} entry_avg={base_price:.2f} entry_low={self.lowest_entry_price:.2f} stop={self.long_stop:.2f} cost={self.total_cost:.2f} qty={self.total_volume}",
            )
        # Spot sell: position info already updated above, no extra processing needed

    def on_order(self, order) -> None:
        """Callback of new order data update."""
        try:
            status = (
                order.status.value
                if hasattr(order.status, "value")
                else str(order.status)
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
                self.write_log(f"订单{orderid}已取消: {order_info}")
                if orderid in self.current_orders:
                    del self.current_orders[orderid]

            elif status == "not_traded":
                # Order not yet filled, update status
                order_info["status"] = "not_traded"
                order_info["traded"] = order.traded if hasattr(order, "traded") else 0
                order_info["remaining"] = (
                    order.volume if hasattr(order, "volume") else 0
                ) - order_info["traded"]

            elif status == "part_traded":
                # Order partially filled
                traded = order.traded if hasattr(order, "traded") else 0
                volume = order.volume if hasattr(order, "volume") else 0
                self.write_log(
                    f"订单{orderid}部分成交: 已成交{traded}, 剩余{volume - traded}"
                )
                order_info["status"] = "part_traded"
                order_info["traded"] = traded
                order_info["remaining"] = volume - traded

            elif status == "all_traded":
                # Order fully filled
                self.write_log(f"订单{orderid}全部成交: {order_info}")
                order_info["status"] = "all_traded"
                order_info["traded"] = order.traded if hasattr(order, "traded") else 0
                order_info["remaining"] = 0  # Remaining is 0

                # Immediately remove fully filled orders from tracking list
                if orderid in self.current_orders:
                    del self.current_orders[orderid]

    def clean_finished_orders(self) -> None:
        """Clean up filled orders to prevent order accumulation."""
        # Check and remove orders with abnormal status (theoretically shouldn't exist since on_order handles them)
        orders_to_remove = []

        for order_id, order_info in self.current_orders.items():
            if (
                isinstance(order_info, dict)
                and order_info.get("status") == "all_traded"
            ):
                self.write_log(
                    f"在clean_finished_orders中发现未清理的完全成交订单: {order_id}"
                )

                orders_to_remove.append(order_id)

        # 移除已成交的订单
        for order_id in orders_to_remove:
            if order_id in self.current_orders:
                del self.current_orders[order_id]

        # 如果移除的订单数量大于0，记录日志
        if orders_to_remove:
            self.write_log(f"清理了{len(orders_to_remove)}个已成交订单")

    def has_active_stop_loss_order(self, direction: str) -> bool:
        """
        检查是否存在指定方向的有效止损单
        :param direction: 'sell'（多头止损）或'buy'（空头止损）
        :return: True表示存在有效止损单
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
        """发送买入开仓/加仓单"""
        # 防止除零错误
        if self.fixed_size <= 0:
            self.write_log("错误：fixed_size必须大于0")
            return

        # 计算当前仓位单位数（不再重复计算，直接使用self.current_unit）
        current_unit = abs(self.current_unit)

        # 检查是否超过最大仓位限制
        if current_unit >= self.max_units:
            self.write_log(f"已达到最大仓位限制: {current_unit}/{self.max_units}")
            return

        # 动态仓位管理：根据当前资金计算仓位大小
        position_size = self.calculate_position_size(price, self.atr_value)

        # 【埋点11】开多仓决策
        self.log_tag(
            "SIZE",
            f"long intent units={current_unit}/{self.max_units} calc={position_size:.6f} fixed={self.fixed_size} risk_pct={self.risk_per_unit * 100:.2f}% capital={self.current_capital:.2f}",
        )

        # 应用滑点
        price_with_slippage = price * (1 + self.slippage)
        self.log_tag(
            "SLIPPAGE",
            f"long px={price:.6f} -> {price_with_slippage:.6f} slippage={self.slippage}",
        )

        # 现货防超买校验：按滑点后的价格计算可买数量
        affordable_size = (
            self.current_capital / price_with_slippage if price_with_slippage > 0 else 0
        )
        if affordable_size <= 0:
            self.log_tag(
                "AFFORD",
                f"reject long price={price_with_slippage:.6f} want={position_size:.6f} need={price_with_slippage * position_size:.2f} capital={self.current_capital:.2f}",
            )
            return

        if position_size > affordable_size:
            self.log_tag(
                "AFFORD",
                f"adjust long want={position_size:.6f} -> {affordable_size:.6f} capital={self.current_capital:.2f} price={price_with_slippage:.6f}",
            )
            position_size = affordable_size
        else:
            self.log_tag(
                "AFFORD",
                f"ok long want={position_size:.6f} affordable={affordable_size:.6f} capital={self.current_capital:.2f} price={price_with_slippage:.6f}",
            )

        # 开仓或加仓
        if current_unit == 0:
            # 开仓使用市价语义（BAR模式下一根K线按开盘成交）
            market_price = bar["high_price"] * 10
            orders = self.buy(self.vt_symbol, market_price, position_size)
            if orders:
                for order in orders:
                    self.current_orders[order] = {
                        "direction": "buy",
                        "type": "entry",
                        "level": 0,
                        "price": market_price,
                        "status": "not_traded",
                    }
                self.log_tag(
                    "ORDER",
                    f"long entry market_px={market_price:.6f} size={position_size:.6f} vt_ids={orders}",
                )
            else:
                self.log_tag(
                    "ORDER",
                    f"long entry market_px={market_price:.6f} size={position_size:.6f} rejected",
                )
        else:
            # 加仓使用限价单，控制价格
            # 只在趋势确认时加仓（价格高于长期EMA，且短期EMA也高于长期EMA表示上升趋势）
            if (
                self.atr_value > 0
                and price > self.ma2_value
                and self.ma1_value > self.ma2_value
                and self.ma1_value > self.ma1_last
            ):  # 确保短期EMA正在上升
                order_price = (
                    price_with_slippage + self.atr_value * 0.25
                )  # 减小加仓价差，避免过度追高

                orders = self.buy(
                    self.vt_symbol, order_price, position_size
                )  # 使用限价单

                if orders:
                    for order in orders:
                        self.current_orders[order] = {
                            "direction": "buy",
                            "type": "pyramid",
                            "level": current_unit,
                            "price": order_price,
                            "status": "not_traded",
                        }

                if orders:
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
        根据当前资金和风险百分比计算仓位大小
        """
        if atr <= 0:
            self.write_log(f"ATR无效({atr}), 使用固定仓位: {self.fixed_size}")
            return self.fixed_size  # 如果ATR无效，使用固定仓位

        # 使用当前资金进行计算
        account_balance = self.current_capital

        # 计算基于账户资金的风险金额
        risk_amount = account_balance * self.risk_per_unit

        # 根据ATR计算每单位的风险
        risk_per_share = atr

        # 计算仓位大小
        position_size = risk_amount / risk_per_share

        # 【埋点13】仓位计算详情
        self.log_tag(
            "SIZE_CALC",
            f"account={account_balance:.2f} risk_amt={risk_amount:.2f} atr={risk_per_share:.4f} calc_size={position_size:.6f}",
        )

        # 确保仓位大小至少为最小下单量
        if position_size < self.min_volume:
            self.log_tag(
                "SIZE_CALC",
                f"bump to min_volume: {position_size:.6f} -> {self.min_volume:.6f}",
            )
            position_size = self.min_volume

        # 不能超过固定大小（固定大小视为单次下单量上限，可为浮点）
        final_size = min(position_size, float(self.fixed_size))
        if final_size != position_size:
            self.log_tag(
                "SIZE_CALC",
                f"cap to fixed_size: {position_size:.6f} -> {final_size:.6f}",
            )

        return final_size
