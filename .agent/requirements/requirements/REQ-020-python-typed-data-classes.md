---
id: REQ-020
title: "Python 类型化数据类（BarData, TickData, OrderData, TradeData）"
status: active
completed_at: "2026-04-21T00:00:00"
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T12:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-022, REQ-023]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "API 对比分析发现 nautilus_trader 使用类型化数据对象（Bar, QuoteTick, TradeTick, OrderBook），vnrs 当前 Python 端接收的数据为 PyDict/PyAny，缺少类型提示和 IDE 补全。用户原始需求：框架面向量化开发 Python 的接口和逻辑是否友好。"
    reason: "Python 策略开发者需要类型化的数据类，替代无类型的 dict，提供 IDE 补全和类型安全"
    snapshot: "Python 端提供 BarData, TickData, OrderData, TradeData 等类型化数据类，通过 PyO3 暴露 Rust 结构体"
  - version: 2
    date: "2026-04-22T12:00:00"
    author: ai
    context: "需求审查发现 status=completed 但 0/7 验收标准已勾选。PyTickData/PyOrderData/PyTradeData 已在 data_types.rs 中实现，但策略回调参数可能仍使用 dict，无 .pyi 类型提示。状态回退为 active。"
    reason: "数据类已实现但回调参数类型和 IDE 类型提示未完成，回退为 active"
    snapshot: "PyTickData/PyOrderData/PyTradeData 类存在，但策略回调仍传 dict，无 .pyi 类型提示"
---

# Python 类型化数据类（BarData, TickData, OrderData, TradeData）

## 描述

当前 Python 策略接收的 on_bar/on_tick/on_order/on_trade 回调参数为无类型 dict（PyDict），缺乏：
- IDE 自动补全和类型提示
- 属性访问（只能 dict["key"]）
- 文档和可发现性

nautilus_trader 使用强类型数据对象（Bar, QuoteTick, TradeTick, Instrument 等），Python 开发者体验远优于 dict。

## 验收标准

- [ ] `PyBarData` 类：symbol, exchange, datetime, open, high, low, close, volume, turnover 等属性
- [ ] `PyTickData` 类：symbol, exchange, datetime, last_price, volume, turnover, bid/ask 等属性
- [ ] `PyOrderData` 类：orderid, symbol, direction, price, volume, status 等属性
- [ ] `PyTradeData` 类：tradeid, orderid, symbol, direction, price, volume 等属性
- [ ] 策略回调参数从 dict 改为类型化对象（向后兼容过渡期可同时支持）
- [ ] Python 类型提示（.pyi 或 typing）支持 IDE 补全
- [ ] 与现有 Rust 结构体 (BarData, TickData, OrderData, TradeData) 对齐

## 影响范围

- `src/python/strategy.rs` — 回调参数类型
- `src/python/backtesting_bindings.rs` — 回测引擎数据传递
- `src/python/bindings.rs` — 实盘引擎数据传递
- 新增 `src/python/data_types.rs` — 类型化数据类定义

## 向后兼容

过渡策略：新代码使用类型化对象，旧 dict 接口标记 deprecated 但不立即移除。
