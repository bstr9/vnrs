---
id: REQ-050
title: "Python 策略回调失败无恢复机制"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P2
cluster: Python-API
relations:
  depends_on: []
  related_to: [REQ-038]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 strategy_adapter.rs:261-374 中 on_init/on_start 等回调失败后仅日志，策略留在不一致状态"
    reason: "初始发现"
    snapshot: "Python 策略回调（on_init/on_start 等）失败后仅记录日志，策略继续运行在可能不一致的状态"
---

# Python 策略回调失败无恢复机制

## 描述
`src/python/strategy_adapter.rs` lines 261-374 中，当 Python 回调（`on_init`、`on_start` 等）抛出异常时，错误仅被记录到日志，策略保持在当前状态继续运行。这可能导致策略在不一致状态下继续交易（如 `on_init` 失败但策略仍在运行）。

## 验收标准
- [ ] `on_init` 失败后策略状态转为 Error，不进入 Running 状态
- [ ] `on_start` 失败后策略自动停止，不接收后续行情
- [ ] 提供策略状态查询接口（Running/Error/Stopped）
- [ ] 提供策略重置/重启机制
