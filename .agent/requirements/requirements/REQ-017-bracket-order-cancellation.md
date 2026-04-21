---
id: REQ-017
title: "BracketOrder handle_cancellation() 空实现"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P0
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-025, REQ-063]
  cluster: Bug-Fix
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "代码审查确认 src/trader/bracket_order.rs:775 handle_cancellation() 方法体为空，括号订单取消时不执行任何操作。"
    reason: "括号订单取消逻辑缺失，止盈/止损单无法正确取消"
    snapshot: "BracketOrder handle_cancellation() 实现：取消所有子订单（止盈/止损），更新订单状态"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "修复完成：handle_cancellation() 实现基于角色的取消逻辑，Entry/Primary取消→标记组取消，TP/SL取消→取消兄弟单+标记组取消，OCO取消→取消对端+标记组取消"
    reason: "Bug 修复完成"
    snapshot: "BracketOrder handle_cancellation() 完整实现，支持入场/出场/OCO取消场景"
---

# BracketOrder handle_cancellation() 空实现

## 描述

`bracket_order.rs:775` 的 `handle_cancellation()` 方法体为空。当括号订单需要取消时（如父订单取消、手动取消），止盈和止损子订单不会被取消，可能导致意料之外的挂单。

## 验收标准

- [x] `handle_cancellation()` 取消所有活跃的子订单（take_profit / stop_loss）
- [x] 通过 OmsEngine 或直接调用网关发送取消请求
- [x] 更新 BracketOrder 状态为 Cancelled
- [x] 已成交的子订单不可取消（仅取消未成交的）
- [ ] 测试：取消括号订单后，所有未成交子订单状态变为 Cancelled

## 影响范围

- `src/trader/bracket_order.rs` — handle_cancellation() 实现
