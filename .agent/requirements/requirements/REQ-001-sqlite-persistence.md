---
id: REQ-001
title: "SQLite 持久化完善（订单/成交/仓位）"
status: completed
completed_at: "2026-04-21T00:00:00"
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P0
relations:
  supersedes: []
  conflicts_with: []
  refines: []
  merged_from: []
  refined_by: [REQ-014]
  related_to: [REQ-015]
  cluster: Infrastructure
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "代码审查发现 sqlite_database.rs 中 save_order_data/save_trade_data/save_position_data 是 stub，仅返回 Ok(true) 不持久化；load_orders/load_trades/load_positions 返回空 vec。MainEngine::new_with_sqlite() 创建了数据库连接但实际无法恢复状态。"
    reason: "崩溃恢复功能形同虚设，这是当前最阻塞的问题"
    snapshot: "SQLite 订单/成交/仓位数据的持久化写入和读取，使崩溃恢复真正可用"
---

# SQLite 持久化完善（订单/成交/仓位）

## 描述

当前 `SqliteDatabase` (sqlite_database.rs, 849行) 已实现 `BaseDatabase` trait 并创建数据库文件，但以下关键方法是 stub：
- `save_order_data` → 返回 `Ok(true)`，不持久化
- `save_trade_data` → 返回 `Ok(true)`，不持久化
- `save_position_data` → 返回 `Ok(true)`，不持久化
- `load_orders` → 返回空 vec
- `load_trades` → 返回空 vec
- `load_positions` → 返回空 vec

`MainEngine::new_with_sqlite()` 和 `new_with_sqlite_at()` 已存在（behind `#[cfg(feature = "sqlite")]`），但因为没有实际持久化，崩溃恢复不工作。

## 验收标准

- [ ] 在 `create_tables()` 中添加 `dborderdata`、`dbtradedata`、`dbpositiondata` 三张表
- [ ] 实现 `save_order_data`：INSERT OR REPLACE 写入订单数据（含 post_only/reduce_only）
- [ ] 实现 `save_trade_data`：INSERT OR REPLACE 写入成交数据
- [ ] 实现 `save_position_data`：INSERT OR REPLACE 写入仓位数据
- [ ] 实现 `load_orders`：从数据库读取订单列表
- [ ] 实现 `load_trades`：从数据库读取成交列表
- [ ] 实现 `load_positions`：从数据库读取仓位列表
- [ ] 在 OmsEngine 订单/成交/仓位更新流程中集成 save 调用
- [ ] 在 `new_with_sqlite()` 中集成 restore_from_database 流程
- [ ] 测试：new_with_sqlite → 下单 → 重启 → 恢复状态

## 设计参考

详见 `.sisyphus/plans/development-guide.md` 第三节 "P0-3 SQLite 持久化完善设计"
