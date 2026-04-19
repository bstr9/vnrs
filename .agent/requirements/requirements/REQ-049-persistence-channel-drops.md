---
id: REQ-049
title: "持久化通道静默丢弃订单/成交/仓位数据"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T19:25:00"
priority: P0
cluster: Bug-Fix
relations:
  depends_on: [REQ-001]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 trader/engine.rs:756-767 中 persist_tx.try_send() 失败时仅 warn 日志，订单/成交/仓位数据永久丢失"
    reason: "初始发现"
    snapshot: "持久化通道满时订单/成交/仓位数据被静默丢弃，崩溃恢复后数据不一致"
  - version: 2
    date: "2026-04-20T19:25:00"
    author: ai
    context: "修复完成：实现溢出文件备份机制，通道满时写入 .rstrader/overflow/persist_overflow.jsonl，drain 任务自动回放"
    reason: "Bug 修复完成"
    snapshot: "持久化溢出数据写入本地文件，通道恢复时自动回放，添加统计接口 get_persist_stats()"
---

# 持久化通道静默丢弃数据

## 描述
`src/trader/engine.rs` lines 756-767 中，当持久化通道（`persist_tx`）满时，`try_send()` 返回 `Err` 后仅打印 warn 日志，订单、成交、仓位数据被永久丢弃。这意味着在高负载或持久化线程卡顿的情况下，交易数据可能永久丢失，崩溃恢复后数据不一致。

## 验收标准
- [x] 使用有界通道 + 背压机制，或在溢出时写入溢出日志可回放
- [x] 持久化失败时至少将数据写入本地文件作为备份
- [x] 添加指标统计丢弃的数据条数，便于监控
