---
id: REQ-090
title: "实盘K线生成与on_bar回调触发"
status: completed
level: story
priority: P0
cluster: strategy-data
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-29T16:00:00"
relations:
  depends_on: []
  refines: []
  related_to: [REQ-082, REQ-059]
  - version: 2
    date: "2026-04-29T16:00:00"
    author: ai
    context: "Phase 10 实现：添加 SubscribeRequest.interval 字段；两个 Gateway 添加 kline_subscriptions + @kline 解析；constants.rs 添加 INTERVAL_BINANCE2VT 反向映射；on_bar 回调在 k.x==true 时触发"
    reason: "核心功能实现完成，clippy pedantic 零警告验证通过"
    snapshot: "Binance @kline WebSocket 已实现，K线关闭触发 on_bar，支持多周期订阅"
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "代码审计发现：Binance Gateway只订阅了@ticker和@depth5流，没有订阅@kline流。StrategyEngine没有将实时tick喂给BarGenerator。导致实盘策略只能收到on_tick回调，永远收不到on_bar回调——所有基于K线的策略（均线、布林带等）在实盘中完全无法工作。"
    reason: "这是量化交易的最关键缺失：没有实盘K线就没有基于K线的策略，而绝大多数量化策略都是K线驱动的"
    snapshot: "实盘策略能稳定收到on_bar回调，K线数据从Binance @kline WebSocket流或tick→BarGenerator聚合产生"
---

# 实盘K线生成与on_bar回调触发

## 描述
当前实盘策略**只能收到 `on_tick` 回调，永远收不到 `on_bar` 回调**。原因是：

1. Binance Gateway 只订阅了 `@ticker` 和 `@depth5` 两个WebSocket流，没有订阅 `@kline_<interval>` 流
2. StrategyEngine 没有将实时 tick 喂给 BarGenerator，即使 BarGenerator 存在也无人调用
3. DataEngine 虽然有 BarGenerator，但没有被 StrategyEngine 正确使用

需要实现两种K线来源（双通道）：
1. **WebSocket @kline 流**（推荐）：Binance 提供 `{symbol}@kline_{interval}` 实时K线推送，1分钟K线在每分钟结束时推送完整K线
2. **Tick→BarGenerator 聚合**（fallback）：从 `@trade` 或 `@ticker` 流聚合生成K线

两种方式都需要：
- K线完成时触发策略的 `on_bar` 回调
- 支持多周期同时订阅
- 未完成的K线（in-progress bar）也能被策略访问

## 验收标准
- [x] Binance Gateway 的 `subscribe()` 方法支持订阅 `@kline_1m` WebSocket流
- [x] 收到 `@kline` 消息后，K线关闭时触发 `on_bar` 回调
- [x] StrategyEngine 将 tick 事件喂给策略的 BarGenerator 作为备用K线来源
- [x] BarGenerator 1分钟K线完成时也触发 `on_bar`
- [x] 策略可同时收到 `on_tick` 和 `on_bar` 回调
- [x] 支持订阅任意周期K线（1m/5m/15m/1h/4h/1d 等）
- [x] `@kline` 流断线重连后自动恢复
