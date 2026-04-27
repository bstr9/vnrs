---
id: REQ-081
title: "策略历史K线加载与数据库预热"
status: active
level: story
priority: P0
cluster: strategy-data
created_at: "2026-04-26T12:00:00"
updated_at: "2026-04-26T12:00:00"
relations:
  depends_on: [REQ-052]
  refines: []
  related_to: [REQ-018, REQ-055]
versions:
  - version: 1
    date: "2026-04-26T12:00:00"
    author: ai
    context: "用户已接入Binance市场，想要做量化交易。策略启动时需要加载历史K线来初始化ArrayManager指标计算，当前策略的load_bar虽然代码存在但实际运行时数据库可能为空，需要完整的数据下载→存储→加载链路。"
    reason: "量化策略必须依赖历史数据预热指标，否则策略启动时指标为NaN无法决策"
    snapshot: "策略启动时能自动加载N天历史K线到ArrayManager，确保指标可计算"
---

# 策略历史K线加载与数据库预热

## 描述
量化策略在启动时（`on_init`）需要加载历史K线数据来初始化 `ArrayManager`，使得技术指标（MA、RSI、MACD等）在策略开始接收实时数据前就已经有值。当前链路存在以下断裂：

1. `BaseStrategy::load_bar()` 是空壳（返回 `Vec::new()`），`StrategyContext::load_bar()` 虽然实现了从数据库加载，但数据库中可能没有数据
2. `DataDownloadManager` 能从Binance下载K线，但策略启动时没有自动触发下载
3. 缺少"策略启动前自动检查并下载所需历史数据"的机制

## 验收标准
- [ ] 策略在 `on_init` 中调用 `context.load_bar()` 能获取到历史K线数据
- [ ] 如果数据库中没有所需数据，自动触发下载并等待完成后返回
- [ ] 支持指定加载天数（如10天、30天）和K线周期（1m、5m、1h、1d等）
- [ ] 下载完成后数据持久化到SQLite/数据库，下次启动无需重新下载
- [ ] `ArrayManager` 在 `on_init` 完成后 `is_inited()` 返回 `true`
- [ ] 在GUI策略面板上显示数据加载进度
