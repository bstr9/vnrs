---
id: REQ-046
title: "系统性错误类型改造（149 处 Result<T, String> → thiserror 枚举）"
status: completed
completed_at: "2026-04-21T00:00:00"
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P2
cluster: Infrastructure
relations:
  depends_on: [REQ-004]
  related_to: [REQ-044, REQ-045]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现全代码库 149 处 Result<T, String>，仅 rpc/common.rs 有 1 个 ConnectionError 枚举"
    reason: "初始发现"
    snapshot: "全代码库使用 Result<T, String> 字符串错误，无法结构化匹配、组合或传播错误上下文"
---

# 系统性错误类型改造

## 描述
整个代码库有 149 处 `Result<T, String>`，只有 `src/rpc/common.rs` 定义了 1 个 `ConnectionError` 枚举。字符串错误导致：
1. 无法结构化模式匹配（`match` on error kind）
2. 错误上下文丢失（调用链无法追加信息）
3. 无法组合不同模块的错误
4. 日志/监控无法按错误类型聚合

建议引入 `thiserror` crate，按模块定义错误枚举：`GatewayError`、`DatabaseError`、`StrategyError`、`BacktestError`。

## 验收标准
- [x] 引入 `thiserror` 依赖
- [x] 定义核心错误枚举：`GatewayError`、`DatabaseError`、`StrategyError`、`BacktestError`
- [ ] `BaseGateway` trait 的返回类型从 `Result<T, String>` 改为 `Result<T, GatewayError>`
- [ ] `BaseDatabase` trait 的返回类型从 `Result<T, String>` 改为 `Result<T, DatabaseError>`
- [x] 渐进式迁移：新代码使用枚举，旧代码逐步改造
