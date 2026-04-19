---
id: REQ-047
title: "集成测试缺失（无 tests/ 目录、无端到端验证）"
status: active
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T16:00:00"
priority: P1
cluster: Infrastructure
relations:
  depends_on: [REQ-048]
  related_to: []
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 603 个单元测试但 0 个集成测试，无 tests/ 目录和测试固件"
    reason: "初始发现"
    snapshot: "项目无集成测试，无法验证模块间交互（订单生命周期、Python-Rust 互操作、RPC 通信等）"
---

# 集成测试缺失

## 描述
项目有 603 个单元测试，但完全没有集成测试。没有 `tests/` 目录，没有测试固件，没有端到端验证。关键未验证的跨模块路径：
1. 完整订单生命周期（提交 → 撮合 → 仓位更新）
2. Python-Rust 互操作端到端（策略注册 → 事件路由 → 回调执行）
3. 数据库读写往返（SQLite/Parquet 真实磁盘 I/O）
4. RPC 客户端-服务端通信

## 验收标准
- [ ] 创建 `tests/` 目录结构
- [ ] 订单生命周期集成测试：submit → fill → position update
- [ ] Python 策略端到端测试：register → on_init → on_tick → send_order
- [ ] 数据库往返测试：write → read → verify
