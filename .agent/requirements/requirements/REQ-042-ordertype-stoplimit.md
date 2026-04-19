---
id: REQ-042
title: "OrderType 枚举缺少 StopLimit 变体"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T20:00:00"
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
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "新增 OrderType::StopLimit 变体，更新 12 个文件包括 gateway/backtesting/strategy/ui/python 层"
    reason: "修复完成"
    snapshot: "OrderType::StopLimit 变体已添加，Stop 显示'止损市价' StopLimit 显示'止损限价'，回测/gateway/python 全部支持"
---

# OrderType 枚举缺少 StopLimit 变体

## 描述
`src/trader/constant.rs` 中的 `OrderType` 枚举没有 `StopLimit` 变体。`src/python/order_factory.rs:519` 中止损限价单使用 `OrderType::Stop` 加 `limit_price` 字段作为变通方案。这导致：
1. 无法区分止损单和止损限价单
2. 订单路由逻辑可能对两种订单类型处理不正确
3. 交易所接口可能需要不同的 API 参数

## 验收标准
- [x] `OrderType` 枚举新增 `StopLimit` 变体
- [x] OrderFactory 止损限价单使用 `OrderType::StopLimit`
- [x] Gateway 层正确映射 StopLimit 到交易所 API 参数
- [x] 回测引擎正确处理 StopLimit 订单的撮合逻辑
