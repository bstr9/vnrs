---
id: REQ-041
title: "Strategy 日志/邮件集成缺失（write_log 用 println!, send_email 是空壳）"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P2
cluster: Infrastructure
relations:
  depends_on: [REQ-004]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 strategy.rs 和 engine.rs 中 write_log 用 println!, send_email 只打印不发送"
    reason: "初始发现"
    snapshot: "Python 策略的 write_log() 输出到 stdout 而非日志系统，send_email() 仅打印不发送邮件"
---

# Strategy 日志/邮件集成缺失

## 描述
两个问题：
1. `src/python/strategy.rs:404` 和 `src/python/engine.rs:400` 中 `write_log()` 使用 `println!` 输出日志，而不是通过 EventEngine 的日志系统。策略日志无法被持久化、过滤或统一管理。
2. `src/python/engine.rs:396` 中 `send_email()` 仅打印 "Email sent" 消息，没有实际邮件发送功能。

## 验收标准
- [ ] `write_log()` 通过 EventEngine 日志系统记录，与 Rust 端日志行为一致
- [ ] `send_email()` 接入邮件发送基础设施（或在没有配置时明确返回错误，而非静默假装成功）
