---
id: REQ-015
title: "确定性重放 (Deterministic Replay)"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-22T00:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  depends_on: [REQ-001]
  cluster: Infrastructure
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比发现 nautilus_trader 核心优势之一是确定性事件循环。OrderBook-rs 也通过 FileJournal + CRC32 校验实现确定性重放。vnrs 当前无此能力。"
    reason: "调试复杂策略问题需要可重现的事件序列"
    snapshot: "实现确定性重放：事件按时间戳严格排序，支持从日志恢复事件序列并重放"
---

# 确定性重放 (Deterministic Replay)

## 描述

nautilus_trader 的核心优势之一是确定性架构：相同输入永远产生相同输出。这对于调试复杂策略问题至关重要。

OrderBook-rs 通过 FileJournal + CRC32 校验 + 段轮换实现确定性重放。

## 验收标准

- [ ] 事件按时间戳严格排序（无并发不确定性）
- [ ] EventJournal：事件持久化（追加写入）
- [ ] CRC32 校验确保数据完整性
- [ ] replay_from_journal()：从日志恢复事件序列并重放
- [ ] 相同输入 → 相同输出（可复现的回测/调试）

## 工作量

2-3 天
