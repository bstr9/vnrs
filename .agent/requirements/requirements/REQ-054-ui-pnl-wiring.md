---
id: REQ-054
title: "UI PnL 显示未接入策略引擎（硬编码 0.0）"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T20:00:00"
priority: P2
cluster: Infrastructure
relations:
  depends_on: []
  related_to: [REQ-003, REQ-061, REQ-062]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 main_window.rs:362 中 today_pnl 硬编码 0.0，StrategyEngine 内部有 strategy_pnl 数据但未接入 UI"
    reason: "初始发现"
    snapshot: "UI 的 today_pnl 显示硬编码 0.0，StrategyEngine 已有 per-strategy PnL 数据但未接入"
  - version: 2
    date: "2026-04-20T20:00:00"
    author: ai
    context: "strategy_cache 扩展为 3-tuple 含 total_pnl，后台刷新任务调用 get_strategy_total_pnl() 获取实际值"
    reason: "修复完成"
    snapshot: "UI today_pnl 从 StrategyEngine 读取实际 PnL 数据，支持按策略分别显示"
---

# UI PnL 显示未接入策略引擎

## 描述
`src/trader/ui/main_window.rs:362` 中 `today_pnl` 字段硬编码为 `0.0`，并有 `// TODO: per-strategy PnL tracking` 注释。`StrategyEngine` 内部已跟踪 `strategy_pnl` 和 `strategy_unrealized_pnl`，但 UI 没有读取这些数据。用户在 UI 中始终看到 0 PnL。

## 验收标准
- [x] UI 从 StrategyEngine 读取 per-strategy realized/unrealized PnL
- [x] `today_pnl` 字段显示实际值
- [x] 支持按策略分别显示 PnL
