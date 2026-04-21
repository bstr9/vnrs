---
id: REQ-006
title: "Pegged 订单支持"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-002, REQ-005]
  cluster: Order-Types
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比发现 OrderBook-rs 和 tesser 均支持 Pegged 订单，vnrs 无任何 pegged 相关代码。做市策略需要追踪最优价的 Pegged 订单。"
    reason: "做市策略需要 Pegged 订单类型"
    snapshot: "实现 Pegged 订单，支持追踪最优价（PeggedBest）等变体"
---

# Pegged 订单支持

## 描述

Pegged 订单是指价格自动追踪市场最优价的订单类型，做市策略常用。OrderBook-rs 和 tesser 均已实现。

tesser 实现了：PeggedBest（追踪最优价）、Sniper（等待目标价）、TrailingStop。

## 验收标准

- [ ] `OrderType` 枚举添加 Pegged 变体
- [ ] `OrderRequest` 添加 pegged 相关字段（offset, peg_type）
- [ ] 通过 `order_emulator.rs` 本地模拟（交易所不原生支持时）
- [ ] PeggedBest：追踪最优买/卖价 ± offset
- [ ] 价格变化时自动修改订单价格

## 工作量

约 1 天
