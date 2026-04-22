---
id: REQ-022
title: "Python on_depth() 回调暴露"
status: completed
completed_at: "2026-04-21T00:00:00"
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-19T12:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-020]
  merged_from: []
  cluster: Python-API
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "API 对比分析发现 nautilus_trader 提供 on_book 回调接收 OrderBook 数据，vnrs 的 Rust 端有 on_depth 回调但未暴露给 Python。做市策略和高频策略依赖盘口数据。"
    reason: "做市和高频策略需要盘口深度数据回调"
    snapshot: "Python Strategy 暴露 on_depth() 回调，接收 PyDepthData（买卖盘口数据）"
---

# Python on_depth() 回调暴露

## 描述

Rust 端 `StrategyTemplate` trait 有 `on_depth()` 回调方法，接收 `DepthData`（买卖盘口数据），但 Python `Strategy` 基类未暴露此回调。做市策略和高频策略依赖盘口深度数据进行报价决策。

## 验收标准

- [x] Python `Strategy` 类添加 `on_depth(self, depth: PyDepthData)` 回调
- [x] `PyDepthData` 类：symbol, exchange, datetime, bid_prices, bid_volumes, ask_prices, ask_volumes
- [x] `PythonStrategyAdapter` 在 Rust 端转发 on_depth 事件到 Python
- [x] 回测引擎支持深度数据回放（如数据可用）
- [x] 与 REQ-020 的类型化数据类对齐

## 影响范围

- `src/python/strategy.rs` — 添加 on_depth 回调
- `src/python/strategy_adapter.rs` — 转发 on_depth 事件
- `src/python/data_types.rs` — PyDepthData 定义（与 REQ-020 合并）
