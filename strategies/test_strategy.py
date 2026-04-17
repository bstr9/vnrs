"""
Test Strategy

Migrated from vnpy_ctastrategy. Tests basic order operations
(market, limit, stop, cancel_all).

Changes from vnpy version:
- Import from trade_engine.CtaStrategy instead of vnpy_ctastrategy.CtaTemplate
- tick.limit_up / tick.limit_down may not be available in vnrs;
  fallback to ask_price_1 / bid_price_1
- stop=True parameter removed from order calls
"""

from time import time
from trade_engine import CtaStrategy


class TestStrategy(CtaStrategy):
    """"""

    author = "用Python的交易员"

    test_trigger: int = 10

    tick_count: int = 0
    test_all_done: bool = False

    parameters = ["test_trigger"]
    variables = ["tick_count", "test_all_done"]

    def on_init(self) -> None:
        """
        Callback when strategy is inited.
        """
        self.write_log("策略初始化")

        self.test_funcs = [
            self.test_market_order,
            self.test_limit_order,
            self.test_cancel_all,
            self.test_stop_order,
        ]

        self.last_tick = None

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
        if self.test_all_done:
            return

        self.last_tick = tick

        self.tick_count += 1
        if self.tick_count >= self.test_trigger:
            self.tick_count = 0

            if self.test_funcs:
                test_func = self.test_funcs.pop(0)

                start = time()
                test_func()
                time_cost = (time() - start) * 1000
                self.write_log(f"耗时{time_cost}毫秒")
            else:
                self.write_log("测试已全部完成")
                self.test_all_done = True

        self.put_event()

    def on_bar(self, bar) -> None:
        """
        Callback of new bar data update.
        """
        pass

    def on_order(self, order) -> None:
        """
        Callback of new order data update.
        """
        self.put_event()

    def on_trade(self, trade) -> None:
        """
        Callback of new trade data update.
        """
        self.put_event()

    def _get_tick_price(self, field: str) -> float:
        """Get price from tick, with fallback for vnrs compatibility."""
        if self.last_tick is None:
            return 0.0
        if isinstance(self.last_tick, dict):
            # vnrs passes tick as dict
            if field == "limit_up":
                return self.last_tick.get(
                    "ask_price_1", self.last_tick.get("last_price", 0)
                )
            elif field == "limit_down":
                return self.last_tick.get(
                    "bid_price_1", self.last_tick.get("last_price", 0)
                )
            return self.last_tick.get(field, 0)
        else:
            # vnpy-style object
            if field == "limit_up":
                return getattr(
                    self.last_tick,
                    "limit_up",
                    getattr(self.last_tick, "ask_price_1", 0),
                )
            elif field == "limit_down":
                return getattr(
                    self.last_tick,
                    "limit_down",
                    getattr(self.last_tick, "bid_price_1", 0),
                )
            return getattr(self.last_tick, field, 0)

    def test_market_order(self) -> None:
        """"""
        price = self._get_tick_price("limit_up")
        if price <= 0:
            self.write_log("没有最新tick数据")
            return

        self.buy(self.vt_symbol, price, 1)
        self.write_log("执行市价单测试")

    def test_limit_order(self) -> None:
        """"""
        price = self._get_tick_price("limit_down")
        if price <= 0:
            self.write_log("没有最新tick数据")
            return

        self.buy(self.vt_symbol, price, 1)
        self.write_log("执行限价单测试")

    def test_stop_order(self) -> None:
        """"""
        price = self._get_tick_price("ask_price_1")
        if price <= 0:
            self.write_log("没有最新tick数据")
            return

        # Note: vnrs Python API doesn't support stop=True parameter
        # This tests a regular buy at stop price level
        self.buy(self.vt_symbol, price, 1)
        self.write_log("执行停止单测试（以限价单替代）")

    def test_cancel_all(self) -> None:
        """"""
        self.cancel_all()
        self.write_log("执行全部撤单测试")
