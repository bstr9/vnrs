---
id: REQ-044
title: "RwLock .unwrap() 互斥锁中毒风险（risk.rs 25 处）"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T19:25:00"
priority: P0
cluster: Infrastructure
relations:
  depends_on: [REQ-046]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 trader/risk.rs 中 25 处 RwLock .unwrap() 调用，任一线程 panic 将导致整个交易引擎级联崩溃"
    reason: "初始发现"
    snapshot: "RiskManager 中 25 处 RwLock .unwrap() 调用，互斥锁中毒将级联崩溃整个交易引擎"
  - version: 2
    date: "2026-04-20T19:25:00"
    author: ai
    context: "修复完成：将所有 27 处 RwLock .unwrap() 替换为 .unwrap_or_else(|e| e.into_inner())"
    reason: "Bug 修复完成"
    snapshot: "risk.rs 中所有 RwLock 锁获取均使用安全恢复模式"
---

# RwLock .unwrap() 互斥锁中毒风险

## 描述
`src/trader/risk.rs` 中有 25 处 `.unwrap()` 调用在 `RwLock` 上（lines 183-493）。如果任何线程在持有锁时 panic，Rust 的互斥锁会被"中毒"（poisoned），后续所有 `.unwrap()` 调用都会 panic，导致整个交易引擎级联崩溃。

应使用 `.unwrap_or_else(|e| e.into_inner())` 恢复中毒锁（数据仍然有效，只是有线程在持锁期间 panic），或使用 `ParkResult` 等模式。

## 验收标准
- [x] `risk.rs` 中所有 27 处 `RwLock` `.unwrap()` 替换为 `.unwrap_or_else(|e| e.into_inner())` 或等效安全模式
- [x] 其他文件中的关键 `RwLock` `.unwrap()` 也需要审计和处理
- [x] 添加注释说明为什么选择恢复中毒锁而非 panic
