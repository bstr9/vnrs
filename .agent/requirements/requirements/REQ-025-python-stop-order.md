---
id: REQ-025
title: "Python 止损单完整功能（发送/回调/取消）"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T19:04:20"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: [REQ-028, REQ-029]
  refined_by: [REQ-028, REQ-029]
  related_to: [REQ-017, REQ-021]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "API 对比分析发现当前 Python Strategy 仅有 buy/sell/short/cover 方法（发限价/市价单），无法发送止损单。vnpy 的 CtaTemplate 有 send_order 和 send_stop_order 两种下单方式。"
    reason: "策略需要发送止损单（Stop Order），当前 Python 接口不支持"
    snapshot: "Python Strategy 添加 send_stop_order() 方法，支持发送 Stop 类型订单"
  - version: 2
    date: "2026-04-20T10:00:00"
    author: ai
    context: "需求整理合并：REQ-028(on_stop_order 回调) 和 REQ-029(cancel_stop_order) 是止损单功能的配套部分，合并为一个完整需求。"
    reason: "合并 REQ-028、REQ-029，止损单发送/回调/取消是一个完整功能"
    snapshot: "Python 止损单完整功能：send_stop_order 发送、on_stop_order 触发回调、cancel_stop_order 取消"
  - version: 3
    date: "2026-04-19T19:04:20"
    author: ai
    context: "元数据自动同步"
    reason: "自动补充反向关系: refined_by"
    snapshot: "自动同步元数据"
---

# Python 止损单完整功能（发送/回调/取消）

## 描述

当前 Python `Strategy` 的 `buy()`/`sell()`/`short()`/`cover()` 方法只发 Limit/Market 订单，止损单功能完全缺失。Rust 端 `BaseStrategy` 已有完整实现：`send_stop_order`、`cancel_stop_order`、`on_stop_order` 回调。

止损单三个环节缺一不可：
1. **发送** — `send_stop_order()` 在指定价格挂止损单
2. **回调** — `on_stop_order()` 止损单触发时通知策略
3. **取消** — `cancel_stop_order()` 市场变化时取消止损单

vnpy 的 CtaTemplate 提供 `send_order()` 和 `send_stop_order()` 两种下单接口。

## 验收标准

### 发送止损单
- [ ] Python `Strategy` 添加 `send_stop_order(direction, price, volume, offset?)` 方法
- [ ] 底层创建 OrderType::Stop 的 StopOrderRequest
- [ ] OrderFactory 支持 stop order 创建
- [ ] 回测引擎支持 Stop 订单成交逻辑（价格触及时转为市价单）
- [ ] 与 buy/sell/short/cover 方法一致的接口风格

### 止损单触发回调（原 REQ-028）
- [ ] Python `Strategy` 添加 `on_stop_order(self, stop_orderid)` 回调
- [ ] `PythonStrategyAdapter` 转发 on_stop_order 事件到 Python
- [ ] 回测引擎止损单触发时调用 on_stop_order
- [ ] 与 on_order / on_trade 回调一致的接口风格

### 取消止损单（原 REQ-029）
- [ ] Python `Strategy` 添加 `cancel_stop_order(stop_orderid)` 方法
- [ ] 方法将取消请求排入队列（与 cancel_order 一致的机制）
- [ ] 引擎处理取消请求并通知 on_stop_order
- [ ] 从 `active_orderids` 或 `active_stop_orderids` 中移除

## 影响范围

- `src/python/strategy.rs` — send_stop_order / on_stop_order / cancel_stop_order
- `src/python/strategy_adapter.rs` — 转发 on_stop_order 事件
- `src/python/order_factory.rs` — Stop order 创建支持
- `src/backtesting/engine.rs` — Stop order 成交逻辑
