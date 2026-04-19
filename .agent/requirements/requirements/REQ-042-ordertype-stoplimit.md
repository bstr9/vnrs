---
id: REQ-042
title: "OrderType 枚举缺少 StopLimit 变体"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P2
cluster: Order-Types
relations:
  depends_on: []
  related_to: [REQ-025, REQ-005]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 order_factory.rs:519 中止损限价单使用 OrderType::Stop 代替，缺少 StopLimit 类型"
    reason: "初始发现"
    snapshot: "OrderType 枚举缺少 StopLimit 变体，止损限价单被存储为 OrderType::Stop 加 limit_price 字段作为变通"
---

# OrderType 枚举缺少 StopLimit 变体

## 描述
`src/trader/constant.rs` 中的 `OrderType` 枚举没有 `StopLimit` 变体。`src/python/order_factory.rs:519` 中止损限价单使用 `OrderType::Stop` 加 `limit_price` 字段作为变通方案。这导致：
1. 无法区分止损单和止损限价单
2. 订单路由逻辑可能对两种订单类型处理不正确
3. 交易所接口可能需要不同的 API 参数

## 验收标准
- [ ] `OrderType` 枚举新增 `StopLimit` 变体
- [ ] OrderFactory 止损限价单使用 `OrderType::StopLimit`
- [ ] Gateway 层正确映射 StopLimit 到交易所 API 参数
- [ ] 回测引擎正确处理 StopLimit 订单的撮合逻辑
