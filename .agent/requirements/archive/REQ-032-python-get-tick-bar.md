---
id: REQ-032
title: "Python get_tick/get_bar 最新数据查询"
status: deprecated
created_at: "2026-04-19T14:00:00"
updated_at: "2026-04-20T10:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-026]
  merged_from: []
versions:
  - version: 1
    date: "2026-04-19T14:00:00"
    author: ai
    context: "代码分析发现 Rust StrategyContext.get_tick/get_bar 可获取最新市场数据，但 Python 策略无此功能。"
    reason: "策略在任意时刻需要查询最新 tick/bar 价格"
    snapshot: "Python Strategy 添加 get_tick/get_bar 方法"
  - version: 2
    date: "2026-04-20T10:00:00"
    author: ai
    context: "需求整理：合并到 REQ-026 v2 的验收标准中"
    reason: "合并到 REQ-026（StrategyContext 暴露）"
    snapshot: "已合并到 REQ-026"
---

# 已归档 — 合并到 REQ-026

本需求已合并到 REQ-026（StrategyContext 暴露给 Python 策略）v2 的验收标准中。
