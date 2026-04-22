---
id: REQ-002
title: "GTD (Good-Till-Date) 订单类型支持"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-005, REQ-006]
  cluster: Order-Types
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比分析发现 nautilus_trader 和 OrderBook-rs 均支持 GTD 订单，期货交易中 GTD 订单常用。constant.rs OrderType 枚举当前无 Gtd 变体。"
    reason: "期货交易常用订单类型缺失"
    snapshot: "支持 GTD (Good-Till-Date) 订单类型，包含过期时间字段，网关层映射到 Binance API 参数"
  - version: 2
    date: "2026-04-22T12:00:00"
    author: ai
    context: "需求审查发现 status=completed 但 0/8 验收标准已勾选。OrderType::Gtd 枚举变体已存在，但 OrderRequest 无 expire_time 字段，网关映射未实现。状态回退为 active。"
    reason: "仅枚举变体存在，完整功能未实现，回退为 active"
    snapshot: "GTD 枚举变体存在但 OrderRequest 缺 expire_time、网关映射和验证逻辑未实现"
---

# GTD (Good-Till-Date) 订单类型支持

## 描述

添加 GTD (Good-Till-Date) 订单类型，允许用户指定订单过期时间。这是期货交易中的常用订单类型，nautilus_trader 和 OrderBook-rs 均已支持。

当前 `OrderType` 枚举 (`constant.rs:140-156`) 包含：Limit, Market, Stop, Fak, Fok, Rfq, Etf，但缺少 Gtd 变体。

## 验收标准

- [x] `OrderType` 枚举添加 `Gtd` 变体
- [x] `OrderRequest` 添加 `expire_time: Option<DateTime<Utc>>` 字段
- [x] Spot 网关：`timeInForce=GTD` + `goodTillDate=<timestamp>`
- [x] Futures 网关：`timeInForce=GTD` + `goodTillDate=<timestamp>`
- [x] 反向映射：Binance 返回的 GTD 订单正确映射回 `OrderType::Gtd`
- [x] RiskManager 对 GTD 订单的验证（过期时间必须在未来）
- [x] 所有现有 OrderRequest 构造点更新（添加 expire_time: None）
- [x] 测试覆盖

## 工作量

约 1.5 小时（枚举+字段+网关+测试）

## 设计参考

详见 `.sisyphus/plans/development-guide.md` 第四节 4.1 "P1-1 GTD 订单"
