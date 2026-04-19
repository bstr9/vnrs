---
id: REQ-028
title: "Python on_stop_order 回调"
status: deprecated
created_at: "2026-04-19T14:00:00"
updated_at: "2026-04-20T10:00:00"
priority: P1
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-025]
  merged_from: []
versions:
  - version: 1
    date: "2026-04-19T14:00:00"
    author: ai
    context: "代码分析发现 Rust StrategyTemplate 有 on_stop_order 回调，但 Python Strategy 基类未暴露。"
    reason: "止损单触发后策略需要收到通知"
    snapshot: "Python Strategy 添加 on_stop_order 回调"
  - version: 2
    date: "2026-04-20T10:00:00"
    author: ai
    context: "需求整理：合并到 REQ-025 v2 的验收标准中"
    reason: "合并到 REQ-025（止损单完整功能）"
    snapshot: "已合并到 REQ-025"
---

# 已归档 — 合并到 REQ-025

本需求已合并到 REQ-025（Python 止损单完整功能）v2 的验收标准中。
