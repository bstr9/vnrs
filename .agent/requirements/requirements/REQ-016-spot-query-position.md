---
id: REQ-016
title: "Spot 网关 query_position() 空实现"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P0
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-023, REQ-059]
  cluster: Bug-Fix
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "代码审查确认 src/gateway/binance/spot_gateway.rs:1204 query_position() 直接返回 Ok(())，Spot 账户从不查询仓位。用户要求修复所有未实现的代码。"
    reason: "Spot 账户仓位查询功能缺失，实盘无法获取真实持仓"
    snapshot: "Spot 网关 query_position() 实现：调用 Binance Spot /api/v3/account 获取余额并映射为 PositionData"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "修复完成：query_position() 调用 query_position_impl()，查询 /api/v3/account，将非零余额映射为 PositionData 并通过事件引擎发送"
    reason: "Bug 修复完成"
    snapshot: "Spot 网关 query_position() 完整实现，返回账户余额作为仓位数据"
---

# Spot 网关 query_position() 空实现

## 描述

`spot_gateway.rs:1204` 的 `query_position()` 方法直接返回 `Ok(())`，不执行任何查询。Spot 账户永远不会获取仓位信息。

现货账户虽无期货"仓位"概念，但 Binance Spot API 的 `/api/v3/account` 端点返回各币种余额。应将非零余额映射为 PositionData（方向为 Long，volume 为可用余额），使策略能通过统一的仓位接口获取持仓。

## 验收标准

- [x] `query_position()` 调用 Binance Spot `/api/v3/account` 获取余额
- [x] 将非零余额映射为 `PositionData`（direction=Long, volume=free+locked）
- [x] 通过事件引擎发送 `EventPosition(PositionData)` 
- [x] 不影响 Futures 网关现有实现
- [ ] 测试：Spot 网关 query_position 返回正确的 PositionData

## 影响范围

- `src/gateway/binance/spot_gateway.rs` — query_position() 实现
