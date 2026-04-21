---
id: REQ-024
title: "运行时动态订阅/退订行情"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T12:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-026]
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "API 对比分析发现 nautilus_trader 支持运行时动态订阅/退订，vnrs 当前仅通过 vt_symbols 在策略初始化时静态订阅。多品种轮动策略需要运行时动态调整订阅列表。"
    reason: "多品种轮动策略需要运行时动态调整订阅列表"
    snapshot: "Python Strategy 提供 subscribe() / unsubscribe() 方法，支持运行时动态订阅/退订行情"
---

# 运行时动态订阅/退订行情

## 描述

当前策略只能通过 `vt_symbols` 在初始化时静态指定订阅品种，运行时无法动态添加或移除订阅。多品种轮动策略、关注列表策略等需要在运行时动态调整订阅列表。

## 验收标准

- [x] Python `Strategy` 添加 `subscribe(symbol, frequency?)` 方法
- [x] Python `Strategy` 添加 `unsubscribe(symbol)` 方法
- [ ] 调用 subscribe 后，网关发送对应的 WebSocket 订阅请求
- [ ] 调用 unsubscribe 后，网关发送对应的 WebSocket 退订请求
- [x] 回测模式下 subscribe/unsubscribe 操作为 no-op（数据已预加载）
- [ ] 与 OmsEngine 事件系统集成

## 影响范围

- `src/python/strategy.rs` — subscribe/unsubscribe 方法
- `src/gateway/` — 网关订阅/退订实现
- `src/strategy/engine.rs` — 订阅管理
