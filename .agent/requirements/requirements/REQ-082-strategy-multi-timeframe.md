---
id: REQ-082
title: "策略多周期K线订阅与合成"
status: active
level: story
priority: P0
cluster: strategy-data
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: []
  refines: []
  related_to: [REQ-055, REQ-081]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "量化策略常需要多周期确认，如1分钟做入场、5分钟做趋势判断、1小时做方向确认。BarSynthesizer已存在但策略层缺少便捷的多周期订阅接口。"
    reason: "多周期分析是量化交易的基本需求，单一周期策略信号质量不足"
    snapshot: "策略能订阅多个周期的K线，每个周期触发独立的on_bar回调"
---

# 策略多周期K线订阅与合成

## 描述
量化策略通常需要同时关注多个时间周期（如1分钟入场、5分钟趋势、1小时方向）。当前 `BarSynthesizer` 可以从低周期合成高周期K线，但策略层缺少统一的订阅机制。

需要实现：
1. 策略在 `on_init` 中声明需要哪些周期的K线（如 `["1m", "5m", "1h"]`）
2. 引擎自动从Binance `@kline_<interval>` WebSocket流订阅或从 `@trade` + `BarGenerator` 合成
3. 每个周期完成时独立触发 `on_bar` 回调，附带周期标识
4. 支持同步多周期回调 `on_bars()`（所有周期K线到齐才触发，用于对齐分析）

## 验收标准
- [ ] 策略 `on_init` 中可调用 `context.subscribe_bars(vt_symbol, intervals)` 订阅多周期
- [ ] 每个周期K线完成时触发 `on_bar`，`BarData` 包含 `interval` 字段标识周期
- [ ] 支持从1分钟K线合成5分钟/15分钟/1小时等标准周期
- [ ] `on_bars` 回调在所有订阅周期都更新时触发，传入 `HashMap<String, BarData>`
- [ ] Binance `@kline_1m` WebSocket流优先使用，fallback到 `BarGenerator` tick聚合
- [ ] 每个周期的 `ArrayManager` 独立维护，互不干扰
