---
id: REQ-014
title: "LMDB/Redis 可选持久化后端"
status: completed
created_at: "2026-04-19T00:00:00"
updated_at: "2026-04-19T00:00:00"
priority: P2
relations:
  supersedes: []
  conflicts_with: []
  refines: [REQ-001]
  merged_from: []
  cluster: Infrastructure
versions:
  - version: 1
    date: "2026-04-19T00:00:00"
    author: ai
    context: "plans.md 特性对比发�?nautilus_trader 使用 Redis 持久化，tesser 使用 SQLite + LMDB。用户已删除 parquet_database_p1/p2/p3.rs，表明对 Parquet 后端不感兴趣�?
    reason: "不同场景需要不同持久化后端（低延迟�?LMDB，分布式�?Redis�?
    snapshot: "添加 LMDB �?Redis 可选持久化后端，通过 feature flag 控制，与现有 BaseDatabase trait 兼容"
---

# LMDB/Redis 可选持久化后端

## 描述

当前只有 SQLite 持久化实现（且尚不完整）。不同场景需要不同后端：
- **LMDB**：低延迟、嵌入式、适合单机高频
- **Redis**：分布式、适合多进程部�?
## 验收标准

- [x] `LmdbDatabase` 实现 `BaseDatabase` trait
- [x] `RedisDatabase` 实现 `BaseDatabase` trait
- [x] feature flags: `lmdb` (done), `redis`
- [x] `MainEngine` 支持选择不同后端
- [x] �?SQLite 后端 API 一�?
## 依赖

- `heed` �?`lmdb-rs` crate（可�?feature `lmdb`�?- `redis` crate（可�?feature `redis`�?- **可避�?*：先完善 SQLite 即可满足大部分需�?
## 工作�?
每个后端�?1-2 �?