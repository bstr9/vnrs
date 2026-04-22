---
id: REQ-019
title: "自定义优化目标返回 0.0"
status: completed
created_at: "2026-04-19T12:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  related_to: [REQ-018]
  cluster: Bug-Fix
versions:
  - version: 1
    date: "2026-04-19T12:00:00"
    author: ai
    context: "代码审查确认 src/backtesting/optimization.rs:505 自定义优化目标 (OptimizationTarget::Custom) 的 evaluate 始终返回 0.0，未调用用户提供的评估函数。"
    reason: "自定义优化目标完全不可用，用户无法按自定义指标优化策略参数"
    snapshot: "自定义优化目标实现：接受用户提供的评估闭包/函数，返回实际计算结果"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "修复完成：OptimizationTarget::Custom 改为 Box<dyn Fn(&BacktestingStatistics) -> f64 + Send + Sync>，extract_target_value() 实际调用闭包"
    reason: "Bug 修复完成"
    snapshot: "自定义优化目标现在接受闭包并返回实际计算结果"
---

# 自定义优化目标返回 0.0

## 描述

`optimization.rs:505` 中 `OptimizationTarget::Custom` 变体的 `evaluate()` 方法始终返回 `0.0`，未调用用户提供的评估函数。这使得自定义优化目标（如按夏普比率、最大回撤等自定义指标优化）完全不可用。

## 验收标准

- [x] `OptimizationTarget::Custom` 接受用户提供的评估闭包 `Box<dyn Fn(&BacktestResult) -> f64>`
- [x] `evaluate()` 调用用户闭包并返回实际结果
- [x] 闭包在优化循环中被正确调用
- [x] 测试：自定义优化目标返回正确的评估值

## 影响范围

- `src/backtesting/optimization.rs` — Custom variant evaluate() 实现
