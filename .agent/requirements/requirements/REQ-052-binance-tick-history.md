---
id: REQ-052
title: "BinanceDatafeed tick 级历史数据不可用"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P2
cluster: Infrastructure
relations:
  depends_on: [REQ-001]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 datafeed.rs:203-206 中 BinanceDatafeed.query_tick_history() 返回错误，Binance REST API 不支持 tick 级历史"
    reason: "初始发现"
    snapshot: "Binance REST API 不提供 tick 级历史数据，tick 级回测无法进行"
---

# BinanceDatafeed tick 级历史数据不可用

## 描述
`src/trader/datafeed.rs:203-206` 中 `BinanceDatafeed::query_tick_history()` 返回错误，因为 Binance REST API 不提供 tick 级历史数据。当前没有替代数据源（如从已录制的数据库加载）。这使得基于 tick 数据的回测和策略预热不可用。

## 验收标准
- [x] 支持从已录制的数据库（SQLite/Parquet）加载 tick 历史数据
- [x] 在 `BaseDatafeed::query_tick_history()` 默认实现中查找数据库回退路径
- [x] Python 端 `load_tick()` 在回测模式下可返回 tick 数据
